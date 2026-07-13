use std::io::{self, Write};
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
// 桥接 Arc → Box
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
// Main — 交互式循环
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend: Arc<dyn BusinessBackend> = Arc::new(MockBusinessBackend::new());

    let use_real_llm = std::env::var("LLM_API_KEY")
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .is_ok();

    let config = AgentConfig::default();

    let agent = if use_real_llm {
        let llm = OpenAiLlmClient::from_env()?;
        println!("=== seat-agent 客服系统 ===");
        println!("LLM: {} (真实对话模式)", llm.model_name());
        println!("输入消息开始对话，输入 quit/exit 退出\n");
        Agent::new(config, Box::new(LlmBridge(Arc::new(llm))))
    } else {
        println!("=== seat-agent 客服系统 ===");
        println!("LLM: Mock 演示模式（设置 LLM_API_KEY 可接入真实 LLM）");
        println!("输入消息开始对话，输入 quit/exit 退出\n");
        let mock_llm = JsonToolCallMock::new(vec![
            r#"{"tool_calls":[{"id":"call_1","function":{"name":"order_query","arguments":"{\"order_id\":\"20240308001\"}"}}]}"#.to_string(),
            "您的订单 20240308001 状态为「已发货」，金额 ¥299.00。请问还需要什么帮助？".to_string(),
            r#"{"tool_calls":[{"id":"call_2","function":{"name":"refund_query","arguments":"{\"refund_id\":\"RF20240310001\"}"}}]}"#.to_string(),
            "您的退款单 RF20240310001 状态为「处理中」，预计 3-5 个工作日到账。".to_string(),
        ]);
        Agent::new(config, Box::new(mock_llm))
    };

    let mut agent = agent;
    agent.register_tool(Box::new(OrderQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(RefundQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(ComplaintQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(TransferToHumanTool::new()));

    let agent = Arc::new(agent);
    let session_id = "demo-session".to_string();
    let customer_id = "demo-customer".to_string();

    loop {
        print!("你: ");
        io::stdout().flush()?;

        let mut line = String::new();
        io::stdin().read_line(&mut line)?;
        let text = line.trim().to_string();

        if text.is_empty() {
            continue;
        }
        if text == "quit" || text == "exit" {
            println!("再见！");
            break;
        }

        let (tx, mut rx) = tokio::sync::mpsc::channel(200);

        let input = AgentInput {
            session_id: session_id.clone(),
            customer_id: customer_id.clone(),
            message: Message {
                role: MessageRole::User,
                content: text,
                tool_calls: None,
                tool_call_id: None,
            },
        };

        print!("客服: ");
        io::stdout().flush()?;

        let agent_clone = agent.clone();
        let handle = tokio::spawn(async move { agent_clone.on_message(input, tx).await });

        // 实时读取流式事件
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::StreamStart => {}
                AgentEvent::Token(token) => {
                    print!("{}", token);
                    io::stdout().flush()?;
                }
                AgentEvent::StreamEnd => println!(),
                AgentEvent::ToolCallStart {
                    tool_name,
                    arguments,
                } => {
                    println!("\n  [调用工具] {} ({})", tool_name, arguments);
                }
                AgentEvent::ToolCallEnd { tool_name, result } => {
                    println!("  [工具结果] {}: {}", tool_name, result);
                    print!("客服: ");
                    io::stdout().flush()?;
                }
                AgentEvent::TransferToHuman { reason } => {
                    println!("\n  [转人工] {}", reason);
                }
                AgentEvent::Error(err) => {
                    println!("\n  [错误] {}", err);
                }
            }
        }

        if let Err(e) = handle.await? {
            println!("\n  [错误] {}", e);
        }
        println!();
    }

    Ok(())
}
