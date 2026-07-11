use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{
    Agent, AgentConfig, AgentEvent, AgentInput, BusinessBackend, LlmClient,
    LlmRequest, LlmStreamChunk, Message, MessageRole,
};
use seat_agent_tools::business::{ComplaintQueryTool, MockBusinessBackend, OrderQueryTool};
use seat_agent_tools::transfer::TransferToHumanTool;

/// Mock LLM that parses JSON responses and emits proper tool call stream events.
struct JsonToolCallMock {
    responses: Vec<String>,
    idx: std::sync::atomic::AtomicUsize,
}

impl JsonToolCallMock {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmClient for JsonToolCallMock {
    async fn chat_stream(
        &self,
        _req: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>>,
    > {
        let i = self.idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let resp = &self.responses[i % self.responses.len()];

        let parsed: Result<serde_json::Value, _> = serde_json::from_str(resp);

        let chunks: Vec<seat_agent_core::Result<LlmStreamChunk>> = match parsed {
            Ok(serde_json::Value::Object(obj)) if obj.contains_key("tool_calls") => {
                let calls = obj["tool_calls"].as_array().cloned().unwrap_or_default();
                let mut result = Vec::new();
                for call in &calls {
                    let id = call["id"].as_str().unwrap_or("unknown").to_string();
                    let name = call["function"]["name"].as_str().unwrap_or("unknown").to_string();
                    let args = call["function"]["arguments"].as_str().unwrap_or("{}").to_string();
                    result.push(Ok(LlmStreamChunk::ToolCallStart { id, name }));
                    result.push(Ok(LlmStreamChunk::ToolCallDelta { arguments: args }));
                }
                result.push(Ok(LlmStreamChunk::Done {
                    finish_reason: seat_agent_core::FinishReason::ToolCalls,
                }));
                result
            }
            _ => vec![
                Ok(LlmStreamChunk::Content(resp.clone())),
                Ok(LlmStreamChunk::Done {
                    finish_reason: seat_agent_core::FinishReason::Stop,
                }),
            ],
        };

        let stream = tokio_stream::iter(chunks);
        Ok(Box::pin(stream))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend: Arc<dyn BusinessBackend> = Arc::new(MockBusinessBackend::new());

    // 语音模式场景：客户投诉 → LLM 调用 transfer_to_human → 快速转人工
    let mock_llm = JsonToolCallMock::new(vec![
        r#"{"tool_calls":[{"id":"vc1","function":{"name":"transfer_to_human","arguments":"{\"reason\":\"投诉处理\",\"reply\":\"正在为您转接专属客服\"}"}}]}"#.to_string(),
        "已为您转接专属客服，请稍候。".to_string(),
    ]);

    // voice() 工厂方法：max_rounds=2, Modality::Voice
    let config = AgentConfig::voice();
    let max_rounds = config.max_rounds;
    let mut agent = Agent::new(config, Box::new(mock_llm));

    agent.register_tool(Box::new(OrderQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(ComplaintQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(TransferToHumanTool::new()));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    let input = AgentInput {
        session_id: "voice-demo".to_string(),
        message: Message {
            role: MessageRole::User,
            content: "我要投诉！".to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    println!("=== voice_chat 语音客服演示 ===\n");
    println!("客户：{}", input.message.content);
    println!("模式：语音（max_rounds={}，Modality::Voice）\n", max_rounds);

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::StreamStart => {}
            AgentEvent::Token(token) => print!("[语音] {}", token),
            AgentEvent::StreamEnd => println!(),
            AgentEvent::ToolCallStart { tool_name, arguments } => {
                println!("  [工具] {} ({})", tool_name, arguments);
            }
            AgentEvent::ToolCallEnd { tool_name, result } => {
                println!("  [结果] {}: {}", tool_name, result);
            }
            AgentEvent::TransferToHuman { reason } => {
                println!("\n  [转人工] {}", reason);
            }
            AgentEvent::Error(err) => {
                println!("\n  [错误] {}", err);
            }
        }
    }

    handle.await??;
    println!("\n=== 语音演示完成 ===");
    println!("注：语音模式 max_rounds={}，超过此限制将强制结束", max_rounds);

    Ok(())
}
