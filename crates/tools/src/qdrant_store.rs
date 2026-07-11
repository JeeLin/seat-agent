//! Qdrant 向量存储实现（feature = "qdrant" 时启用）
//!
//! 生产可用的分布式向量检索。仅在 `qdrant` feature 启用时编译，
//! 默认构建不引入 `qdrant-client` 依赖。

use std::collections::HashMap;

use async_trait::async_trait;
use qdrant_client::qdrant::{
    point_id::PointIdOptions, with_payload_selector::SelectorOptions, DeletePointsBuilder, PointId,
    PointStruct, PointsIdsList, SearchPointsBuilder, UpsertPointsBuilder, Value,
};
use qdrant_client::{Payload, Qdrant};
use seat_agent_core::{AgentError, Result, SearchResult, VectorStore};

/// 基于 Qdrant 的向量存储实现
pub struct QdrantVectorStore {
    client: Qdrant,
    collection: String,
}

impl QdrantVectorStore {
    /// 连接到指定 URL 的 Qdrant 服务
    pub fn new(url: &str, collection: impl Into<String>) -> Result<Self> {
        let client = Qdrant::from_url(url)
            .build()
            .map_err(|e| AgentError::Vector(format!("failed to connect qdrant: {e}")))?;
        Ok(Self {
            client,
            collection: collection.into(),
        })
    }
}

fn point_id(id: &str) -> PointId {
    // 优先尝试解析为数字 id，否则使用 uuid 字符串
    match id.parse::<u64>() {
        Ok(n) => PointId {
            point_id_options: Some(PointIdOptions::Num(n)),
        },
        Err(_) => PointId {
            point_id_options: Some(PointIdOptions::Uuid(id.to_string())),
        },
    }
}

fn point_id_to_string(id: &PointId) -> String {
    match &id.point_id_options {
        Some(PointIdOptions::Num(n)) => n.to_string(),
        Some(PointIdOptions::Uuid(s)) => s.clone(),
        None => String::new(),
    }
}
fn metadata_to_payload(metadata: &HashMap<String, serde_json::Value>) -> Payload {
    Payload::from(metadata.clone())
}

fn payload_to_metadata(payload: &HashMap<String, Value>) -> HashMap<String, serde_json::Value> {
    payload
        .iter()
        .map(|(k, v)| (k.clone(), qdrant_value_to_json(v)))
        .collect()
}

fn qdrant_value_to_json(v: &Value) -> serde_json::Value {
    use qdrant_client::qdrant::value::Kind;
    match &v.kind {
        Some(Kind::StringValue(s)) => serde_json::Value::String(s.clone()),
        Some(Kind::IntegerValue(i)) => serde_json::Value::Number((*i).into()),
        Some(Kind::BoolValue(b)) => serde_json::Value::Bool(*b),
        Some(Kind::DoubleValue(d)) => serde_json::Number::from_f64(*d)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Some(Kind::ListValue(l)) => {
            serde_json::Value::Array(l.values.iter().map(qdrant_value_to_json).collect())
        }
        Some(Kind::NullValue(_)) => serde_json::Value::Null,
        Some(Kind::StructValue(s)) => serde_json::Value::Object(
            s.fields
                .iter()
                .map(|(k, v)| (k.clone(), qdrant_value_to_json(v)))
                .collect(),
        ),
        None => serde_json::Value::Null,
    }
}

#[async_trait]
impl VectorStore for QdrantVectorStore {
    async fn upsert(
        &self,
        id: &str,
        embedding: &[f32],
        metadata: HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let point = PointStruct::new(
            point_id(id),
            embedding.to_vec(),
            metadata_to_payload(&metadata),
        );
        let request = UpsertPointsBuilder::new(self.collection.clone(), vec![point]);
        self.client
            .upsert_points(request)
            .await
            .map_err(|e| AgentError::Vector(format!("upsert failed: {e}")))?;
        Ok(())
    }

    async fn search(&self, embedding: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let request =
            SearchPointsBuilder::new(self.collection.clone(), embedding.to_vec(), limit as u64)
                .with_payload(SelectorOptions::Enable(true));
        let response = self
            .client
            .search_points(request)
            .await
            .map_err(|e| AgentError::Vector(format!("search failed: {e}")))?;

        Ok(response
            .result
            .into_iter()
            .map(|p| SearchResult {
                id: p.id.as_ref().map(point_id_to_string).unwrap_or_default(),
                score: p.score,
                metadata: payload_to_metadata(&p.payload),
            })
            .collect())
    }

    async fn delete(&self, id: &str) -> Result<()> {
        let request = DeletePointsBuilder::new(self.collection.clone()).points(PointsIdsList {
            ids: vec![point_id(id)],
        });
        self.client
            .delete_points(request)
            .await
            .map_err(|e| AgentError::Vector(format!("delete failed: {e}")))?;
        Ok(())
    }

    async fn count(&self) -> Result<usize> {
        let request = qdrant_client::qdrant::CountPointsBuilder::new(self.collection.clone());
        let response = self
            .client
            .count(request)
            .await
            .map_err(|e| AgentError::Vector(format!("count failed: {e}")))?;
        Ok(response.result.map(|r| r.count as usize).unwrap_or(0))
    }
}
