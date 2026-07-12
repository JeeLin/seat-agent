//! seat-agent-core: Agent Runtime 核心层
//!
//! 定义 Agent Loop、Context 分层模型、核心 Trait 接口。
//! 零外部依赖——不依赖 server 或 infra crate。
pub mod agent;
pub mod config;
pub mod context;
pub mod error;
pub mod mock;
pub use mock::MockLlmClient;
pub mod traits;
pub mod vector_store;
pub use agent::Agent;
pub use config::{AgentConfig, Modality};
pub use context::{AgentEvent, AgentInput, Context, Message};
pub use error::{AgentError, Result};
pub use traits::{
    BusinessBackend, EmbeddingClient, FinishReason, IntermediateReply, KnowledgeResult,
    KnowledgeStore, LlmClient, LlmMessage, LlmRequest, LlmStreamChunk, MemoryManager, MessageRole,
    SearchResult, Tool, ToolCall, ToolDefinition, TtsClient, VectorStore,
};
pub use vector_store::InMemoryVectorStore;
