use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Duration;

use async_trait::async_trait;
use seat_agent_core::{EmbeddingClient, Result};

const DEFAULT_BASE_URL: &str = "https://api.openai.com/v1";
const DEFAULT_MODEL: &str = "text-embedding-3-small";

/// OpenAI Embedding API 客户端
pub struct OpenAiEmbeddingClient {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: String,
}

/// OpenAI embeddings 请求体
#[derive(Debug, serde::Serialize)]
struct OpenAiEmbeddingRequest {
    model: String,
    input: Vec<String>,
}

/// OpenAI embeddings 响应体
#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingResponse {
    data: Vec<OpenAiEmbeddingData>,
}

#[derive(Debug, serde::Deserialize)]
struct OpenAiEmbeddingData {
    embedding: Vec<f32>,
}

impl OpenAiEmbeddingClient {
    pub fn new(api_key: String, base_url: Option<String>, model: Option<String>) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .expect("Failed to create HTTP client");
        Self {
            client,
            api_key,
            base_url: base_url.unwrap_or_else(|| DEFAULT_BASE_URL.to_string()),
            model: model.unwrap_or_else(|| DEFAULT_MODEL.to_string()),
        }
    }
}

#[async_trait]
impl EmbeddingClient for OpenAiEmbeddingClient {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let input: Vec<String> = texts.iter().map(|t| t.to_string()).collect();
        let resp = self
            .client
            .post(format!("{}/embeddings", self.base_url))
            .bearer_auth(&self.api_key)
            .json(&OpenAiEmbeddingRequest {
                model: self.model.clone(),
                input,
            })
            .send()
            .await
            .map_err(|e| seat_agent_core::AgentError::Internal(format!("embedding request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(seat_agent_core::AgentError::Internal(format!(
                "embedding API error {status}: {body}"
            )));
        }

        let parsed: OpenAiEmbeddingResponse = resp
            .json()
            .await
            .map_err(|e| seat_agent_core::AgentError::Internal(format!("embedding parse failed: {e}")))?;

        Ok(parsed.data.into_iter().map(|d| d.embedding).collect())
    }
}

/// 确定性 Mock Embedding 客户端（测试用）
///
/// 基于文本哈希生成确定性伪向量，便于测试断言。
pub struct MockEmbeddingClient {
    dim: usize,
}

impl MockEmbeddingClient {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }

    /// 将文本转换为确定性向量
    fn embed_one(&self, text: &str) -> Vec<f32> {
        let mut vec = Vec::with_capacity(self.dim);
        for i in 0..self.dim {
            let mut hasher = DefaultHasher::new();
            (text, i).hash(&mut hasher);
            let h = hasher.finish();
            // 映射到 [-1, 1]
            vec.push((h as f64 / u64::MAX as f64 * 2.0 - 1.0) as f32);
        }
        vec
    }
}

#[async_trait]
impl EmbeddingClient for MockEmbeddingClient {
    async fn embed(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(texts.iter().map(|t| self.embed_one(t)).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_embed_is_deterministic_and_dimensioned() {
        let client = MockEmbeddingClient::new(16);
        let a = client.embed(&["hello"]).await.unwrap();
        let b = client.embed(&["hello"]).await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].len(), 16);
        assert_eq!(a, b, "相同输入应产生相同向量");

        let c = client.embed(&["world"]).await.unwrap();
        assert_ne!(a[0], c[0], "不同输入应产生不同向量");
    }

    #[tokio::test]
    async fn mock_embed_batch() {
        let client = MockEmbeddingClient::new(8);
        let out = client.embed(&["a", "b", "c"]).await.unwrap();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].len(), 8);
    }
}
