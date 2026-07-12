//! Memory 管理器：将 memory crate 接线到 core 的 MemoryManager trait
//!
//! 实现 `seat_agent_core::MemoryManager`，封装：
//! - `LongTermMemory`：跨会话的向量检索历史摘要
//! - `SummaryGenerator`：会话结束时生成/修正摘要

use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{EmbeddingClient, LlmClient, LlmMessage, MemoryManager, Result, VectorStore};
use seat_agent_memory::{LongTermMemory, SummaryGenerator};

use crate::config::MemoryConfig;

/// 基于 memory crate 的 MemoryManager 实现
pub struct MemoryManagerImpl {
    long_term: LongTermMemory<dyn VectorStore>,
    summary_gen: SummaryGenerator<dyn LlmClient>,
    long_term_top_k: usize,
}

impl MemoryManagerImpl {
    /// 根据配置构建 MemoryManagerImpl
    pub fn new(
        config: &MemoryConfig,
        vector_store: Arc<dyn VectorStore>,
        embedding: Arc<dyn EmbeddingClient>,
        llm: Arc<dyn LlmClient>,
    ) -> Self {
        Self {
            long_term: LongTermMemory::new(vector_store, embedding),
            summary_gen: SummaryGenerator::new(llm),
            long_term_top_k: config.long_term_top_k,
        }
    }
}

#[async_trait]
impl MemoryManager for MemoryManagerImpl {
    /// 检索相关历史摘要，返回摘要文本列表
    async fn recall(&self, query: &str, customer_id: &str) -> Result<Vec<String>> {
        let summaries = self
            .long_term
            .search_summaries(query, customer_id, self.long_term_top_k)
            .await?;
        Ok(summaries.into_iter().map(|(_, text, _)| text).collect())
    }

    /// 会话结束时生成并保存摘要
    ///
    /// 先检索该客户已有的最新摘要（如果有），在其基础上修正合并，
    /// 再 embed 后持久化到 VectorStore。
    async fn save_session_summary(
        &self,
        session_id: &str,
        customer_id: &str,
        history: &[LlmMessage],
    ) -> Result<()> {
        // 尝试获取已有摘要（取最新的一个）
        let existing = self
            .long_term
            .search_summaries("", customer_id, 1)
            .await?
            .into_iter()
            .next()
            .map(|(_, text, _)| text);

        let summary = self
            .summary_gen
            .generate(history, existing.as_deref())
            .await?;

        self.long_term
            .save_summary(session_id, customer_id, &summary)
            .await
    }
}
