use std::collections::VecDeque;

use crate::config::{AgentConfig, Modality};
use crate::traits::{KnowledgeResult, MessageRole};

/// Agent 消息
#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
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
            tool_call_id: None,
        });
    }

    /// 添加助手消息到历史
    pub fn add_assistant_message(&mut self, content: String) {
        self.history.push_back(Message {
            role: MessageRole::Assistant,
            content,
            tool_call_id: None,
        });
    }

    /// 添加工具调用结果到工作区
    pub fn add_tool_result(&mut self, tool_call_id: String, content: String) {
        self.working.push(Message {
            role: MessageRole::Tool,
            content,
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
                tool_calls: None,
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
