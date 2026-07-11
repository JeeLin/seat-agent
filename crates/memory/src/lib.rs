//! seat-agent-memory: 短期记忆与长期记忆
//!
//! 提供三种记忆能力：
//! - **ShortTermMemory**：滑动窗口截断，保护 Context 层不无限增长
//! - **LongTermMemory**：向量检索历史摘要，跨会话保持客户上下文
//! - **SummaryGenerator**：会话结束时生成/修正摘要

pub mod long_term;
pub mod short_term;
pub mod summary;

pub use long_term::LongTermMemory;
pub use short_term::ShortTermMemory;
pub use summary::SummaryGenerator;
