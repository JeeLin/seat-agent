//! OpenAI 兼容的流式 LLM 客户端
//!
//! 支持所有 OpenAI API 兼容的服务（OpenAI、DeepSeek、Moonshot 等）。
//! 通过环境变量或构造参数配置。

use std::pin::Pin;

use async_trait::async_trait;
use futures::StreamExt;
use seat_agent_core::{
    AgentError, FinishReason, LlmClient, LlmRequest, LlmStreamChunk, MessageRole, Result,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// OpenAI 兼容的流式 LLM 客户端
pub struct OpenAiLlmClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

impl OpenAiLlmClient {
    pub fn new(api_key: &str, base_url: &str, model: &str) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key: api_key.to_string(),
            base_url: base_url.trim_end_matches('/').to_string(),
            model: model.to_string(),
        }
    }

    /// 从环境变量构造，支持 LLM_API_KEY / OPENAI_API_KEY 等
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("LLM_API_KEY")
            .or_else(|_| std::env::var("OPENAI_API_KEY"))
            .map_err(|_| AgentError::Config("未设置 LLM_API_KEY 或 OPENAI_API_KEY".into()))?;

        let base_url = std::env::var("LLM_BASE_URL")
            .or_else(|_| std::env::var("OPENAI_BASE_URL"))
            .unwrap_or_else(|_| "https://api.openai.com/v1".into());

        let model = std::env::var("LLM_MODEL").unwrap_or_else(|_| "gpt-4o-mini".into());

        Ok(Self::new(&api_key, &base_url, &model))
    }

    pub fn model_name(&self) -> &str {
        &self.model
    }
}

#[async_trait]
impl LlmClient for OpenAiLlmClient {
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<Pin<Box<dyn futures::Stream<Item = Result<LlmStreamChunk>> + Send>>> {
        let messages: Vec<serde_json::Value> = request
            .messages
            .iter()
            .map(|m| {
                let mut obj = serde_json::json!({
                    "role": match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                        MessageRole::Tool => "tool",
                    },
                    "content": m.content,
                });
                if let Some(tc) = &m.tool_calls {
                    obj["tool_calls"] = serde_json::to_value(tc).unwrap_or_default();
                }
                if let Some(id) = &m.tool_call_id {
                    obj["tool_call_id"] = serde_json::Value::String(id.clone());
                }
                obj
            })
            .collect();

        let mut body = serde_json::json!({
            "model": self.model,
            "messages": messages,
            "stream": true,
        });

        if !request.tools.is_empty() {
            let tools: Vec<serde_json::Value> = request
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "type": "function",
                        "function": {
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.parameters,
                        }
                    })
                })
                .collect();
            body["tools"] = serde_json::Value::Array(tools);
        }

        let response = self
            .client
            .post(format!("{}/chat/completions", self.base_url))
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| AgentError::Llm(format!("请求失败: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AgentError::Llm(format!("HTTP {} {}", status, text)));
        }

        let stream = response.bytes_stream();
        let (tx, rx) = mpsc::channel(128);

        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut stream = std::pin::pin!(stream);

            while let Some(chunk_result) = stream.next().await {
                let bytes = match chunk_result {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx
                            .send(Err(AgentError::Llm(format!("流读取错误: {}", e))))
                            .await;
                        break;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&bytes));

                while let Some(pos) = buffer.find('\n') {
                    let line = buffer[..pos].trim().to_string();
                    buffer = buffer[pos + 1..].to_string();

                    if line.is_empty() || line.starts_with(':') {
                        continue;
                    }

                    if let Some(data) = line.strip_prefix("data: ") {
                        let data = data.trim();
                        if data == "[DONE]" {
                            let _ = tx
                                .send(Ok(LlmStreamChunk::Done {
                                    finish_reason: FinishReason::Stop,
                                }))
                                .await;
                            return;
                        }

                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(delta) = parsed
                                .get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("delta"))
                            {
                                // 文本内容
                                if let Some(content) =
                                    delta.get("content").and_then(|c| c.as_str())
                                {
                                    if !content.is_empty() {
                                        let _ = tx
                                            .send(Ok(LlmStreamChunk::Content(
                                                content.to_string(),
                                            )))
                                            .await;
                                    }
                                }

                                // 工具调用
                                if let Some(tool_calls) =
                                    delta.get("tool_calls").and_then(|tc| tc.as_array())
                                {
                                    for tc in tool_calls {
                                        let idx = tc
                                            .get("index")
                                            .and_then(|i| i.as_u64())
                                            .unwrap_or(0);
                                        if let Some(fn_obj) = tc.get("function") {
                                            if let Some(name) =
                                                fn_obj.get("name").and_then(|n| n.as_str())
                                            {
                                                let _ = tx
                                                    .send(Ok(LlmStreamChunk::ToolCallStart {
                                                        id: format!("call_{}", idx),
                                                        name: name.to_string(),
                                                    }))
                                                    .await;
                                            }
                                            if let Some(args) = fn_obj
                                                .get("arguments")
                                                .and_then(|a| a.as_str())
                                            {
                                                let _ = tx
                                                    .send(Ok(LlmStreamChunk::ToolCallDelta {
                                                        arguments: args.to_string(),
                                                    }))
                                                    .await;
                                            }
                                        }
                                    }
                                }
                            }

                            // finish_reason
                            if let Some(fr) = parsed
                                .get("choices")
                                .and_then(|c| c.get(0))
                                .and_then(|c| c.get("finish_reason"))
                                .and_then(|fr| fr.as_str())
                            {
                                let reason = match fr {
                                    "stop" => FinishReason::Stop,
                                    "length" => FinishReason::Length,
                                    "tool_calls" => FinishReason::ToolCalls,
                                    _ => FinishReason::Stop,
                                };
                                let _ = tx
                                    .send(Ok(LlmStreamChunk::Done {
                                        finish_reason: reason,
                                    }))
                                    .await;
                                return;
                            }
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}
