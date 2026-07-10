use std::sync::Arc;

use futures::StreamExt;
use tokio::sync::mpsc;

use crate::config::AgentConfig;
use crate::context::{AgentEvent, AgentInput, Context};
use crate::error::{AgentError, Result};
use crate::traits::{KnowledgeStore, LlmClient, LlmRequest, LlmStreamChunk, MemoryStore, Tool};

/// Agent 主循环
pub struct Agent {
    config: AgentConfig,
    llm: Arc<dyn LlmClient>,
    tools: Vec<Box<dyn Tool>>,
    knowledge: Option<Box<dyn KnowledgeStore>>,
    memory: Option<Box<dyn MemoryStore>>,
}

impl Agent {
    /// 创建新的 Agent
    pub fn new(config: AgentConfig, llm: Box<dyn LlmClient>) -> Self {
        Self {
            config,
            llm: Arc::from(llm),
            tools: Vec::new(),
            knowledge: None,
            memory: None,
        }
    }

    /// 注册工具
    pub fn register_tool(&mut self, tool: Box<dyn Tool>) {
        self.tools.push(tool);
    }

    /// 设置知识库
    pub fn set_knowledge(&mut self, knowledge: Box<dyn KnowledgeStore>) {
        self.knowledge = Some(knowledge);
    }

    /// 设置记忆存储
    pub fn set_memory(&mut self, memory: Box<dyn MemoryStore>) {
        self.memory = Some(memory);
    }

    /// 处理消息
    pub async fn on_message(
        &self,
        input: AgentInput,
        output_tx: mpsc::Sender<AgentEvent>,
    ) -> Result<()> {
        let mut context = Context::new(input.session_id.clone(), self.config.clone());

        // 添加用户消息
        context.add_user_message(input.message.content.clone());

        // 预检索（如果配置了知识库）
        if let Some(knowledge) = &self.knowledge {
            let results = knowledge.search(&input.message.content, 3).await?;
            context.set_retrieval(results);
        }

        // Agent Loop
        let mut round = 0;
        let start_time = std::time::Instant::now();

        loop {
            // 检查限制
            if round >= self.config.max_rounds {
                tracing::warn!(round, "Max rounds exceeded");
                break;
            }

            if start_time.elapsed() >= self.config.max_duration {
                tracing::warn!("Max duration exceeded");
                break;
            }

            // 发送 StreamStart
            output_tx
                .send(AgentEvent::StreamStart)
                .await
                .map_err(|_| AgentError::Internal("Channel closed".into()))?;

            // 构建 LLM 请求
            let messages = context.build_messages();
            let tool_definitions: Vec<_> = self.tools.iter().map(|t| t.definition()).collect();

            let request = LlmRequest {
                messages,
                tools: tool_definitions,
                max_tokens: Some(self.config.max_output_tokens),
                temperature: Some(0.7),
                stream: true,
            };

            // 调用 LLM
            let mut stream = self.llm.chat_stream(request).await?;

            let mut content = String::new();
            let mut tool_calls: Vec<(String, String, String)> = Vec::new(); // (id, name, arguments)
            let mut current_tool_call: Option<(String, String, String)> = None;

            while let Some(chunk) = stream.next().await {
                match chunk? {
                    LlmStreamChunk::Content(text) => {
                        content.push_str(&text);
                        output_tx
                            .send(AgentEvent::Token(text))
                            .await
                            .map_err(|_| AgentError::Internal("Channel closed".into()))?;
                    }
                    LlmStreamChunk::ToolCallStart { id, name } => {
                        current_tool_call = Some((id, name, String::new()));
                    }
                    LlmStreamChunk::ToolCallDelta { arguments } => {
                        if let Some((_, _, ref mut args)) = current_tool_call {
                            args.push_str(&arguments);
                        }
                    }
                    LlmStreamChunk::Done { .. } => {
                        if let Some(tool_call) = current_tool_call.take() {
                            tool_calls.push(tool_call);
                        }
                    }
                    LlmStreamChunk::Error(e) => {
                        output_tx
                            .send(AgentEvent::Error(e))
                            .await
                            .map_err(|_| AgentError::Internal("Channel closed".into()))?;
                        return Ok(());
                    }
                }
            }

            // 发送 StreamEnd
            output_tx
                .send(AgentEvent::StreamEnd)
                .await
                .map_err(|_| AgentError::Internal("Channel closed".into()))?;

            // 检查是否有工具调用
            let tool_calls_struct: Option<Vec<crate::traits::ToolCall>> = if tool_calls.is_empty() {
                // 没有工具调用，回复完成
                context.add_assistant_message(content, None);
                break;
            } else {
                Some(
                    tool_calls
                        .iter()
                        .map(|(id, name, args)| crate::traits::ToolCall {
                            id: id.clone(),
                            name: name.clone(),
                            arguments: args.clone(),
                        })
                        .collect(),
                )
            };

            // 有工具调用，执行工具
            context.add_assistant_message(content, tool_calls_struct);

            for (tool_call_id, tool_name, arguments) in &tool_calls {
                // 查找工具
                let tool = self
                    .tools
                    .iter()
                    .find(|t| t.definition().name == *tool_name);

                let result = match tool {
                    Some(tool) => {
                        // 发送 ToolCallStart
                        output_tx
                            .send(AgentEvent::ToolCallStart {
                                tool_name: tool_name.clone(),
                                arguments: arguments.clone(),
                            })
                            .await
                            .map_err(|_| AgentError::Internal("Channel closed".into()))?;

                        // 执行工具
                        let args: serde_json::Value = serde_json::from_str(arguments)?;
                        match tool.execute(args).await {
                            Ok(result) => {
                                // 发送 ToolCallEnd
                                output_tx
                                    .send(AgentEvent::ToolCallEnd {
                                        tool_name: tool_name.clone(),
                                        result: result.clone(),
                                    })
                                    .await
                                    .map_err(|_| AgentError::Internal("Channel closed".into()))?;

                                result
                            }
                            Err(e) => {
                                let error_msg = format!("工具执行失败: {}", e);
                                output_tx
                                    .send(AgentEvent::ToolCallEnd {
                                        tool_name: tool_name.clone(),
                                        result: error_msg.clone(),
                                    })
                                    .await
                                    .map_err(|_| AgentError::Internal("Channel closed".into()))?;

                                error_msg
                            }
                        }
                    }
                    None => {
                        let error_msg = format!("工具不存在: {}", tool_name);
                        output_tx
                            .send(AgentEvent::ToolCallEnd {
                                tool_name: tool_name.clone(),
                                result: error_msg.clone(),
                            })
                            .await
                            .map_err(|_| AgentError::Internal("Channel closed".into()))?;

                        error_msg
                    }
                };

                // 添加工具结果到工作区
                context.add_tool_result(tool_call_id.clone(), result);
            }

            // 将工作区内容移动到历史，供下一轮 LLM 查看
            context.flush_working_to_history();
            round += 1;
        }

        Ok(())
    }
}
