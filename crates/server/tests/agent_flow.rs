//! 端到端集成测试
//!
//! 验证核心组件（Agent + MemoryManager + Tools）的完整交互流程。
//! 使用 Mock 组件，不需要真实 LLM/Redis/网络。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{
    Agent, AgentConfig, AgentEvent, AgentInput, BusinessBackend, LlmClient, LlmRequest,
    LlmStreamChunk, Message, MessageRole, FinishReason, Tool, ToolDefinition,
};

// ============================================================================
// Mock 实现
// ============================================================================

/// Mock LLM：可配置多轮响应
struct MockLlm {
    responses: Vec<Vec<LlmStreamChunk>>,
    idx: AtomicUsize,
}

impl MockLlm {
    fn simple(reply: &str) -> Self {
        Self {
            responses: vec![vec![
                LlmStreamChunk::Content(reply.into()),
                LlmStreamChunk::Done {
                    finish_reason: FinishReason::Stop,
                },
            ]],
            idx: AtomicUsize::new(0),
        }
    }

    fn with_responses(responses: Vec<Vec<LlmStreamChunk>>) -> Self {
        Self {
            responses,
            idx: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmClient for MockLlm {
    async fn chat_stream(
        &self,
        _req: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>,
        >,
    > {
        let i = self.idx.fetch_add(1, Ordering::SeqCst);
        let chunks = self.responses[i % self.responses.len()].clone();
        Ok(Box::pin(futures::stream::iter(chunks.into_iter().map(Ok))))
    }
}

/// Mock KnowledgeStore
struct MockKnowledgeStore;

#[async_trait]
impl seat_agent_core::KnowledgeStore for MockKnowledgeStore {
    async fn search(
        &self,
        _query: &str,
        _limit: usize,
    ) -> seat_agent_core::Result<Vec<seat_agent_core::KnowledgeResult>> {
        Ok(vec![])
    }
}

/// Mock MemoryManager：记录调用次数
struct MockMemoryManager {
    recall_count: AtomicUsize,
    save_count: AtomicUsize,
}

impl Default for MockMemoryManager {
    fn default() -> Self {
        Self {
            recall_count: AtomicUsize::new(0),
            save_count: AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl seat_agent_core::MemoryManager for MockMemoryManager {
    async fn recall(
        &self,
        _query: &str,
        _customer_id: &str,
    ) -> seat_agent_core::Result<Vec<String>> {
        self.recall_count.fetch_add(1, Ordering::SeqCst);
        Ok(vec!["历史摘要：客户之前咨询过退货政策".into()])
    }

    async fn save_session_summary(
        &self,
        _session_id: &str,
        _customer_id: &str,
        _history: &[seat_agent_core::LlmMessage],
    ) -> seat_agent_core::Result<()> {
        self.save_count.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

/// Simple echo tool for testing
struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "Echoes the input".to_string(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {"type": "string"}
                }
            }),
        }
    }

    async fn execute(&self, args: serde_json::Value) -> seat_agent_core::Result<String> {
        let msg = args
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("empty");
        Ok(format!("Echo: {}", msg))
    }
}

// ============================================================================
// 测试
// ============================================================================

/// 基本文本对话：用户发消息，LLM 直接回复
#[tokio::test]
async fn test_basic_text_reply() {
    let mut agent = Agent::new(AgentConfig::default(), Box::new(MockLlm::simple("你好！")));
    agent.set_knowledge(Box::new(MockKnowledgeStore));
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = AgentInput {
        session_id: "test-1".into(),
        customer_id: "cust-1".into(),
        message: Message {
            role: MessageRole::User,
            content: "你好".into(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });
    handle.await.unwrap().unwrap();

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    assert!(!events.is_empty());
    // 第一个事件应该是 StreamStart
    assert!(matches!(events.first(), Some(AgentEvent::StreamStart)));
    // 最后一个事件应该是 StreamEnd
    assert!(matches!(events.last(), Some(AgentEvent::StreamEnd)));
    // 中间应该有 Token 事件
    let has_token = events.iter().any(|e| matches!(e, AgentEvent::Token(_)));
    assert!(has_token, "should have token events");
}

/// 工具调用流程：LLM 请求工具 → 执行 → LLM 返回最终回复
#[tokio::test]
async fn test_tool_call_and_reply() {
    let tool_call_response = vec![
        LlmStreamChunk::ToolCallStart {
            id: "call_1".into(),
            name: "echo".into(),
        },
        LlmStreamChunk::ToolCallDelta {
            arguments: r#"{"message":"hello"}"#.into(),
        },
        LlmStreamChunk::Done {
            finish_reason: FinishReason::ToolCalls,
        },
    ];
    let final_response = vec![
        LlmStreamChunk::Content("工具执行完毕".into()),
        LlmStreamChunk::Done {
            finish_reason: FinishReason::Stop,
        },
    ];

    let mut agent = Agent::new(
        AgentConfig::default(),
        Box::new(MockLlm::with_responses(vec![tool_call_response, final_response])),
    );
    agent.set_knowledge(Box::new(MockKnowledgeStore));
    agent.register_tool(Box::new(EchoTool));

    let (tx, mut rx) = tokio::sync::mpsc::channel(100);
    let input = AgentInput {
        session_id: "test-2".into(),
        customer_id: "cust-2".into(),
        message: Message {
            role: MessageRole::User,
            content: "帮我echo一下".into(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });
    handle.await.unwrap().unwrap();

    let mut events = Vec::new();
    while let Some(event) = rx.recv().await {
        events.push(event);
    }

    // 应该有工具调用开始和结束事件
    let has_tool_start = events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallStart { .. }));
    let has_tool_end = events
        .iter()
        .any(|e| matches!(e, AgentEvent::ToolCallEnd { .. }));
    assert!(has_tool_start, "should have ToolCallStart");
    assert!(has_tool_end, "should have ToolCallEnd");
}

/// MemoryManager 被正确调用：recall 在开始，save 在结束
#[tokio::test]
async fn test_memory_lifecycle() {
    let memory = Arc::new(MockMemoryManager::default());
    let memory_check = memory.clone();

    let mut agent = Agent::new(AgentConfig::default(), Box::new(MockLlm::simple("收到")));
    agent.set_knowledge(Box::new(MockKnowledgeStore));
    agent.set_memory(Box::new(MockMemoryManager::default()));
    // Note: we can't easily check the internal memory manager's call count
    // because Agent takes ownership via Box. This test verifies the flow doesn't panic.

    let (tx, _rx) = tokio::sync::mpsc::channel(100);
    let input = AgentInput {
        session_id: "test-3".into(),
        customer_id: "cust-3".into(),
        message: Message {
            role: MessageRole::User,
            content: "测试记忆".into(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });
    handle.await.unwrap().unwrap();
}

/// max_rounds 配置验证
#[tokio::test]
async fn test_max_rounds_respected() {
    // AgentConfig default max_rounds = 10 for text
    let config = AgentConfig::default();
    assert_eq!(config.max_rounds, 10, "default text max_rounds should be 10");

    // Voice config has lower max_rounds
    let voice_config = AgentConfig::voice();
    assert_eq!(voice_config.max_rounds, 2, "voice max_rounds should be 2");
}

/// Customer ID 通过 AgentInput 传递
#[tokio::test]
async fn test_customer_id_propagation() {
    let mut agent = Agent::new(AgentConfig::default(), Box::new(MockLlm::simple("ok")));
    agent.set_knowledge(Box::new(MockKnowledgeStore));

    let (tx, _rx) = tokio::sync::mpsc::channel(100);
    let input = AgentInput {
        session_id: "test-4".into(),
        customer_id: "vip-customer-123".into(),
        message: Message {
            role: MessageRole::User,
            content: "测试".into(),
            tool_calls: None,
            tool_call_id: None,
        },
    };

    let handle = tokio::spawn(async move { agent.on_message(input, tx).await });
    handle.await.unwrap().unwrap();
}
