use std::collections::VecDeque;

use crate::config::{AgentConfig, Modality};
use crate::traits::{KnowledgeResult, MessageRole, ToolCall};

/// Agent 消息
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub tool_call_id: Option<String>,
}

/// Agent 输入
#[derive(Debug)]
pub struct AgentInput {
    pub session_id: String,
    pub message: Message,
}

/// Agent 事件（输出流）
#[derive(Debug)]
pub enum AgentEvent {
    /// 流开始
    StreamStart,
    /// 文本 token
    Token(String),
    /// 流结束
    StreamEnd,
    /// 工具调用开始
    ToolCallStart {
        tool_name: String,
        arguments: String,
    },
    /// 工具调用结束
    ToolCallEnd { tool_name: String, result: String },
    /// 转人工
    TransferToHuman { reason: String },
    /// 错误
    Error(String),
}

/// Context 分层模型
///
/// - system: 系统指令，不可截断
/// - retrieval: 检索结果，不可截断
/// - history_summary: 历史摘要，不可截断
/// - history: 对话历史，可截断
/// - working: 当前轮工作区（工具调用结果等）
#[derive(Debug)]
pub struct Context {
    /// 会话 ID
    pub session_id: String,

    /// 对话模态
    pub modality: Modality,

    /// 系统消息
    pub system: Vec<Message>,

    /// 检索结果
    pub retrieval: Vec<KnowledgeResult>,

    /// 历史摘要
    pub history_summary: Option<String>,

    /// 对话历史
    pub history: VecDeque<Message>,

    /// 当前轮工作区
    pub working: Vec<Message>,

    /// 配置
    pub config: AgentConfig,
}

impl Context {
    /// 创建新的 Context
    pub fn new(session_id: String, config: AgentConfig) -> Self {
        let system_prompt = config
            .system_prompt
            .clone()
            .unwrap_or_else(|| build_default_system_prompt(&config.modality));

        Self {
            session_id,
            modality: config.modality.clone(),
            system: vec![Message {
                role: MessageRole::System,
                content: system_prompt,
                tool_calls: None,
                tool_call_id: None,
            }],
            retrieval: Vec::new(),
            history_summary: None,
            history: VecDeque::new(),
            working: Vec::new(),
            config,
        }
    }

    /// 添加用户消息到历史
    pub fn add_user_message(&mut self, content: String) {
        self.history.push_back(Message {
            role: MessageRole::User,
            content,
            tool_calls: None,
            tool_call_id: None,
        });
    }

    /// 添加助手消息到历史
    pub fn add_assistant_message(
        &mut self,
        content: String,
        tool_calls: Option<Vec<crate::traits::ToolCall>>,
    ) {
        self.history.push_back(Message {
            role: MessageRole::Assistant,
            content,
            tool_calls,
            tool_call_id: None,
        });
    }

    /// 添加工具调用结果到工作区
    pub fn add_tool_result(&mut self, tool_call_id: String, content: String) {
        self.working.push(Message {
            role: MessageRole::Tool,
            content,
            tool_calls: None,
            tool_call_id: Some(tool_call_id),
        });
    }

    /// 设置检索结果
    pub fn set_retrieval(&mut self, results: Vec<KnowledgeResult>) {
        self.retrieval = results;
    }

    /// 设置历史摘要
    pub fn set_history_summary(&mut self, summary: Option<String>) {
        self.history_summary = summary;
    }

    /// 构建发送给 LLM 的消息列表
    pub fn build_messages(&self) -> Vec<crate::traits::LlmMessage> {
        let mut messages = Vec::new();

        // 1. System messages
        for msg in &self.system {
            messages.push(crate::traits::LlmMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 2. History summary (as system message)
        if let Some(summary) = &self.history_summary {
            messages.push(crate::traits::LlmMessage {
                role: MessageRole::System,
                content: format!("历史会话摘要：{}", summary),
                tool_calls: None,
                tool_call_id: None,
            });
        }

        // 3. Retrieval results (as system message)
        if !self.retrieval.is_empty() {
            let retrieval_content = self
                .retrieval
                .iter()
                .map(|r| format!("【{}】{}", r.id, r.content))
                .collect::<Vec<_>>()
                .join("\n");
            messages.push(crate::traits::LlmMessage {
                role: MessageRole::System,
                content: format!("知识库检索结果：\n{}", retrieval_content),
                tool_calls: None,
                tool_call_id: None,
            });
        }
        // 4. History messages
        for msg in &self.history {
            messages.push(crate::traits::LlmMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                tool_calls: msg.tool_calls.clone(),
                tool_call_id: msg.tool_call_id.clone(),
            });
        }

        // 5. Working messages (tool results)
        for msg in &self.working {
            messages.push(crate::traits::LlmMessage {
                role: msg.role.clone(),
                content: msg.content.clone(),
                tool_calls: None,
                tool_call_id: msg.tool_call_id.clone(),
            });
        }

        messages
    }

    /// 清空工作区
    pub fn clear_working(&mut self) {
        self.working.clear();
    }

    /// 将工作区内容移动到历史
    pub fn flush_working_to_history(&mut self) {
        for msg in self.working.drain(..) {
            self.history.push_back(msg);
        }
    }

    /// 截断历史消息
    pub fn truncate_history(&mut self) {
        let min_messages = self.config.min_history_messages;
        while self.history.len() > min_messages {
            self.history.pop_front();
        }
    }

    /// 估算 token 数（简化版本，按字符数/4 估算）
    pub fn estimate_tokens(&self) -> usize {
        let mut total = 0;

        for msg in &self.system {
            total += msg.content.len() / 4;
        }

        if let Some(summary) = &self.history_summary {
            total += summary.len() / 4;
        }

        for result in &self.retrieval {
            total += result.content.len() / 4;
        }

        for msg in &self.history {
            total += msg.content.len() / 4;
        }

        for msg in &self.working {
            total += msg.content.len() / 4;
        }

        total
    }
}

/// 构建默认系统提示词
fn build_default_system_prompt(modality: &Modality) -> String {
    let base = r#"你是客服助手。回复必须简洁、专业、有温度。

## 话术规则

### 转人工话术（必须委婉）
- ✅ "好的，为您转接专属客服"
- ✅ "这个操作需要专员为您处理，正在转接"
- ✅ "您的问题正在加急处理中，请稍候"
- ❌ "我处理不了，转人工"
- ❌ "系统出错了"
- ❌ "我不知道"

### 通用话术原则
1. 使用"您"而不是"你"
2. 使用"我们"而不是"我"
3. 避免负面词汇（不行、不能、不知道）
4. 用"正在为您..."代替"我需要..."
5. 用"建议您..."代替"你应该...""#;

    match modality {
        Modality::Text => format!("{}\n\n回复可以稍长，分段清晰。", base),
        Modality::Voice => format!("{}\n\n回复必须简短（<100字），口语化，适合语音播放。", base),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::KnowledgeResult;
    use std::collections::HashMap;

    fn text_config() -> AgentConfig {
        AgentConfig::default()
    }

    fn voice_config() -> AgentConfig {
        AgentConfig::voice()
    }

    #[test]
    fn test_context_new_text_mode() {
        let ctx = Context::new("s1".into(), text_config());
        assert_eq!(ctx.session_id, "s1");
        assert_eq!(ctx.system.len(), 1);
        assert_eq!(ctx.system[0].role, MessageRole::System);
        assert!(ctx.system[0].content.contains("客服助手"));
        assert!(ctx.system[0].content.contains("分段清晰"));
    }

    #[test]
    fn test_context_new_voice_mode() {
        let ctx = Context::new("s2".into(), voice_config());
        assert_eq!(ctx.modality, Modality::Voice);
        assert!(ctx.system[0].content.contains("简短"));
    }

    #[test]
    fn test_context_custom_system_prompt() {
        let mut config = text_config();
        config.system_prompt = Some("Custom prompt".into());
        let ctx = Context::new("s3".into(), config);
        assert_eq!(ctx.system[0].content, "Custom prompt");
    }

    #[test]
    fn test_add_user_message() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_user_message("hello".into());
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].role, MessageRole::User);
        assert_eq!(ctx.history[0].content, "hello");
    }

    #[test]
    fn test_add_assistant_message() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_assistant_message("hi".into(), None);
        assert_eq!(ctx.history.len(), 1);
        assert_eq!(ctx.history[0].role, MessageRole::Assistant);
    }

    #[test]
    fn test_add_tool_result() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_tool_result("call_1".into(), "result".into());
        assert_eq!(ctx.working.len(), 1);
        assert_eq!(ctx.working[0].role, MessageRole::Tool);
        assert_eq!(ctx.working[0].tool_call_id.as_deref(), Some("call_1"));
    }

    #[test]
    fn test_build_messages_order() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.set_history_summary(Some("past summary".into()));
        ctx.set_retrieval(vec![KnowledgeResult {
            id: "k1".into(),
            content: "retrieval text".into(),
            score: 0.9,
            metadata: HashMap::new(),
        }]);
        ctx.add_user_message("user msg".into());
        ctx.add_tool_result("call_1".into(), "tool result".into());

        let msgs = ctx.build_messages();
        // Expected order: system, summary, retrieval, history(user), working(tool)
        assert!(msgs.len() >= 5);
        assert_eq!(msgs[0].role, MessageRole::System);
        // msgs[1] = summary (System role with "历史会话摘要")
        assert!(
            msgs[1].content.contains("历史会话摘要"),
            "Expected summary at index 1"
        );
        // msgs[2] = retrieval
        assert!(
            msgs[2].content.contains("retrieval text"),
            "Expected retrieval at index 2"
        );
        // msgs[3] = user history
        assert_eq!(msgs[3].role, MessageRole::User);
        assert_eq!(msgs[3].content, "user msg");
        // msgs[4] = tool result
        assert_eq!(msgs[4].role, MessageRole::Tool);
    }

    #[test]
    fn test_build_messages_without_optional_layers() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_user_message("msg".into());
        let msgs = ctx.build_messages();
        // system + history only
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, MessageRole::System);
        assert_eq!(msgs[1].role, MessageRole::User);
    }

    #[test]
    fn test_flush_working_to_history() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_tool_result("c1".into(), "r1".into());
        ctx.add_tool_result("c2".into(), "r2".into());
        assert_eq!(ctx.working.len(), 2);
        assert_eq!(ctx.history.len(), 0);

        ctx.flush_working_to_history();
        assert_eq!(ctx.working.len(), 0);
        assert_eq!(ctx.history.len(), 2);
        assert_eq!(ctx.history[0].tool_call_id.as_deref(), Some("c1"));
    }

    #[test]
    fn test_clear_working() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_tool_result("c1".into(), "r1".into());
        ctx.clear_working();
        assert!(ctx.working.is_empty());
    }

    #[test]
    fn test_truncate_history() {
        let mut ctx = Context::new("s1".into(), text_config());
        for i in 0..10 {
            ctx.add_user_message(format!("msg{}", i));
        }
        assert_eq!(ctx.history.len(), 10);

        ctx.truncate_history();
        // min_history_messages defaults to 2
        assert_eq!(ctx.history.len(), 2);
        // Should keep the last 2
        assert_eq!(ctx.history[0].content, "msg8");
        assert_eq!(ctx.history[1].content, "msg9");
    }

    #[test]
    fn test_truncate_history_preserves_min() {
        let mut ctx = Context::new("s1".into(), text_config());
        ctx.add_user_message("a".into());
        ctx.add_user_message("b".into());
        ctx.truncate_history();
        assert_eq!(ctx.history.len(), 2);
    }

    #[test]
    fn test_estimate_tokens() {
        let mut ctx = Context::new("s1".into(), text_config());
        // System prompt is ~200 chars → ~50 tokens
        let base = ctx.estimate_tokens();
        assert!(base > 0);

        ctx.add_user_message("hello world".into()); // 11 chars → 2 tokens
        let after_msg = ctx.estimate_tokens();
        assert!(after_msg > base);

        ctx.set_history_summary(Some("a summary here".into())); // 14 chars → 3 tokens
        let after_summary = ctx.estimate_tokens();
        assert!(after_summary > after_msg);

        ctx.set_retrieval(vec![KnowledgeResult {
            id: "k1".into(),
            content: "retrieval content".into(),
            score: 0.9,
            metadata: HashMap::new(),
        }]);
        let after_retrieval = ctx.estimate_tokens();
        assert!(after_retrieval > after_summary);
    }

    #[test]
    fn test_estimate_tokens_empty_context() {
        let ctx = Context::new("s1".into(), text_config());
        // System prompt alone should have tokens
        assert!(ctx.estimate_tokens() > 0);
    }
}
