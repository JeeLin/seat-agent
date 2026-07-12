//! seat-agent-tools: 工具注册与实现
//!
//! 提供知识库检索、业务查询、转人工等 Tool 实现，
//! 以及 ToolRegistry 分组激活机制。
pub mod business;
pub mod embedding;
pub mod knowledge;
pub mod registry;
pub mod transfer;

#[cfg(feature = "qdrant")]
pub mod qdrant_store;
