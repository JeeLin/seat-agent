use seat_agent_core::{Agent, AgentConfig, AgentEvent, AgentInput, MockLlmClient};

/// 简单的问候工具
struct GreetTool;

#[async_trait::async_trait]
impl seat_agent_core::Tool for GreetTool {
    fn definition(&self) -> seat_agent_core::ToolDefinition {
        seat_agent_core::ToolDefinition {
            name: "greet".to_string(),
            description: "向用户打招呼".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "用户的名字"
                    }
                },
                "required": ["name"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> seat_agent_core::Result<String> {
        let name = args["name"].as_str().unwrap_or("World");
        Ok(format!("你好，{}！", name))
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 创建 MockLlmClient，预设响应
    let mock_llm = MockLlmClient::new(vec![
        // 第一次响应：调用 greet 工具
        r#"{"tool_calls":[{"id":"call_1","function":{"name":"greet","arguments":"{\"name\":\"Alice\"}"}}]}"#.to_string(),
        // 第二次响应：最终回复
        "你好，Alice！今天有什么可以帮你的吗？".to_string(),
    ]);

    // 创建 Agent
    let config = AgentConfig::default();
    let mut agent = Agent::new(config, Box::new(mock_llm));

    // 注册工具
    agent.register_tool(Box::new(GreetTool));

    // 创建输出 channel
    let (tx, mut rx) = tokio::sync::mpsc::channel(100);

    // 发送消息
    let input = AgentInput {
        session_id: "test-session".to_string(),
        message: seat_agent_core::Message {
            role: seat_agent_core::MessageRole::User,
            content: "你好".to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    // 启动 Agent 处理
    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });

    // 接收并打印输出
    println!("=== Agent 输出 ===\n");
    while let Some(event) = rx.recv().await {
        match event {
            AgentEvent::StreamStart => println!("[流开始]"),
            AgentEvent::Token(token) => print!("{}", token),
            AgentEvent::StreamEnd => println!("\n[流结束]"),
            AgentEvent::ToolCallStart {
                tool_name,
                arguments,
            } => {
                println!("[工具调用] {} ({})", tool_name, arguments);
            }
            AgentEvent::ToolCallEnd { tool_name, result } => {
                println!("[工具结果] {}: {}", tool_name, result);
            }
            AgentEvent::TransferToHuman { reason } => {
                println!("[转人工] {}", reason);
            }
            AgentEvent::Error(err) => {
                println!("[错误] {}", err);
            }
        }
    }

    // 等待 Agent 完成
    handle.await??;
    println!("\n=== 完成 ===");

    Ok(())
}
