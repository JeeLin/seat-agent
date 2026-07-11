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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_llm() {
        let err = AgentError::Llm("bad request".into());
        assert_eq!(format!("{}", err), "LLM error: bad request");
    }

    #[test]
    fn test_error_display_tool() {
        let err = AgentError::Tool("timeout".into());
        assert_eq!(format!("{}", err), "Tool error: timeout");
    }

    #[test]
    fn test_error_display_knowledge() {
        let err = AgentError::Knowledge("not found".into());
        assert_eq!(format!("{}", err), "Knowledge store error: not found");
    }

    #[test]
    fn test_error_display_memory() {
        let err = AgentError::Memory("connection failed".into());
        assert_eq!(format!("{}", err), "Memory store error: connection failed");
    }

    #[test]
    fn test_error_display_vector() {
        let err = AgentError::Vector("index error".into());
        assert_eq!(format!("{}", err), "Vector store error: index error");
    }

    #[test]
    fn test_error_display_timeout() {
        let err = AgentError::Timeout(Duration::from_secs(5));
        let msg = format!("{}", err);
        assert!(msg.contains("5s"), "Expected 5s in timeout message: {}", msg);
    }

    #[test]
    fn test_error_display_max_rounds() {
        let err = AgentError::MaxRoundsExceeded(10);
        assert_eq!(format!("{}", err), "Max rounds exceeded: 10");
    }

    #[test]
    fn test_error_display_config() {
        let err = AgentError::Config("missing field".into());
        assert_eq!(format!("{}", err), "Configuration error: missing field");
    }

    #[test]
    fn test_error_display_internal() {
        let err = AgentError::Internal("bug".into());
        assert_eq!(format!("{}", err), "Internal error: bug");
    }

    #[test]
    fn test_error_from_serde_json() {
        let serde_err = serde_json::from_str::<serde_json::Value>("invalid").unwrap_err();
        let agent_err: AgentError = serde_err.into();
        assert!(matches!(agent_err, AgentError::Serialization(_)));
        let msg = format!("{}", agent_err);
        assert!(msg.contains("Serialization error"));
    }

    #[test]
    fn test_result_type_alias() {
        let ok: Result<i32> = Ok(42);
        assert_eq!(ok.unwrap(), 42);

        let err: Result<()> = Err(AgentError::Internal("test".into()));
        assert!(err.is_err());
    }
}
