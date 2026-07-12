use anyhow::{Context, Result};
use seat_agent_core::config::AgentConfig;
use serde::Deserialize;

/// 服务器配置
#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    /// 监听地址
    #[serde(default = "default_addr")]
    pub addr: String,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            addr: default_addr(),
        }
    }
}

fn default_addr() -> String {
    "0.0.0.0:50051".to_string()
}

/// LLM 配置
#[derive(Debug, Deserialize)]
pub struct LlmConfig {
    /// API 基础 URL
    pub base_url: String,

    /// API 密钥
    pub api_key: String,

    /// 模型名称
    #[serde(default = "default_model")]
    pub model: String,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: default_model(),
        }
    }
}

fn default_model() -> String {
    "gpt-4".to_string()
}

/// Redis 配置
#[derive(Debug, Deserialize)]
pub struct RedisConfig {
    /// Redis URL
    #[serde(default = "default_redis_url")]
    pub url: String,

    /// 会话 TTL（秒）
    #[serde(default = "default_session_ttl")]
    pub session_ttl: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            url: default_redis_url(),
            session_ttl: default_session_ttl(),
        }
    }
}

fn default_redis_url() -> String {
    "redis://127.0.0.1:6379".to_string()
}

fn default_session_ttl() -> u64 {
    3600
}

/// Embedding 配置
#[derive(Debug, Deserialize)]
pub struct EmbeddingConfig {
    /// API 基础 URL
    #[serde(default = "default_embedding_base_url")]
    pub base_url: String,

    /// API 密钥
    #[serde(default)]
    pub api_key: String,

    /// 模型名称
    #[serde(default = "default_embedding_model")]
    pub model: String,

    /// 向量维度
    #[serde(default = "default_embedding_dim")]
    pub dim: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            base_url: default_embedding_base_url(),
            api_key: String::new(),
            model: default_embedding_model(),
            dim: default_embedding_dim(),
        }
    }
}

fn default_embedding_base_url() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_embedding_dim() -> usize {
    1536
}

/// 知识库检索配置
#[derive(Debug, Deserialize)]
pub struct KnowledgeConfig {
    /// 召回数量
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// 向量存储后端: "memory" 或 "qdrant"
    #[serde(default = "default_vector_store")]
    pub vector_store: String,

    /// Qdrant 服务 URL（仅 vector_store = "qdrant" 时使用）
    #[serde(default)]
    pub qdrant_url: String,

    /// Qdrant 集合名
    #[serde(default = "default_qdrant_collection")]
    pub qdrant_collection: String,
}

impl Default for KnowledgeConfig {
    fn default() -> Self {
        Self {
            top_k: default_top_k(),
            vector_store: default_vector_store(),
            qdrant_url: String::new(),
            qdrant_collection: default_qdrant_collection(),
        }
    }
}

fn default_top_k() -> usize {
    3
}

fn default_vector_store() -> String {
    "memory".to_string()
}

fn default_qdrant_collection() -> String {
    "seat_agent_knowledge".to_string()
}

/// Memory 配置
#[derive(Debug, Deserialize)]
pub struct MemoryConfig {
    /// 短期记忆保留最大消息数（0 = 不限制）
    #[serde(default = "default_short_term_max")]
    pub short_term_max: usize,

    /// 长期记忆检索 top_k
    #[serde(default = "default_long_term_top_k")]
    pub long_term_top_k: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            short_term_max: 20,
            long_term_top_k: 3,
        }
    }
}

fn default_short_term_max() -> usize {
    20
}

fn default_long_term_top_k() -> usize {
    3
}

/// 应用配置
#[derive(Debug, Deserialize)]
pub struct AppConfig {
    /// 服务器配置
    #[serde(default)]
    pub server: ServerConfig,

    /// LLM 配置
    pub llm: LlmConfig,

    /// Embedding 配置
    #[serde(default)]
    pub embedding: EmbeddingConfig,

    /// 知识库检索配置
    #[serde(default)]
    pub knowledge: KnowledgeConfig,

    /// Redis 配置
    #[serde(default)]
    pub redis: RedisConfig,

    /// Agent 配置
    #[serde(default)]
    pub agent: AgentConfig,

    /// Memory 配置
    #[serde(default)]
    pub memory: MemoryConfig,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig::default(),
            llm: LlmConfig::default(),
            embedding: EmbeddingConfig::default(),
            knowledge: KnowledgeConfig::default(),
            redis: RedisConfig::default(),
            agent: AgentConfig::default(),
            memory: MemoryConfig::default(),
        }
    }
}

impl AppConfig {
    /// 从文件加载配置
    pub fn from_file(path: &str) -> Result<Self> {
        let content =
            std::fs::read_to_string(path).with_context(|| format!("无法读取配置文件: {}", path))?;

        let config: AppConfig = serde_yaml::from_str(&content)
            .with_context(|| format!("配置文件解析失败: {}", path))?;

        Ok(config)
    }

    /// 从环境变量覆盖配置
    pub fn with_env_overrides(mut self) -> Self {
        if let Ok(addr) = std::env::var("SEAT_AGENT_ADDR") {
            self.server.addr = addr;
        }
        if let Ok(base_url) = std::env::var("SEAT_AGENT_LLM_BASE_URL") {
            self.llm.base_url = base_url;
        }
        if let Ok(api_key) = std::env::var("SEAT_AGENT_LLM_API_KEY") {
            self.llm.api_key = api_key;
        }
        if let Ok(model) = std::env::var("SEAT_AGENT_LLM_MODEL") {
            self.llm.model = model;
        }
        if let Ok(url) = std::env::var("SEAT_AGENT_EMBEDDING_URL") {
            self.embedding.base_url = url;
        }
        if let Ok(key) = std::env::var("SEAT_AGENT_EMBEDDING_API_KEY") {
            self.embedding.api_key = key;
        }
        if let Ok(redis_url) = std::env::var("SEAT_AGENT_REDIS_URL") {
            self.redis.url = redis_url;
        }
        self
    }
}

impl AppConfig {
    /// 验证配置是否有效
    pub fn validate(&self) -> Result<()> {
        // 验证 LLM 配置
        if self.llm.api_key.is_empty() {
            anyhow::bail!("LLM API key 不能为空，请设置 SEAT_AGENT_LLM_API_KEY 或在配置文件中提供");
        }

        if self.llm.base_url.is_empty() {
            anyhow::bail!("LLM base URL 不能为空");
        }

        // 验证 Redis URL 格式
        if !self.redis.url.starts_with("redis://") && !self.redis.url.starts_with("rediss://") {
            anyhow::bail!("Redis URL 格式无效，应以 redis:// 或 rediss:// 开头");
        }

        // 验证 TTL
        if self.redis.session_ttl == 0 {
            anyhow::bail!("Redis session TTL 不能为 0");
        }

        Ok(())
    }
}
