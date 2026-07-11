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
    BusinessBackend, EmbeddingClient, IntermediateReply, KnowledgeResult, KnowledgeStore,
    LlmClient, LlmMessage, LlmRequest, LlmStreamChunk, MemoryStore, MessageRole, SearchResult,
    Tool, ToolCall, ToolDefinition, TtsClient, VectorStore,
};
pub use vector_store::InMemoryVectorStore;
