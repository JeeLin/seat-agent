//! Integration tests for Agent + Tools cross-crate flow.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use futures::StreamExt;
use seat_agent_core::{
    AgentConfig, AgentEvent, AgentInput, LlmClient, LlmMessage, LlmRequest, LlmStreamChunk,
    Message, MessageRole, Modality, Tool, ToolCall, ToolDefinition,
};
use tokio::sync::mpsc;

// ============================================================================
// Test helpers
// ============================================================================

/// Mock LLM client that emits tool call stream events.
struct ToolCallMock {
    responses: Vec<Vec<LlmStreamChunk>>,
    idx: AtomicUsize,
}

impl ToolCallMock {
    fn new(responses: Vec<Vec<LlmStreamChunk>>) -> Self {
        Self {
            responses,
            idx: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmClient for ToolCallMock {
    async fn chat_stream(
        &self,
        _req: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>>,
    > {
        let i = self.idx.fetch_add(1, Ordering::SeqCst);
        let chunks = self.responses[i % self.responses.len()].clone();
        Ok(Box::pin(futures::stream::iter(chunks.into_iter().map(Ok))))
    }
}

/// Mock LLM that always returns an error.
struct ErrorMock;

#[async_trait]
impl LlmClient for ErrorMock {
    async fn chat_stream(
        &self,
        _req: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>>,
    > {
        Err(seat_agent_core::AgentError::Llm("LLM unavailable".into()))
    }
}

/// Simple echo tool.
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".into(),
            description: "Echoes input".into(),
            parameters: serde_json::json!({ "type": "object" }),
        }
    }
    async fn execute(&self, args: serde_json::Value) -> seat_agent_core::Result<String> {
        let msg = args["message"].as_str().unwrap_or("empty");
        Ok(format!("Echo: {}", msg))
    }
}

/// Tool that always fails.
struct FailTool;

#[async_trait]
impl Tool for FailTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "fail".into(),
            description: "Always fails".into(),
            parameters: serde_json::json!({ "type": "object" }),
        }
    }
    async fn execute(&self, _: serde_json::Value) -> seat_agent_core::Result<String> {
        Err(seat_agent_core::AgentError::Tool("boom".into()))
    }
}

fn user_input(text: &str) -> AgentInput {
    AgentInput {
        session_id: "itest".into(),
        message: Message {
            role: MessageRole::User,
            content: text.into(),
            tool_calls: None,
            tool_call_id: None,
        },
    }
}

async fn collect(rx: &mut mpsc::Receiver<AgentEvent>) -> Vec<AgentEvent> {
    let mut v = Vec::new();
    while let Some(e) = rx.recv().await {
        v.push(e);
    }
    v
}

fn token_texts(events: &[AgentEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|e| if let AgentEvent::Token(t) = e { Some(t.as_str()) } else { None })
        .collect()
}

// ============================================================================
// Tests
// ============================================================================

#[tokio::test]
async fn integration_direct_reply() {
    let llm = Box::new(seat_agent_core::MockLlmClient::new(vec![
        "Direct answer".into(),
    ]));
    let agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("hi"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    assert!(matches!(events.first(), Some(AgentEvent::StreamStart)));
    assert!(matches!(events.last(), Some(AgentEvent::StreamEnd)));
    assert!(token_texts(&events).contains(&"Direct answer"));
}

#[tokio::test]
async fn integration_tool_call_then_final_reply() {
    let tc = vec![
        LlmStreamChunk::ToolCallStart { id: "c1".into(), name: "echo".into() },
        LlmStreamChunk::ToolCallDelta { arguments: r#"{"message":"hi"}"#.into() },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];
    let final_reply = vec![
        LlmStreamChunk::Content("Got it".into()),
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::Stop },
    ];
    let llm = Box::new(ToolCallMock::new(vec![tc, final_reply]));
    let mut agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("hello"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    // ToolCallEnd should contain echo result
    let tool_ends: Vec<&str> = events
        .iter()
        .filter_map(|e| if let AgentEvent::ToolCallEnd { result, .. } = e { Some(result.as_str()) } else { None })
        .collect();
    assert!(tool_ends.iter().any(|r| r.contains("Echo: hi")));

    // Final reply
    assert!(token_texts(&events).contains(&"Got it"));
}

#[tokio::test]
async fn integration_transfer_to_human() {
    let tc = vec![
        LlmStreamChunk::ToolCallStart { id: "t1".into(), name: "transfer_to_human".into() },
        LlmStreamChunk::ToolCallDelta {
            arguments: r#"{"reason":"complex issue","reply":"为您转接专属客服"}"#.into(),
        },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];
    let after = vec![
        LlmStreamChunk::Content("好的".into()),
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::Stop },
    ];

    struct TransferTool;
    #[async_trait]
    impl Tool for TransferTool {
        fn definition(&self) -> ToolDefinition {
            ToolDefinition {
                name: "transfer_to_human".into(),
                description: "Transfer to human".into(),
                parameters: serde_json::json!({ "type": "object" }),
            }
        }
        async fn execute(&self, _: serde_json::Value) -> seat_agent_core::Result<String> {
            Ok("<<TRANSFER>>".into())
        }
    }

    let llm = Box::new(ToolCallMock::new(vec![tc, after]));
    let mut agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(TransferTool));

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("投诉"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    // ToolCallEnd for transfer tool should exist
    let has_transfer = events.iter().any(|e| {
        matches!(e, AgentEvent::ToolCallEnd { tool_name, .. } if tool_name == "transfer_to_human")
    });
    assert!(has_transfer, "Expected transfer_to_human tool call");
}

#[tokio::test]
async fn integration_max_rounds_enforced() {
    // Keep returning tool calls to hit the round limit
    let make_tc = |id: &str| vec![
        LlmStreamChunk::ToolCallStart { id: id.into(), name: "echo".into() },
        LlmStreamChunk::ToolCallDelta { arguments: r#"{"message":"x"}"#.into() },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];

    // Generate many tool call responses — more than max_rounds
    let responses: Vec<Vec<LlmStreamChunk>> = (0..5)
        .map(|i| make_tc(&format!("c{}", i)))
        .collect();
    let llm = Box::new(ToolCallMock::new(responses));
    let mut agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("loop"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    // Count tool call rounds — should be at most max_rounds (10 for default config)
    let tool_call_count = events.iter().filter(|e| matches!(e, AgentEvent::ToolCallStart { .. })).count();
    // With max_rounds=10, we should have at most 10 tool calls (but we only provided 5 responses)
    assert!(tool_call_count <= 10, "Tool calls {} exceeded max_rounds", tool_call_count);
}

#[tokio::test]
async fn integration_tool_execution_failure_continues() {
    let tc = vec![
        LlmStreamChunk::ToolCallStart { id: "f1".into(), name: "fail".into() },
        LlmStreamChunk::ToolCallDelta { arguments: "{}".into() },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];
    let final_reply = vec![
        LlmStreamChunk::Content("Recovered".into()),
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::Stop },
    ];

    let llm = Box::new(ToolCallMock::new(vec![tc, final_reply]));
    let mut agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(FailTool));

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("fail"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    // Tool call end should have error info
    let tool_end = events.iter().find(|e| {
        matches!(e, AgentEvent::ToolCallEnd { tool_name, .. } if tool_name == "fail")
    });
    assert!(tool_end.is_some());

    // Agent continues to final reply after tool failure
    assert!(token_texts(&events).contains(&"Recovered"));
}

#[tokio::test]
async fn integration_llm_error_returns_err() {
    let llm = Box::new(ErrorMock);
    let agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);

    let (tx, _rx) = mpsc::channel(100);
    let result = agent.on_message(user_input("test"), tx).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn integration_voice_mode_config() {
    let config = AgentConfig::voice();
    assert_eq!(config.modality, Modality::Voice);
    assert_eq!(config.max_rounds, 2);

    // Verify context uses voice system prompt
    let ctx = seat_agent_core::Context::new("s1".into(), config);
    assert!(ctx.system[0].content.contains("简短"));
}

#[tokio::test]
async fn integration_context_truncation() {
    let mut ctx = seat_agent_core::Context::new("s1".into(), AgentConfig::default());
    for i in 0..20 {
        ctx.add_user_message(format!("msg{}", i));
    }
    assert_eq!(ctx.history.len(), 20);
    ctx.truncate_history();
    // min_history_messages defaults to 2
    assert_eq!(ctx.history.len(), 2);
    assert_eq!(ctx.history[0].content, "msg18");
    assert_eq!(ctx.history[1].content, "msg19");
}

#[tokio::test]
async fn integration_multiple_tool_rounds() {
    let tc1 = vec![
        LlmStreamChunk::ToolCallStart { id: "c1".into(), name: "echo".into() },
        LlmStreamChunk::ToolCallDelta { arguments: r#"{"message":"first"}"#.into() },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];
    let tc2 = vec![
        LlmStreamChunk::ToolCallStart { id: "c2".into(), name: "echo".into() },
        LlmStreamChunk::ToolCallDelta { arguments: r#"{"message":"second"}"#.into() },
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::ToolCalls },
    ];
    let final_reply = vec![
        LlmStreamChunk::Content("Done with both".into()),
        LlmStreamChunk::Done { finish_reason: seat_agent_core::FinishReason::Stop },
    ];

    let llm = Box::new(ToolCallMock::new(vec![tc1, tc2, final_reply]));
    let mut agent = seat_agent_core::Agent::new(AgentConfig::default(), llm);
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = mpsc::channel(100);
    agent.on_message(user_input("multi"), tx).await.unwrap();
    let events = collect(&mut rx).await;

    let tool_count = events.iter().filter(|e| matches!(e, AgentEvent::ToolCallStart { .. })).count();
    assert_eq!(tool_count, 2, "Expected 2 tool call rounds");

    let tool_results: Vec<&str> = events
        .iter()
        .filter_map(|e| if let AgentEvent::ToolCallEnd { result, .. } = e { Some(result.as_str()) } else { None })
        .collect();
    assert!(tool_results.iter().any(|r| r.contains("first")));
    assert!(tool_results.iter().any(|r| r.contains("second")));
    assert!(token_texts(&events).contains(&"Done with both"));
}
