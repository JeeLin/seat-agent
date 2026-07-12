use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use futures::StreamExt;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use seat_agent_core::{AgentError, FinishReason, LlmClient, LlmRequest, LlmStreamChunk, Result};

/// OpenAI API 客户端
pub struct OpenAiClient {
    client: Client,
    api_key: String,
    base_url: String,
    model: String,
}

/// OpenAI API 请求
#[derive(Debug, Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<OpenAiMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tools: Option<Vec<OpenAiTool>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    max_tokens: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    stream: bool,
}

/// OpenAI 消息
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiMessage {
    role: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<Vec<OpenAiToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
}

/// OpenAI 工具调用
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiToolCall {
    id: String,
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiFunction,
}

/// OpenAI 函数
#[derive(Debug, Serialize, Deserialize)]
struct OpenAiFunction {
    name: String,
    arguments: String,
}

/// OpenAI 工具定义
#[derive(Debug, Serialize)]
struct OpenAiTool {
    #[serde(rename = "type")]
    tool_type: String,
    function: OpenAiToolDefinition,
}

/// OpenAI 函数定义
#[derive(Debug, Serialize)]
struct OpenAiToolDefinition {
    name: String,
    description: String,
    parameters: serde_json::Value,
}

/// SSE 流式响应
#[derive(Debug, Deserialize)]
struct OpenAiStreamResponse {
    choices: Vec<OpenAiStreamChoice>,
}

/// SSE 流式选择
#[derive(Debug, Deserialize)]
struct OpenAiStreamChoice {
    delta: OpenAiStreamDelta,
    finish_reason: Option<String>,
}

/// SSE 流式增量
#[derive(Debug, Deserialize)]
struct OpenAiStreamDelta {
    #[serde(default)]
    content: Option<String>,
    #[serde(default)]
    tool_calls: Option<Vec<OpenAiToolCallDelta>>,
}

/// SSE 流式工具调用增量
#[derive(Debug, Deserialize)]
struct OpenAiToolCallDelta {
    index: usize,
    id: Option<String>,
    function: Option<OpenAiFunctionDelta>,
}

/// SSE 流式函数增量
#[derive(Debug, Deserialize)]
struct OpenAiFunctionDelta {
    name: Option<String>,
    arguments: Option<String>,
}

impl OpenAiClient {
    /// 创建新的 OpenAI 客户端
    pub fn new(api_key: String, base_url: Option<String>, model: Option<String>) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()),
            model: model.unwrap_or_else(|| "gpt-3.5-turbo".to_string()),
        }
    }

    /// 转换 LlmRequest 为 OpenAiRequest
    fn convert_request(&self, request: LlmRequest) -> OpenAiRequest {
        let messages: Vec<OpenAiMessage> = request
            .messages
            .iter()
            .map(|msg| OpenAiMessage {
                role: match msg.role {
                    seat_agent_core::MessageRole::System => "system".to_string(),
                    seat_agent_core::MessageRole::User => "user".to_string(),
                    seat_agent_core::MessageRole::Assistant => "assistant".to_string(),
                    seat_agent_core::MessageRole::Tool => "tool".to_string(),
                },
                content: Some(msg.content.clone()),
                tool_calls: msg.tool_calls.as_ref().map(|calls| {
                    calls
                        .iter()
                        .map(|call| OpenAiToolCall {
                            id: call.id.clone(),
                            tool_type: "function".to_string(),
                            function: OpenAiFunction {
                                name: call.name.clone(),
                                arguments: call.arguments.clone(),
                            },
                        })
                        .collect()
                }),
                tool_call_id: msg.tool_call_id.clone(),
            })
            .collect();

        let tools = if request.tools.is_empty() {
            None
        } else {
            Some(
                request
                    .tools
                    .iter()
                    .map(|tool| OpenAiTool {
                        tool_type: "function".to_string(),
                        function: OpenAiToolDefinition {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            parameters: tool.parameters.clone(),
                        },
                    })
                    .collect(),
            )
        };

        OpenAiRequest {
            model: self.model.clone(),
            messages,
            tools,
            max_tokens: request.max_tokens,
            temperature: request.temperature,
            stream: true,
        }
    }
}

#[async_trait]
impl LlmClient for OpenAiClient {
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamChunk>> + Send>>> {
        let openai_request = self.convert_request(request);
        let url = format!("{}/chat/completions", self.base_url);

        let response = self
            .client
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .header("Content-Type", "application/json")
            .json(&openai_request)
            .send()
            .await
            .map_err(|e| AgentError::Llm(format!("Request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(AgentError::Llm(format!(
                "API error {}: {}",
                status, body
            )));
        }

        let (tx, rx) = mpsc::channel(100);

        tokio::spawn(async move {
            let mut buffer = String::new();
            let mut tool_calls: Vec<(
                usize,
                Option<String>,
                Option<String>,
                Option<String>,
            )> = Vec::new();

            let mut stream = response.bytes_stream();

            while let Some(chunk_result) = stream.next().await {
                let chunk = match chunk_result {
                    Ok(c) => c,
                    Err(e) => {
                        let _ = tx
                            .send(Err(AgentError::Llm(format!("Stream error: {}", e))))
                            .await;
                        return;
                    }
                };

                buffer.push_str(&String::from_utf8_lossy(&chunk));

                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line.is_empty() || !line.starts_with("data: ") {
                        continue;
                    }

                    let data = &line[6..];
                    if data == "[DONE]" {
                        // 处理累积的 tool_calls
                        for (index, id, name, arguments) in &tool_calls {
                            let id = id.clone().unwrap_or_default();
                            let name = name.clone().unwrap_or_default();
                            let arguments = arguments.clone().unwrap_or_default();

                            let _ = tx
                                .send(Ok(LlmStreamChunk::ToolCallStart {
                                    id,
                                    name,
                                }))
                                .await;

                            let _ = tx
                                .send(Ok(LlmStreamChunk::ToolCallDelta {
                                    arguments,
                                }))
                                .await;
                        }

                        let _ = tx
                            .send(Ok(LlmStreamChunk::Done {
                                finish_reason: FinishReason::Stop,
                            }))
                            .await;
                        return;
                    }

                    match serde_json::from_str::<OpenAiStreamResponse>(data) {
                        Ok(response) => {
                            for choice in &response.choices {
                                if let Some(content) = &choice.delta.content {
                                    let _ = tx
                                        .send(Ok(LlmStreamChunk::Content(content.clone())))
                                        .await;
                                }

                                if let Some(calls) = &choice.delta.tool_calls {
                                    for call in calls {
                                        // 确保 tool_calls 向量足够大
                                        while tool_calls.len() <= call.index {
                                            tool_calls.push((tool_calls.len(), None, None, None));
                                        }

                                        if let Some(id) = &call.id {
                                            tool_calls[call.index].1 = Some(id.clone());
                                        }

                                        if let Some(func) = &call.function {
                                            if let Some(name) = &func.name {
                                                tool_calls[call.index].2 = Some(name.clone());
                                            }
                                            if let Some(args) = &func.arguments {
                                                tool_calls[call.index].3 =
                                                    Some(args.clone());
                                            }
                                        }
                                    }
                                }

                                if let Some(finish_reason) = &choice.finish_reason {
                                    match finish_reason.as_str() {
                                        "length" => {
                                            let _ = tx
                                                .send(Ok(LlmStreamChunk::Done {
                                                    finish_reason: FinishReason::Length,
                                                }))
                                                .await;
                                        }
                                        "tool_calls" => {
                                            // tool_calls 在 [DONE] 时处理
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Err(_) => {
                            // 忽略解析错误
                        }
                    }
                }
            }
        });

        Ok(Box::pin(ReceiverStream::new(rx)))
    }
}
