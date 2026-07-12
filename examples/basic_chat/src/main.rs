use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{
    Agent, AgentConfig, AgentEvent, AgentInput, BusinessBackend, LlmClient, LlmRequest,
    LlmStreamChunk, Message, MessageRole,
};
use seat_agent_tools::business::{
    ComplaintQueryTool, MockBusinessBackend, OrderQueryTool, RefundQueryTool,
};
use seat_agent_tools::llm::OpenAiLlmClient;
use seat_agent_tools::transfer::TransferToHumanTool;

// ============================================================================
// Mock LLM（无 API key 时使用）
// ============================================================================

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
        std::pin::Pin<
            Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>,
        >,
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
                    let name = call["function"]["name"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let args = call["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_string();
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
        Ok(Box::pin(tokio_stream::iter(chunks)))
    }
}

// ============================================================================
// 桥接 Arc<dyn LlmClient> → Box<dyn LlmClient>
// ============================================================================

struct LlmBridge(Arc<dyn LlmClient>);

#[async_trait]
impl LlmClient for LlmBridge {
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>,
        >,
    > {
        self.0.chat_stream(request).await
    }
}

// ============================================================================
// Main
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend: Arc<dyn BusinessBackend> = Arc::new(MockBusinessBackend::new());

    // 检查是否有 LLM_API_KEY → 真实对话；否则用 Mock 演示
    let use_real_llm = std::env::var("LLM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .is_ok();

    let config = AgentConfig::default();
    let mut agent = if use_real_llm {
        let llm = OpenAiLlmClient::from_env()?;
        println!("✅ 使用真实 LLM: {}", llm.model_name());
        println!("   设置 LLM_BASE_URL 可切换服务（默认 OpenAI）\n");
        Agent::new(config, Box::new(LlmBridge(Arc::new(llm))))
    } else {
        println!("ℹ️  未检测到 LLM_API_KEY，使用 Mock 演示模式");
        println!("   设置 LLM_API_KEY=your-key 即可接入真实 LLM\n");
        let mock_llm = JsonToolCallMock::new(vec![
            r#"{"tool_calls":[{"id":"call_1","function":{"name":"order_query","arguments":"{\"order_id\":\"20240308001\"}"}}]}"#.to_string(),
            "您的订单 20240308001 状态为「已发货」，金额 ¥299.00。请问还需要什么帮助？".to_string(),
            r#"{"tool_calls":[{"id":"call_2","function":{"name":"refund_query","arguments":"{\"refund_id\":\"RF20240310001\"}"}}]}"#.to_string(),
            "您的退款单 RF20240310001 状态为「处理中」，预计 3-5 个工作日到账。".to_string(),
        ]);
        Agent::new(config, Box::new(mock_llm))
    };

    // 注册全部业务工具
    agent.register_tool(Box::new(OrderQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(RefundQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(ComplaintQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(TransferToHumanTool::new()));

    let (tx, mut rx) = tokio::sync::mpsc::channel(200);

    let input = AgentInput {
        session_id: "demo-session".to_string(),
        customer_id: "demo-customer".to_string(),
        message: Message {
            role: MessageRole::User,
            content: "我想查一下我的订单 20240308001".to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    println!("=== seat-agent 客服接待演示 ===\n");
    println!("客户：{}", input.message.content);
    println!();

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });

    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::StreamStart => {}
            AgentEvent::Token(token) => print!("{}", token),
            AgentEvent::StreamEnd => println!(),
            AgentEvent::ToolCallStart {
                tool_name,
                arguments,
            } => {
                println!("  [调用工具] {} ({})", tool_name, arguments);
            }
            AgentEvent::ToolCallEnd { tool_name, result } => {
                println!("  [工具结果] {}: {}", tool_name, result);
                println!();
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
    println!("\n=== 演示完成 ===");

    Ok(())
}
