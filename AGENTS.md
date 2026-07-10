# seat-agent — AI Agent 上下文

## 项目

seat-agent 是面向客服接待场景的 Agent Runtime，支持文本和语音两种对话模态。
为 OCC（全渠道客服工作台）构建，通过 git 依赖集成。

核心特性：**准确性优先**（RAG 必须，不幻觉）、**速度优先**（流式，延迟有上限）。

## 技术栈

| 层级 | 技术 |
|------|------|
| 核心 Runtime | Rust |
| Trait 实现 | reqwest（LLM/Embedding HTTP API）、PyO3（可选，复用 Python 生态） |
| 通信 | gRPC Bidi Streaming（独立服务模式） |
| 会话存储 | Redis（独立服务模式） |

## 架构原则

### 混合管线 + 受限 Agent Loop

```
客户消息 → 预检索（并行：知识库 + 意图分类）
           ↓
         前置规则检查（检索结果为空/无相关度 → 转人工）
           ↓
         工具分组激活（intent_tags → 激活相关工具组）
           ↓
         Agent Loop（最多 N 轮）:
           ├── 第 1 轮：LLM 决策
           │   ├── 有 tool_call → 执行工具 → 流式输出中间回复 → 注入结果，进入第 2 轮
           │   └── 无 tool_call → 最终回复，结束
           └── 超过轮次上限 → 强制结束，返回当前回复
```

### 准确性约束

- 预检索阶段必须执行，知识库内容是回复的信息基础
- 检索不到相关信息 → 转人工，不编造
- 前置规则检查（Rust，毫秒级）可提前拦截，不进 Agent Loop
- Agent Loop 中的每次工具调用结果都注入上下文

### 速度约束

- 预检索并行执行（目标 <200ms）
- LLM 首 token（目标 <500ms）
- 中间回复模板注入（0ms）
- 工具调用轮次有硬上限：文本 4 轮，语音 2 轮
- 全链路流式输出

### 两种模态

| | 文本模式 | 语音模式 |
|---|---|---|
| 工具调用轮次 | 最多 4 轮 | 最多 2 轮 |
| 模型选择 | 灵活 | 低延迟优先 |
| Agent Loop | 完整 | 受限 |

## 仓库结构

```
seat-agent/
├── Cargo.toml                  # workspace 根配置
├── crates/
│   ├── core/                   # [lib] Agent Loop + Context + Trait 定义
│   │   └── src/
│   │       ├── agent.rs        #   Agent 主循环
│   │       ├── context.rs      #   Context 分层模型 + token 预算
│   │       ├── config.rs       #   AgentConfig
│   │       ├── error.rs        #   错误类型
│   │       └── traits.rs       #   核心 trait 定义
│   ├── tools/                  # [lib] 工具注册 + 分组 + 动态激活
│   │   └── src/
│   │       ├── registry.rs     #   ToolRegistry（分组 + 激活逻辑）
│   │       ├── knowledge.rs    #   知识库检索工具
│   │       ├── business.rs     #   业务系统查询工具
│   │       └── transfer.rs     #   转人工工具
│   ├── memory/                 # [lib] 短期记忆 + 长期记忆 + 摘要生成
│   │   └── src/
│   │       ├── short_term.rs   #   短期记忆（滑动窗口）
│   │       ├── long_term.rs    #   长期记忆（向量检索）
│   │       └── summary.rs      #   会话摘要生成/修正
│   └── server/                 # [bin] 独立 gRPC 服务
│       └── src/
│           ├── main.rs         #   入口
│           ├── grpc.rs         #   gRPC Bidi Streaming server
│           ├── config.rs       #   ConfigProvider 实现
│           ├── redis_store.rs  #   SessionStore 实现
│           ├── embedding.rs    #   EmbeddingClient 实现
│           ├── llm.rs          #   LlmClient 实现（reqwest）
│           └── tts.rs          #   TtsClient 实现
├── docs/
└── examples/
    └── basic_chat/
```

## 硬性约束

1. **Rust 依赖声明在根 Cargo.toml**，子 crate 用 `workspace = true`，不重复声明版本。
2. **Domain 层零外部依赖**：core crate 不依赖 server/infra。
3. **全链路流式**：Agent 的响应天然是 token 流，不是完整字符串。
4. **工具调用轮次硬上限**：由 AgentConfig.max_tool_rounds 控制，不可绕过。
5. **不幻觉**：检索不到知识时，必须转人工或明确告知，不编造。
6. **Context 截断安全**：永远只截断 history 层，system/retrieval/summary 不可截断。
7. **意图分类零延迟**：由 Rust 规则实现，不走 LLM。
8. **单会话单节点**：一次会话的 Agent Loop 在单个节点内存中运行，不跨节点。

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

按功能拆分提交，每个 commit 只包含一个子功能点。commit message 使用英文。
