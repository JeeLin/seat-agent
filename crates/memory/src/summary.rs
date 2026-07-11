//! 会话摘要生成/修正
//!
//! 会话结束时生成/修正长期记忆摘要，不增加实时延迟。
//! 若已有摘要（existing），则在其基础上修正为最新事实。

use seat_agent_core::{LlmClient, LlmMessage, LlmRequest, LlmStreamChunk, MessageRole};

/// 摘要生成器
pub struct SummaryGenerator<L: LlmClient> {
    llm: L,
}

impl<L: LlmClient> SummaryGenerator<L> {
    pub fn new(llm: L) -> Self {
        Self { llm }
    }

    /// 根据当前会话历史生成或修正摘要
    ///
    /// - `history`：当前会话的完整历史
    /// - `existing`：已有摘要（可选），提供则在其基础上修正
    ///
    /// 返回简洁的结构化摘要文本。
    pub async fn generate(
        &self,
        history: &[LlmMessage],
        existing: Option<&str>,
    ) -> seat_agent_core::Result<String> {
        let history_text = history
            .iter()
            .map(|m| format!("{:?}: {}", m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        let prompt = match existing {
            Some(existing) => format!(
                "以下是客户的历史摘要和最新对话，请修正并合并为不超过100字的最新摘要：\n\n\
                 已有摘要：{}\n\n最新对话：\n{}",
                existing, history_text
            ),
            None => format!(
                "请将以下对话总结为不超过100字的结构化摘要（包含客户意图和关键事实）：\n\n{}",
                history_text
            ),
        };

        let messages = vec![LlmMessage {
            role: MessageRole::User,
            content: prompt,
            tool_calls: None,
            tool_call_id: None,
        }];

        let request = LlmRequest {
            messages,
            tools: vec![],
            max_tokens: Some(200),
            temperature: Some(0.3),
            stream: false,
        };

        let mut stream = self.llm.chat_stream(request).await?;
        let mut summary = String::new();

        while let Some(chunk) = futures::StreamExt::next(&mut stream).await {
            match chunk? {
                LlmStreamChunk::Content(text) => summary.push_str(&text),
                LlmStreamChunk::Done { .. } => break,
                _ => {} // 摘要生成不使用工具调用
            }
        }

        Ok(summary.trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use seat_agent_core::MockLlmClient;

    #[tokio::test]
    async fn generate_new_summary() {
        let llm = MockLlmClient::new(vec!["客户咨询订单退款".to_string()]);
        let gen = SummaryGenerator::new(llm);

        let history = vec![LlmMessage {
            role: MessageRole::User,
            content: "我的订单退款还没到账".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let summary = gen.generate(&history, None).await.unwrap();
        assert!(summary.contains("退款"));
    }

    #[tokio::test]
    async fn generate_merged_summary() {
        let llm = MockLlmClient::new(vec!["客户已收到退款，问题已解决".to_string()]);
        let gen = SummaryGenerator::new(llm);

        let history = vec![LlmMessage {
            role: MessageRole::User,
            content: "退款已到账，谢谢".to_string(),
            tool_calls: None,
            tool_call_id: None,
        }];

        let summary = gen
            .generate(&history, Some("客户咨询退款"))
            .await
            .unwrap();
        assert!(summary.contains("退款"));
    }
}
