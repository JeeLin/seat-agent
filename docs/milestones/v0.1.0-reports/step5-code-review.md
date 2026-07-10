# 代码审查报告 - v0.1.0 基础架构

## 审查范围

| 文件 | 内容 |
|------|------|
| `crates/core/src/traits.rs` | 核心 trait 定义（LlmClient, KnowledgeStore, MemoryStore, VectorStore, Tool 等） |
| `crates/core/src/config.rs` | AgentConfig、Modality |
| `crates/core/src/error.rs` | AgentError、Result 类型别名 |
| `crates/core/src/context.rs` | Context 分层模型（5层上下文） |
| `crates/core/src/agent.rs` | Agent 主循环 |
| `crates/tools/src/registry.rs` | ToolRegistry、ToolConfig、JSON 加载 |

## 编译状态

- ✅ `cargo check --workspace` 通过
- ✅ `cargo clippy --workspace --all-targets` 无警告
- ✅ `cargo fmt --check` 通过

## 发现

| 严重程度 | 文件 | 行号 | 问题 | 建议 |
|----------|------|------|------|------|
| 🔴 | agent.rs | 168 | `context.clear_working()` 在工具执行后清空工作区，导致工具结果在下一轮 LLM 调用时已丢失，LLM 无法看到工具返回值。Agent Loop 的多轮工具调用流程完全失效 | 改为 `context.flush_working_to_history()` 将工具结果移入历史 |
| 🔴 | agent.rs / context.rs | 129-131, build_messages | Assistant 消息仅存储文本内容，`tool_calls` 始终为 `None`。LLM 无法看到自己之前发出的工具调用，破坏多轮对话连贯性 | 在 Message 中存储 tool_calls 信息，`build_messages()` 序列化时输出 |
| 🟡 | context.rs | 199-206 | `truncate_history()` 仅按 `min_history_messages` 截断，未检查 `total_token_limit`。`estimate_tokens()` 已实现但未使用，Token 预算机制形同虚设 | 在截断逻辑中加入 token 预算检查 |
| 🟡 | agent.rs | 17 | `memory` 字段已声明并有 setter，但在 `on_message()` 中从未使用。v0.1.0 未包含内存功能，该字段为死代码 | 要么移除该字段，要么添加 TODO 注释明确集成点 |
| 🟡 | registry.rs | 53-56 | `tools` HashMap 和 `configs` Vec 独立维护，`from_json()` 只填充 configs，`register()` 只填充 tools。两套数据可能不同步，导致 `tool_definitions()` 和 `enabled_configs()` 视图不一致 | 统一数据源，或在设计上明确 configs 仅用于 UI 展示 |
| 🟡 | — | — | 里程碑文档要求的单元测试（token 计算、截断逻辑、mock LLM Agent Loop）全部缺失 | 补充单元测试，至少覆盖 Context 和 Agent 核心逻辑 |
| 🟢 | agent.rs | 136-138 | 工具查找使用 `Vec::iter().find()` 做线性扫描，每次调用还触发 `definition()` 克隆 | 改用 `HashMap<String, Box<dyn Tool>>` 做 O(1) 查找 |
| 🟢 | config.rs | 8-29 | 所有字段 `pub`，无构造验证。外部可设置 `max_rounds=0` 或 `min_history_messages > total_token_limit` 等无效配置 | 添加 `new()` 或 `try_from()` 构造器做参数校验 |
| 🟢 | context.rs | 211-227 | `estimate_tokens()` 按 `len()/4` 估算，中文 UTF-8 每字符 3 字节但约 1-2 token，导致中文内容估算偏高约 2 倍 | 后续引入 tiktoken-rs 或按字符类型分别估算 |

## 发现详情

### 🔴 P0-1: Agent Loop 工具结果被丢弃

**文件**: `crates/core/src/agent.rs:168`

`context.clear_working()` 在每轮工具执行结束后清空工作区。由于工具结果仅存放在 working 层，下一轮 `build_messages()` 时 LLM 收不到任何工具返回值。正确的调用应为 `context.flush_working_to_history()`（已在 context.rs 中实现但从未被调用）。

### 🔴 P0-2: Assistant 工具调用元数据丢失

**文件**: `crates/core/src/agent.rs:129-131` + `context.rs build_messages()`

LLM 返回工具调用后，仅文本内容通过 `add_assistant_message()` 存入上下文。`LlmMessage` 的 `tool_calls` 字段在 `build_messages()` 中始终为 `None`。这导致 LLM 无法看到自己之前的工具调用决策，无法做多轮推理。

### 🟡 P1-1: Token 预算未生效

**文件**: `crates/core/src/context.rs:199-206`

`AgentConfig` 定义了 `total_token_limit: 8000`，`Context` 实现了 `estimate_tokens()`，但 `truncate_history()` 只检查消息数量（`min_history_messages`），不检查 token 总量。在长对话中可能超出 LLM 上下文窗口。

## 结论

❌ **需要修复**

两个 🔴 P0 问题均位于 Agent 核心循环，导致多轮工具调用流程完全失效：
1. 工具结果在下一轮 LLM 调用前被清空
2. 工具调用元数据未序列化到消息历史

修复建议：
- `agent.rs:168` → 将 `context.clear_working()` 改为 `context.flush_working_to_history()`
- `context.rs` → 在 `Message` 中增加 tool_calls 字段，`build_messages()` 序列化时输出
- 补充至少覆盖 Agent Loop 和 Context 截断的单元测试
