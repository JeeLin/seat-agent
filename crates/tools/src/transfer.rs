use async_trait::async_trait;
use seat_agent_core::{Tool, ToolDefinition};
use serde_json::{json, Value};

/// 转人工工具：作为 Agent 转人工的出口。
///
/// 仅负责格式化 LLM 决策的转人工原因与回复话术，并标记转人工出口；
/// 何时调用由 Agent Loop 决策（如检索不到、无法回答、需人工介入）。
const TRANSFER_MARKER: &str = "<<TRANSFER>>";

/// 转人工工具
pub struct TransferToHumanTool;

impl TransferToHumanTool {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TransferToHumanTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for TransferToHumanTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "transfer_to_human".to_string(),
            description: "转接人工客服".to_string(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "reason": {
                        "type": "string",
                        "description": "转人工原因（内部记录）"
                    },
                    "reply": {
                        "type": "string",
                        "description": "回复给客户的话术（委婉专业）"
                    }
                },
                "required": ["reason", "reply"]
            }),
        }
    }

    async fn execute(&self, args: Value) -> seat_agent_core::Result<String> {
        let reason = args.get("reason").and_then(|v| v.as_str()).ok_or_else(|| {
            seat_agent_core::AgentError::Tool("transfer_to_human: missing 'reason'".into())
        })?;
        let reply = args.get("reply").and_then(|v| v.as_str()).ok_or_else(|| {
            seat_agent_core::AgentError::Tool("transfer_to_human: missing 'reply'".into())
        })?;

        Ok(format!("{TRANSFER_MARKER}\n原因：{reason}\n话术：{reply}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_transfer_marker_and_reply() {
        let tool = TransferToHumanTool::new();
        let out = tool
            .execute(json!({
                "reason": "超出知识库范围，需人工核实",
                "reply": "非常抱歉，这个问题需要为您转接专属客服，请稍候。"
            }))
            .await
            .unwrap();
        assert!(out.starts_with("<<TRANSFER>>"));
        assert!(out.contains("超出知识库范围"));
        assert!(out.contains("专属客服"));
    }

    #[tokio::test]
    async fn missing_reason_is_error() {
        let tool = TransferToHumanTool::new();
        let err = tool.execute(json!({ "reply": "x" })).await.unwrap_err();
        assert!(err.to_string().contains("missing 'reason'"));
    }

    #[tokio::test]
    async fn missing_reply_is_error() {
        let tool = TransferToHumanTool::new();
        let err = tool.execute(json!({ "reason": "x" })).await.unwrap_err();
        assert!(err.to_string().contains("missing 'reply'"));
    }
}
