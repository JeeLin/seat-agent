use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{BusinessBackend, Tool, ToolDefinition};
use serde_json::{json, Value};

/// 业务查询工具共用后端类型
type Backend = Arc<dyn BusinessBackend>;

/// 订单查询工具：根据订单号查询并格式化订单信息
pub struct OrderQueryTool {
    backend: Backend,
}

impl OrderQueryTool {
    pub fn new(backend: Backend) -> Self {
        Self { backend }
    }
}

#[async_trait]
impl Tool for OrderQueryTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "order_query".to_string(),
            description: "查询订单信息（状态、金额、下单时间等）".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "order_id": {
                        "type": "string",
                        "description": "订单号"
                    }
                },
                "required": ["order_id"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> seat_agent_core::Result<String> {
        let order_id = args
            .get("order_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                seat_agent_core::AgentError::Tool("order_query: missing 'order_id'".into())
            })?;

        let data = self.backend.query_order(order_id).await?;
        if data.is_null() {
            return Ok(format!("未找到订单 {order_id}，请核对订单号或转人工核实。"));
        }

        let order_id = data
            .get("order_id")
            .and_then(|v| v.as_str())
            .unwrap_or(order_id);
        let status = data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("未知");
        let amount = data
            .get("amount")
            .and_then(|v| v.as_str())
            .unwrap_or("未知");
        let created_at = data
            .get("created_at")
            .and_then(|v| v.as_str())
            .unwrap_or("未知");
        Ok(format!(
            "订单查询成功：\n订单号：{order_id}\n状态：{status}\n金额：{amount}\n下单时间：{created_at}"
        ))
    }
}

/// Mock 业务后端：返回确定性样本数据，用于测试与示例演示
pub struct MockBusinessBackend;

impl MockBusinessBackend {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MockBusinessBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl BusinessBackend for MockBusinessBackend {
    async fn query_order(&self, order_id: &str) -> seat_agent_core::Result<Value> {
        if order_id == "ORD-1001" {
            Ok(json!({
                "order_id": "ORD-1001",
                "status": "已发货",
                "amount": "¥299.00",
                "created_at": "2026-07-01 10:24"
            }))
        } else {
            Ok(Value::Null)
        }
    }

    async fn query_refund(&self, _refund_id: &str) -> seat_agent_core::Result<Value> {
        Ok(Value::Null)
    }

    async fn query_complaint(&self, _complaint_id: &str) -> seat_agent_core::Result<Value> {
        Ok(Value::Null)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> Arc<dyn BusinessBackend> {
        Arc::new(MockBusinessBackend::new())
    }

    #[tokio::test]
    async fn order_query_returns_expected_text() {
        let tool = OrderQueryTool::new(backend());
        let out = tool
            .execute(json!({ "order_id": "ORD-1001" }))
            .await
            .unwrap();
        assert!(out.contains("ORD-1001"));
        assert!(out.contains("已发货"));
        assert!(out.contains("¥299.00"));
    }

    #[tokio::test]
    async fn order_query_missing_id_is_error() {
        let tool = OrderQueryTool::new(backend());
        let err = tool.execute(json!({})).await.unwrap_err();
        assert!(err.to_string().contains("missing 'order_id'"));
    }

    #[tokio::test]
    async fn order_query_unknown_is_explicit() {
        let tool = OrderQueryTool::new(backend());
        let out = tool
            .execute(json!({ "order_id": "NOPE" }))
            .await
            .unwrap();
        assert!(out.contains("未找到订单"));
    }
}
