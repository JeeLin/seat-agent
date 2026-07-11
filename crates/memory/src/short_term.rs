//! 短期记忆：滑动窗口截断 + token 预算保护
//!
//! 在每轮 Agent Loop 开始时对 history 执行滑动窗口截断，
//! 保证 history 层不会无限增长，遵循 Context 分层模型的保护区约束：
//! system / retrieval / history_summary 不可截断，只有 history 可截断。

use seat_agent_core::{Message, MessageRole};

/// 短期记忆管理器
#[derive(Debug, Clone)]
pub struct ShortTermMemory {
    /// 滑动窗口保留的最大消息数（0 表示不限制）
    max_messages: usize,
    /// 保留的最小消息数，截断时至少保留这么多条（避免丢失关键信息）
    min_messages: usize,
}

impl ShortTermMemory {
    pub fn new(max_messages: usize) -> Self {
        Self {
            max_messages,
            min_messages: 2,
        }
    }

    /// 设置最小保留消息数
    pub fn with_min_messages(mut self, min_messages: usize) -> Self {
        self.min_messages = min_messages;
        self
    }

    /// 对 history 执行滑动窗口截断
    ///
    /// 规则：
    /// - 若 history 长度 ≤ max_messages，原样返回
    /// - 否则保留最后 min_messages 条，其余从头部移除
    ///
    /// 注意：此方法不截断 system/retrieval/summary，仅处理传入的 history 切片。
    pub fn trim(&self, history: &[Message]) -> Vec<Message> {
        if self.max_messages == 0 || history.len() <= self.max_messages {
            return history.to_vec();
        }

        let keep = self.min_messages.min(history.len());
        let start = history.len() - keep;

        // 保留最后 `keep` 条；对这段保留区间，system 角色消息不应出现于 history，
        // 但防御性地跳过非用户/助手消息可保持语义清晰。
        history[start..]
            .iter()
            .filter(|m| matches!(m.role, MessageRole::User | MessageRole::Assistant))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: MessageRole, content: &str) -> Message {
        Message {
            role,
            content: content.to_string(),
            tool_calls: None,
            tool_call_id: None,
        }
    }

    #[test]
    fn trim_keeps_all_when_under_limit() {
        let stm = ShortTermMemory::new(10);
        let history: Vec<Message> = (0..5)
            .map(|i| msg(MessageRole::User, &format!("m{}", i)))
            .collect();
        let trimmed = stm.trim(&history);
        assert_eq!(trimmed.len(), 5);
    }

    #[test]
    fn trim_drops_oldest_beyond_window() {
        let stm = ShortTermMemory::new(4);
        let history: Vec<Message> = (0..10)
            .map(|i| msg(MessageRole::User, &format!("m{}", i)))
            .collect();
        let trimmed = stm.trim(&history);
        assert_eq!(trimmed.len(), 2); // min_messages=2
        assert_eq!(trimmed[0].content, "m8");
        assert_eq!(trimmed[1].content, "m9");
    }

    #[test]
    fn trim_respects_min_messages_over_max() {
        let stm = ShortTermMemory::new(2).with_min_messages(5);
        let history: Vec<Message> = (0..10)
            .map(|i| msg(MessageRole::User, &format!("m{}", i)))
            .collect();
        let trimmed = stm.trim(&history);
        // max_messages(2) < min_messages(5)，保留 min_messages 条
        assert_eq!(trimmed.len(), 5);
        assert_eq!(trimmed[0].content, "m5");
    }

    #[test]
    fn trim_empty_history() {
        let stm = ShortTermMemory::new(2);
        let history: Vec<Message> = vec![];
        assert!(stm.trim(&history).is_empty());
    }

    fn trim_filters_non_conversational_roles() {
        // 若 history 末尾混入 system 角色，截断后（保留最后 min 条）被 filter 剔除
        let stm = ShortTermMemory::new(4).with_min_messages(4);
        let mut history: Vec<Message> = (0..10)
            .map(|i| msg(MessageRole::User, &format!("m{}", i)))
            .collect();
        history.push(msg(MessageRole::System, "should-be-removed"));
        let trimmed = stm.trim(&history);
        assert!(trimmed.iter().all(|m| m.role != MessageRole::System));
        // 保留最后 4 条 user 消息（m7-m10），System 被剔除
        assert_eq!(trimmed.len(), 4);
        assert_eq!(trimmed[3].content, "m10");
    }
}
