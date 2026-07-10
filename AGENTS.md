# seat-agent — AI Agent 上下文

## 项目

seat-agent 是面向客服接待场景的 Agent Runtime，支持文本和语音两种对话模态。

核心特性：**准确性优先**（RAG 必须，不幻觉）、**速度优先**（流式，延迟有上限）。

## 技术栈

| 层级 | 技术 |
|------|------|
| 核心 Runtime | Rust |
| Trait 实现 | reqwest（LLM/Embedding HTTP API）、PyO3（可选，复用 Python 生态） |
| 配置 | YAML/TOML + Rust API |
| 通信 | gRPC Bidi Streaming（对外接口） |
| 会话存储 | Redis（可选） |"{"输入

## 架构原则

### 混合管线 + 受限 Agent Loop

```
客户消息 → 预检索（并行：知识库 + 意图分类）
           ↓
         Agent Loop（最多 N 轮）:
           ├── 输入：原始消息 + 预检索结果 + 工具列表
           ├── LLM 决策：直接回复 / 再查一次 / 转人工
           └── 流式输出
```

### 准确性约束

- 预检索阶段必须执行，知识库内容是回复的信息基础
- 检索不到相关信息 → 转人工，不编造
- Agent Loop 中的每次工具调用结果都注入上下文

### 速度约束

- 预检索并行执行（目标 <200ms）
- LLM 首 token（目标 <500ms）
- 工具调用轮次有硬上限：文本 2 轮，语音 1 轮
- 全链路流式输出

### 两种模态

| | 文本模式 | 语音模式 |
|---|---|---|
| 工具调用轮次 | 最多 2 轮 | 最多 1 轮 |
| 模型选择 | 灵活 | 低延迟优先 |
| Agent Loop | 完整 | 受限 |

## 仓库结构

```
seat-agent/
├── Cargo.toml                         # Rust workspace 根配置
├── AGENTS.md                          # 本文件
├── crates/
│   ├── core/                          # Agent runtime 核心
│   │   └── src/
│   │       ├── agent.rs               # Agent 主循环
│   │       ├── context.rs             # 上下文管理（消息历史、工具结果）
│   │       ├── config.rs              # Agent 配置（模式、轮次、模型）
│   │       └── error.rs               # 错误类型
│   ├── tools/                         # 工具注册与调用
│   │   └── src/
│   │       ├── registry.rs            # 工具注册表
│   │       ├── knowledge.rs           # 知识库检索工具
│   │       ├── business.rs            # 业务系统查询工具
│   │       └── transfer.rs            # 转人工工具
│   ├── memory/                        # 记忆系统
│   │   └── src/
│   │       ├── short_term.rs          # 短期记忆（对话内）
│   │       └── long_term.rs           # 长期记忆（向量检索）
│   └── bridge/                        # PyO3 桥接
│       └── src/
│           ├── embedding.rs           # Python embedding 桥接
│           └── llm.rs                 # Python LLM SDK 桥接
├── docs/
│   └── ARCHITECTURE.md
└── examples/
    └── basic_chat/
```

## 硬性约束

1. **Rust 依赖声明在根 Cargo.toml**，子 crate 用 `workspace = true`，不重复声明版本。
2. **Domain 层零外部依赖**：core crate 不依赖 infra/bridge。
3. **全链路流式**：Agent 的响应天然是 token 流，不是完整字符串。
4. **工具调用轮次硬上限**：由 AgentConfig.max_tool_rounds 控制，不可绕过。
5. **不幻觉**：检索不到知识时，必须转人工或明确告知无信息，不编造。

## 开发流程

### 质量门禁

```bash
cargo fmt --check
cargo clippy --workspace --all-targets
cargo test --workspace
```

### 常用命令

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## 提交约定

按功能拆分提交，每个 commit 只包含一个子功能点。
