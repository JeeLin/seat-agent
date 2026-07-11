use std::time::Duration;

use serde::{Deserialize, Serialize};

/// 对话模态
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub enum Modality {
    /// 文本模式
    #[default]
    Text,
    /// 语音模式
    Voice,
}

/// Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    /// 对话模态
    pub modality: Modality,

    /// 最大工具调用轮次
    pub max_rounds: usize,

    /// 最大运行时长
    pub max_duration: Duration,

    /// 最大输出 token 数
    pub max_output_tokens: usize,

    /// 最少历史消息数（截断时保留）
    pub min_history_messages: usize,

    /// 总 token 预算
    pub total_token_limit: usize,

    /// 自定义系统提示词（None 使用默认）
    pub system_prompt: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            modality: Modality::Text,
            max_rounds: 10,
            max_duration: Duration::from_secs(30),
            max_output_tokens: 500,
            min_history_messages: 2,
            total_token_limit: 8000,
            system_prompt: None,
        }
    }
}

impl AgentConfig {
    /// 创建文本模式配置
    pub fn text() -> Self {
        Self::default()
    }

    /// 创建语音模式配置
    pub fn voice() -> Self {
        Self {
            modality: Modality::Voice,
            max_rounds: 2,
            max_output_tokens: 200,
            ..Default::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AgentConfig::default();
        assert_eq!(config.modality, Modality::Text);
        assert_eq!(config.max_rounds, 10);
        assert_eq!(config.max_duration, Duration::from_secs(30));
        assert_eq!(config.max_output_tokens, 500);
        assert_eq!(config.min_history_messages, 2);
        assert_eq!(config.total_token_limit, 8000);
        assert!(config.system_prompt.is_none());
    }

    #[test]
    fn test_text_factory() {
        let config = AgentConfig::text();
        assert_eq!(config.modality, Modality::Text);
        assert_eq!(config.max_rounds, 10);
    }

    #[test]
    fn test_voice_factory() {
        let config = AgentConfig::voice();
        assert_eq!(config.modality, Modality::Voice);
        assert_eq!(config.max_rounds, 2);
        assert_eq!(config.max_output_tokens, 200);
        // Other fields should use defaults
        assert_eq!(config.max_duration, Duration::from_secs(30));
        assert_eq!(config.min_history_messages, 2);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AgentConfig::voice();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: AgentConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.modality, Modality::Voice);
        assert_eq!(deserialized.max_rounds, 2);
        assert_eq!(deserialized.max_output_tokens, 200);
    }

    #[test]
    fn test_modality_serialization() {
        let text_json = serde_json::to_string(&Modality::Text).unwrap();
        assert_eq!(text_json, "\"Text\"");

        let voice_json = serde_json::to_string(&Modality::Voice).unwrap();
        assert_eq!(voice_json, "\"Voice\"");

        let deserialized: Modality = serde_json::from_str("\"Text\"").unwrap();
        assert_eq!(deserialized, Modality::Text);
    }
}
