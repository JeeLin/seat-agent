use std::collections::HashMap;
use std::sync::RwLock;

use async_trait::async_trait;

use crate::error::Result;
use crate::traits::{SearchResult, VectorStore};

/// 进程内向量存储（默认 / 测试实现）
///
/// 使用余弦相似度进行检索，适合知识库规模可控的场景。
/// 零外部依赖，符合 core crate 硬性约束。
pub struct InMemoryVectorStore {
    data: RwLock<HashMap<String, Entry>>,
}

struct Entry {
    embedding: Vec<f32>,
    metadata: HashMap<String, serde_json::Value>,
}

impl InMemoryVectorStore {
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    /// 余弦相似度：a·b / (|a|·|b|)
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
        let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let norm_b: f32 = b.iter().map(|y| y * y).sum::<f32>().sqrt();
        if norm_a == 0.0 || norm_b == 0.0 {
            return 0.0;
        }
        dot / (norm_a * norm_b)
    }
}

impl Default for InMemoryVectorStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VectorStore for InMemoryVectorStore {
    async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let mut store = self
            .data
            .write()
            .map_err(|_| crate::error::AgentError::Internal("vector store lock poisoned".into()))?;
        store.insert(
            id.to_string(),
            Entry {
                embedding: embedding.to_vec(),
                metadata,
            },
        );
        Ok(())
    }

    async fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let store = self
            .data
            .read()
            .map_err(|_| crate::error::AgentError::Internal("vector store lock poisoned".into()))?;

        let mut scored: Vec<SearchResult> = store
            .iter()
            .map(|(id, entry)| SearchResult {
                id: id.clone(),
                score: Self::cosine_similarity(embedding, &entry.embedding),
                metadata: entry.metadata.clone(),
            })
            .collect();

        // 分数降序，取前 limit 个
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(limit);
        Ok(scored)
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let mut store = self
            .data
            .write()
            .map_err(|_| crate::error::AgentError::Internal("vector store lock poisoned".into()))?;
        store.remove(id);
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let store = self
            .data
            .read()
            .map_err(|_| crate::error::AgentError::Internal("vector store lock poisoned".into()))?;
        Ok(store.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn embed(x: f32) -> Vec<f32> {
        vec![x, 1.0 - x]
    }

    #[tokio::test]
    async fn upsert_search_delete_count() {
        let store = InMemoryVectorStore::new();
        let mut meta = HashMap::new();
        meta.insert("content".to_string(), json!("alpha"));
        store.upsert("a", &embed(0.9), meta).await.unwrap();

        let mut meta2 = HashMap::new();
        meta2.insert("content".to_string(), json!("beta"));
        store.upsert("b", &embed(0.1), meta2).await.unwrap();

        assert_eq!(store.count().await.unwrap(), 2);

        // 查询接近 embed(0.9)，期望 "a" 排第一
        let results = store.search(&embed(0.85), 1).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "a");
        assert!(results[0].score > 0.9);

        store.delete("a").await.unwrap();
        assert_eq!(store.count().await.unwrap(), 1);
    }

    #[tokio::test]
    async fn search_respects_limit_and_descending() {
        let store = InMemoryVectorStore::new();
        for (i, x) in [0.2f32, 0.5, 0.8].into_iter().enumerate() {
            let mut meta = HashMap::new();
            meta.insert("content".to_string(), json!(format!("doc-{i}")));
            store
                .upsert(&format!("d{i}"), &embed(x), meta)
                .await
                .unwrap();
        }
        let results = store.search(&embed(0.5), 2).await.unwrap();
        assert_eq!(results.len(), 2);
        // 最接近 0.5 的应排第一
        assert_eq!(results[0].id, "d1");
        assert!(results[0].score >= results[1].score);
    }
}
