use std::time::Duration;

/// seat-agent 统一错误类型
#[derive(Debug, thiserror::Error)]
pub enum AgentError {
    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Knowledge store error: {0}")]
    Knowledge(String),

    #[error("Memory store error: {0}")]
    Memory(String),

    #[error("Vector store error: {0}")]
    Vector(String),

    #[error("Timeout after {0:?}")]
    Timeout(Duration),

    #[error("Max rounds exceeded: {0}")]
    MaxRoundsExceeded(usize),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
}

/// Result 类型别名，默认错误类型为 AgentError
pub type Result<T, E = AgentError> = std::result::Result<T, E>;
