use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use serde::{Deserialize, Serialize};

use crate::error::Result;

// ============================================================================
// LLM Client
// ============================================================================

/// LLM 请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub stream: bool,
}

/// LLM 消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// 消息角色
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 工具调用
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

/// LLM 流式响应块
#[derive(Debug, Clone)]
pub enum LlmStreamChunk {
    /// 文本内容
    Content(String),
    /// 工具调用开始
    ToolCallStart { id: String, name: String },
    /// 工具调用参数（增量）
    ToolCallDelta { arguments: String },
    /// 流结束
    Done { finish_reason: FinishReason },
    /// 错误
    Error(String),
}

/// 完成原因
#[derive(Debug, Clone)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
    ContentFilter,
}

/// LLM 客户端 trait
#[async_trait]
pub trait LlmClient: Send + Sync {
    /// 流式调用 LLM
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamChunk>> + Send>>>;
}

// ============================================================================
// Knowledge Store
// ============================================================================

/// 知识库检索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeResult {
    pub id: String,
    pub content: String,
    pub score: f32,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 知识库存储 trait
#[async_trait]
pub trait KnowledgeStore: Send + Sync {
    /// 检索知识库
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<KnowledgeResult>>;
}

// ============================================================================
// Memory Store
// ============================================================================

/// 记忆存储 trait
#[async_trait]
pub trait MemoryStore: Send + Sync {
    /// 保存记忆
    async fn save(&self, key: &str, value: &str, ttl: Option<Duration>) -> Result<()>;

    /// 加载记忆
    async fn load(&self, key: &str) -> Result<Option<String>>;

    /// 删除记忆
    async fn delete(&self, key: &str) -> Result<()>;
}

// ============================================================================
// Vector Store
// ============================================================================

/// 向量搜索结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub metadata: HashMap<String, serde_json::Value>,
}

/// 向量存储 trait
#[async_trait]
pub trait VectorStore: Send + Sync {
    /// 插入或更新向量
    async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()>;

    /// 搜索相似向量
    async fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>>;

    /// 删除向量
    async fn delete(&self, id: &str) -> Result<()>;

    /// 统计数量
    async fn count(&self) -> Result<usize>;
}

// ============================================================================
// Embedding Client
// ============================================================================

/// Embedding 客户端 trait
#[async_trait]
pub trait EmbeddingClient: Send + Sync {
    /// 将文本转换为向量
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>>;
}

// ============================================================================
// TTS Client
// ============================================================================

/// TTS 客户端 trait
#[async_trait]
pub trait TtsClient: Send + Sync {
    /// 将文本转换为音频
    async fn synthesize(&self, text: &str) -> Result<Vec<u8>>;
}

// ============================================================================
// Tool
// ============================================================================

/// 工具定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// 工具参数定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    pub r#type: String,
}

/// 中间回复
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntermediateReply {
    pub text: String,
    pub audio_cue: Option<String>,
}

/// 工具 trait
#[async_trait]
pub trait Tool: Send + Sync {
    /// 获取工具定义
    fn definition(&self) -> ToolDefinition;

    /// 执行工具
    async fn execute(&self, args: serde_json::Value) -> Result<String>;
}
