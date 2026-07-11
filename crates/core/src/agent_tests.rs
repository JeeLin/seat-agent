use crate::config::AgentConfig;
use crate::context::{AgentEvent, AgentInput, Message};
use crate::error::AgentError;
use crate::mock::MockLlmClient;
use crate::traits::{
    FinishReason, LlmClient, LlmMessage, LlmRequest, LlmStreamChunk, MessageRole, Tool,
    ToolDefinition,
};
use crate::Agent;

// ============================================================================
// Test helpers
// ============================================================================

/// A mock LLM client that can emit tool call events.
struct ToolCallMockLlmClient {
    /// Each item is a sequence of stream chunks for one `chat_stream` call.
    responses: Vec<Vec<LlmStreamChunk>>,
    index: std::sync::atomic::AtomicUsize,
}

impl ToolCallMockLlmClient {
    fn new(responses: Vec<Vec<LlmStreamChunk>>) -> Self {
        Self {
            responses,
            index: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl LlmClient for ToolCallMockLlmClient {
    async fn chat_stream(
        &self,
        _request: LlmRequest,
    ) -> crate::Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = crate::Result<LlmStreamChunk>> + Send,
            >,
        >,
    > {
        let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let response_idx = idx % self.responses.len();
        let chunks = self.responses[response_idx].clone();

        let stream = futures::stream::iter(chunks.into_iter().map(Ok));
        Ok(Box::pin(stream))
    }
}

/// A simple echo tool for testing.
struct EchoTool;

#[async_trait::async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "Echoes input".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string" }
                },
                "required": ["message"]
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> crate::Result<String> {
        let msg = args["message"].as_str().unwrap_or("no message");
        Ok(format!("Echo: {}", msg))
    }
}

/// A tool that always fails.
struct FailTool;

#[async_trait::async_trait]
impl Tool for FailTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fail".to_string(),
            description: "Always fails".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    async fn execute(&self, _args: serde_json::Value) -> crate::Result<String> {
        Err(AgentError::Tool("intentional failure".into()))
    }
}

fn make_user_input(content: &str) -> AgentInput {
    AgentInput {
        session_id: "test-session".to_string(),
        message: Message {
            role: MessageRole::User,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        },
    }
}

async fn collect_events(rx: &mut tokio::sync::mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }
    events
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn test_agent_new_creates_instance() {
    let config = AgentConfig::default();
    let llm = Box::new(MockLlmClient::new(vec!["hello".to_string()]));
    let _agent = Agent::new(config, llm);
    // Agent created successfully — no panic means success
}

#[tokio::test]
async fn test_register_tool() {
    let config = AgentConfig::default();
    let llm = Box::new(MockLlmClient::new(vec!["hello".to_string()]));
    let mut agent = Agent::new(config, llm);
    agent.register_tool(Box::new(EchoTool));
    agent.register_tool(Box::new(FailTool));
    // Two tools registered — no panic means success
}

#[tokio::test]
async fn test_on_message_direct_reply() {
    let config = AgentConfig::default();
    let llm = Box::new(MockLlmClient::new(vec!["Hello there!".to_string()]));
    let agent = Agent::new(config, llm);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("hi");

    agent.on_message(input, tx).await.unwrap();
    let events = collect_events(&mut rx).await;

    // Should have: StreamStart, Token("Hello there!"), StreamEnd
    assert!(matches!(events.first(), Some(AgentEvent::StreamStart)));
    assert!(
        matches!(&events[1], AgentEvent::Token(t) if t == "Hello there!")
    );
    assert!(matches!(events.last(), Some(AgentEvent::StreamEnd)));
}

#[tokio::test]
async fn test_on_message_two_round_tool_call_and_reply() {
    // Round 1: tool call → tool executed → round 2: final reply
    let tool_call_chunks = vec![
        LlmStreamChunk::ToolCallStart {
            id: "call_1".to_string(),
            name: "echo".to_string(),
        },
        LlmStreamChunk::ToolCallDelta {
            arguments: r#"{"message":"ping"}"#.to_string(),
        },
        LlmStreamChunk::Done {
            finish_reason: FinishReason::ToolCalls,
        },
    ];
    let final_chunks = vec![
        LlmStreamChunk::Content("pong".to_string()),
        LlmStreamChunk::Done {
            finish_reason: FinishReason::Stop,
        },
    ];
    let llm = Box::new(ToolCallMockLlmClient::new(vec![
        tool_call_chunks,
        final_chunks,
    ]));
    let mut agent = Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("ping");
    agent.on_message(input, tx).await.unwrap();
    let events = collect_events(&mut rx).await;

    let tokens: Vec<&str> = events
        .iter()
        .filter_map(|e| if let AgentEvent::Token(t) = e { Some(t.as_str()) } else { None })
        .collect();
    let tool_results: Vec<&str> = events
        .iter()
        .filter_map(|e| {
            if let AgentEvent::ToolCallEnd { result, .. } = e { Some(result.as_str()) } else { None }
        })
        .collect();
    assert!(
        tool_results.iter().any(|r| r.contains("Echo: ping")),
        "Expected tool result 'Echo: ping', got: {:?}",
        tool_results
    );
    assert!(
        tokens.contains(&"pong"),
        "Expected final reply 'pong', got tokens: {:?}",
        tokens
    );
}

#[tokio::test]
async fn test_on_message_tool_call_then_reply() {
    // First response: tool call via stream events
    // Second response: direct text reply
    let tool_call_chunks = vec![
        LlmStreamChunk::ToolCallStart {
            id: "call_1".to_string(),
            name: "echo".to_string(),
        },
        LlmStreamChunk::ToolCallDelta {
            arguments: r#"{"message":"hi"}"#.to_string(),
        },
        LlmStreamChunk::Done {
            finish_reason: FinishReason::ToolCalls,
        },
    ];
    let final_reply_chunks = vec![
        LlmStreamChunk::Content("Done!".to_string()),
        LlmStreamChunk::Done {
            finish_reason: FinishReason::Stop,
        },
    ];

    let llm = Box::new(ToolCallMockLlmClient::new(vec![
        tool_call_chunks,
        final_reply_chunks,
    ]));
    let mut agent = Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("hello");

    agent.on_message(input, tx).await.unwrap();
    let events = collect_events(&mut rx).await;

    // Verify event sequence
    let event_names: Vec<String> = events
        .iter()
        .map(|e| match e {
            AgentEvent::StreamStart => "StreamStart".into(),
            AgentEvent::Token(t) => format!("Token({})", t),
            AgentEvent::StreamEnd => "StreamEnd".into(),
            AgentEvent::ToolCallStart { tool_name, .. } => {
                format!("ToolCallStart({})", tool_name)
            }
            AgentEvent::ToolCallEnd { tool_name, result } => {
                format!("ToolCallEnd({}: {})", tool_name, result)
            }
            AgentEvent::Error(e) => format!("Error({})", e),
            AgentEvent::TransferToHuman { reason } => format!("Transfer({})", reason),
        })
        .collect();

    assert!(
        event_names.contains(&"ToolCallStart(echo)".to_string()),
        "Expected ToolCallStart(echo), got: {:?}",
        event_names
    );
    assert!(
        event_names
            .iter()
            .any(|n| n.starts_with("ToolCallEnd(echo: Echo:")),
        "Expected ToolCallEnd with echo result, got: {:?}",
        event_names
    );
    assert!(
        event_names.contains(&"Token(Done!)".to_string()),
        "Expected Token(Done!), got: {:?}",
        event_names
    );
}

#[tokio::test]
async fn test_on_message_error_propagation() {
    let llm = Box::new(MockLlmClient::new(vec![]).with_error());
    let agent = Agent::new(AgentConfig::default(), llm);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("test");

    // LLM error propagates via `?` from chat_stream through on_message
    let result = agent.on_message(input, tx).await;
    assert!(result.is_err(), "Expected on_message to return Err on LLM failure");
    let err_msg = format!("{}", result.unwrap_err());
    assert!(err_msg.contains("Mock error"), "Expected mock error message: {}", err_msg);
}

#[tokio::test]
async fn test_on_message_tool_execution_error() {
    // Tool call for a tool that always fails
    let tool_call_chunks = vec![
        LlmStreamChunk::ToolCallStart {
            id: "call_1".to_string(),
            name: "fail".to_string(),
        },
        LlmStreamChunk::ToolCallDelta {
            arguments: "{}".to_string(),
        },
        LlmStreamChunk::Done {
            finish_reason: FinishReason::ToolCalls,
        },
    ];
    let final_reply_chunks = vec![
        LlmStreamChunk::Content("After failure".to_string()),
        LlmStreamChunk::Done {
            finish_reason: FinishReason::Stop,
        },
    ];

    let llm = Box::new(ToolCallMockLlmClient::new(vec![
        tool_call_chunks,
        final_reply_chunks,
    ]));
    let mut agent = Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(FailTool));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("do it");

    agent.on_message(input, tx).await.unwrap();
    let events = collect_events(&mut rx).await;

    // Tool call end should contain the error message
    let tool_end_events: Vec<&AgentEvent> = events
        .iter()
        .filter(|e| matches!(e, AgentEvent::ToolCallEnd { .. }))
        .collect();
    assert_eq!(tool_end_events.len(), 1);
    if let AgentEvent::ToolCallEnd { result, .. } = tool_end_events[0] {
        assert!(
            result.contains("failure"),
            "Expected error in result, got: {}",
            result
        );
    }
}

#[tokio::test]
async fn test_on_message_unknown_tool() {
    // Tool call for a tool that isn't registered
    let tool_call_chunks = vec![
        LlmStreamChunk::ToolCallStart {
            id: "call_1".to_string(),
            name: "nonexistent".to_string(),
        },
        LlmStreamChunk::ToolCallDelta {
            arguments: "{}".to_string(),
        },
        LlmStreamChunk::Done {
            finish_reason: FinishReason::ToolCalls,
        },
    ];
    let final_reply_chunks = vec![
        LlmStreamChunk::Content("ok".to_string()),
        LlmStreamChunk::Done {
            finish_reason: FinishReason::Stop,
        },
    ];

    let llm = Box::new(ToolCallMockLlmClient::new(vec![
        tool_call_chunks,
        final_reply_chunks,
    ]));
    let agent = Agent::new(AgentConfig::default(), llm);

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = make_user_input("test");

    agent.on_message(input, tx).await.unwrap();
    let events = collect_events(&mut rx).await;

    // Should have ToolCallEnd with "工具不存在" message
    let tool_end = events.iter().find(|e| {
        matches!(e, AgentEvent::ToolCallEnd { tool_name, .. } if tool_name == "nonexistent")
    });
    assert!(tool_end.is_some(), "Expected ToolCallEnd for nonexistent tool");
}
