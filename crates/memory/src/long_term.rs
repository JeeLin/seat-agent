//! 长期记忆：向量检索历史摘要
//!
//! 会话开始时检索长期记忆，将历史摘要注入 Context，
//! 使 Agent 能跨会话记住客户信息。
//!
//! 存储流程：
//! - 会话结束时，将摘要 embed 成向量，upsert 到 VectorStore
//! - 会话开始时，将用户最新消息 embed，搜索相似历史摘要
//!
//! key 规律：`summary:{session_id}:{customer_id}`

use std::sync::Arc;

use seat_agent_core::{EmbeddingClient, SearchResult, VectorStore};

/// 长期记忆管理器
pub struct LongTermMemory<V: VectorStore> {
    store: Arc<V>,
    embedding: Arc<dyn EmbeddingClient>,
}

impl<V: VectorStore> LongTermMemory<V> {
    pub fn new(store: Arc<V>, embedding: Arc<dyn EmbeddingClient>) -> Self {
        Self { store, embedding }
    }

    /// 保存会话摘要到长期记忆
    ///
    /// - `session_id`：会话 ID
    /// - `customer_id`：客户 ID（用于跨会话关联）
    /// - `summary`：摘要文本
    pub async fn save_summary(
        &self,
        session_id: &str,
        customer_id: &str,
        summary: &str,
    ) -> seat_agent_core::Result<()> {
        let embedding = self.embedding.embed(&[summary]).await?;
        if let Some(vec) = embedding.first() {
            let id = format!("summary:{}:{}", session_id, customer_id);
            let mut metadata = std::collections::HashMap::new();
            metadata.insert(
                "session_id".to_string(),
                serde_json::Value::String(session_id.to_string()),
            );
            metadata.insert(
                "customer_id".to_string(),
                serde_json::Value::String(customer_id.to_string()),
            );
            metadata.insert(
                "summary".to_string(),
                serde_json::Value::String(summary.to_string()),
            );
            self.store.upsert(&id, vec, metadata).await?;
        }
        Ok(())
    }

    /// 根据当前用户消息检索相关历史摘要
    ///
    /// 返回 `Vec<(session_id, summary_text, score)>`，按相关度降序排列。
    pub async fn search_summaries(
        &self,
        query: &str,
        customer_id: &str,
        top_k: usize,
    ) -> seat_agent_core::Result<Vec<(String, String, f32)>> {
        let embedding = self.embedding.embed(&[query]).await?;
        let Some(vec) = embedding.first() else {
            return Ok(vec![]);
        };

        let results = self.store.search(vec, top_k).await?;

        // 过滤掉不属于该 customer_id 的结果（VectorStore 搜索是全量的）
        let summaries: Vec<(String, String, f32)> = results
            .into_iter()
            .filter(|r| {
                r.metadata
                    .get("customer_id")
                    .and_then(|v| v.as_str())
                    == Some(customer_id)
            })
            .filter_map(|r| {
                let session_id = r
                    .metadata
                    .get("session_id")
                    .and_then(|v| v.as_str())?
                    .to_string();
                let summary = r
                    .metadata
                    .get("summary")
                    .and_then(|v| v.as_str())?
                    .to_string();
                Some((session_id, summary, r.score))
            })
            .collect();

        Ok(summaries)
    }

    /// 删除指定会话的摘要
    pub async fn delete_summary(
        &self,
        session_id: &str,
        customer_id: &str,
    ) -> seat_agent_core::Result<()> {
        let id = format!("summary:{}:{}", session_id, customer_id);
        self.store.delete(&id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seat_agent_core::{AgentError, InMemoryVectorStore};

    struct MockEmbedding {
        dim: usize,
    }

    #[async_trait::async_trait]
    impl EmbeddingClient for MockEmbedding {
        async fn embed(
            &self,
            texts: &[&str],
        ) -> seat_agent_core::Result<Vec<Vec<f32>>> {
            // 伪 embedding：用文本 hash 生成固定向量
            Ok(texts
                .iter()
                .map(|t| {
                    let hash = t.len() as f32;
                    (0..self.dim)
                        .map(|i| (hash + i as f32).sin())
                        .collect()
                })
                .collect())
        }
    }

    fn make_store() -> (
        Arc<InMemoryVectorStore>,
        Arc<MockEmbedding>,
    ) {
        let store = Arc::new(InMemoryVectorStore::new());
        let embedding = Arc::new(MockEmbedding { dim: 128 });
        (store, embedding)
    }

    #[tokio::test]
    async fn save_and_search_summary() {
        let (store, emb) = make_store();
        let ltm = LongTermMemory::new(store, emb);

        ltm.save_summary("sess1", "cust1", "客户上周咨询过退款")
            .await
            .unwrap();
        ltm.save_summary("sess2", "cust2", "另一位客户咨询退货")
            .await
            .unwrap();

        // 搜索时应只返回属于 cust1 的结果
        let results = ltm
            .search_summaries("退款", "cust1", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "sess1");
        assert!(results[0].1.contains("退款"));
    }

    #[tokio::test]
    async fn delete_summary() {
        let (store, emb) = make_store();
        let ltm = LongTermMemory::new(store, emb);

        ltm.save_summary("sess1", "cust1", "test summary")
            .await
            .unwrap();

        ltm.delete_summary("sess1", "cust1").await.unwrap();

        let results = ltm
            .search_summaries("test", "cust1", 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn search_empty_store() {
        let (store, emb) = make_store();
        let ltm = LongTermMemory::new(store, emb);

        let results = ltm
            .search_summaries("任何查询", "cust1", 10)
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
