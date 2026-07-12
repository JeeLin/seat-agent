use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{EmbeddingClient, Tool, ToolDefinition, VectorStore};
use serde_json::{json, Value};

#[cfg(test)]
use crate::embedding::MockEmbeddingClient;
/// 知识库检索工具
///
/// 串联：embed 查询 → 向量检索 → 格式化结果。
/// 作为 Agent 可调用的工具，是 RAG「准确性优先」的信息基础。
pub struct KnowledgeSearchTool {
    vector_store: Arc<dyn VectorStore>,
    embedding_client: Arc<dyn EmbeddingClient>,
    top_k: usize,
}

impl KnowledgeSearchTool {
    pub fn new(
        vector_store: Arc<dyn VectorStore>,
        embedding_client: Arc<dyn EmbeddingClient>,
        top_k: usize,
    ) -> Self {
        Self {
            vector_store,
            embedding_client,
            top_k,
        }
    }
}

#[async_trait]
impl Tool for KnowledgeSearchTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "knowledge_search".to_string(),
            description: "搜索知识库获取答案".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "搜索关键词或问题描述"
                    }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> seat_agent_core::Result<String> {
        let query = args.get("query").and_then(|v| v.as_str()).ok_or_else(|| {
            seat_agent_core::AgentError::Tool("knowledge_search: missing 'query'".into())
        })?;

        let embeddings = self.embedding_client.embed(&[query]).await?;
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| seat_agent_core::AgentError::Tool("embedding returned empty".into()))?;

        let results = self.vector_store.search(&embedding, self.top_k).await?;

        if results.is_empty() {
            // 不编造：明确告知无结果
            return Ok("知识库中没有找到相关内容，建议转人工或向客户说明无法回答。".to_string());
        }

        let mut out = String::new();
        for (i, r) in results.iter().enumerate() {
            let content = r
                .metadata
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("<无正文>");
            out.push_str(&format!("[{i}] (score={:.3}) {}\n", r.score, content));
        }
        Ok(out)
    }
}
#[async_trait]
impl seat_agent_core::KnowledgeStore for KnowledgeSearchTool {
    async fn search(
        &self,
        query: &str,
        limit: usize,
    ) -> seat_agent_core::Result<Vec<seat_agent_core::KnowledgeResult>> {
        let embeddings = self.embedding_client.embed(&[query]).await?;
        let embedding = embeddings
            .into_iter()
            .next()
            .ok_or_else(|| seat_agent_core::AgentError::Tool("embedding returned empty".into()))?;

        let top_k = if limit > 0 { limit } else { self.top_k };
        let results = self.vector_store.search(&embedding, top_k).await?;

        Ok(results
            .into_iter()
            .map(|r| {
                let content = r
                    .metadata
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                seat_agent_core::KnowledgeResult {
                    id: r.id,
                    content,
                    score: r.score,
                    metadata: r.metadata,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seat_agent_core::InMemoryVectorStore;

    use std::collections::HashMap;

    fn sample_embedding_client() -> Arc<dyn EmbeddingClient> {
        Arc::new(MockEmbeddingClient::new(8))
    }

    async fn seeded_store() -> Arc<dyn VectorStore> {
        let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new());
        let dim = 8;
        let texts = ["退款政策", "物流时效"];
        for (i, text) in texts.iter().enumerate() {
            let emb = sample_embedding_client().embed(&[text]).await.unwrap();
            let mut meta = HashMap::new();
            meta.insert(
                "content".to_string(),
                json!(format!("知识{i}: {text}详情...")),
            );
            store.upsert(&format!("k{i}"), &emb[0], meta).await.unwrap();
            let _ = dim;
        }
        store
    }

    #[tokio::test]
    async fn returns_relevant_content() {
        let store = seeded_store().await;
        let tool = KnowledgeSearchTool::new(store, sample_embedding_client(), 3);
        let result = tool.execute(json!({ "query": "退款" })).await.unwrap();
        assert!(result.contains("退款"), "结果应包含退款相关内容: {result}");
    }

    #[tokio::test]
    async fn missing_query_is_error() {
        let store = seeded_store().await;
        let tool = KnowledgeSearchTool::new(store, sample_embedding_client(), 3);
        let err = tool.execute(json!({})).await;
        assert!(err.is_err());
    }

    #[tokio::test]
    async fn no_result_is_explicit() {
        let store: Arc<dyn VectorStore> = Arc::new(InMemoryVectorStore::new());
        let tool = KnowledgeSearchTool::new(store, sample_embedding_client(), 3);
        let result = tool
            .execute(json!({ "query": "完全无关的问题xyz" }))
            .await
            .unwrap();
        assert!(result.contains("没有找到"), "空结果应明确告知: {result}");
    }
}
