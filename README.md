# seat-agent

> AI Agent Runtime for Customer Service

seat-agent 是面向客服接待场景的 Agent Runtime，支持文本和语音两种对话模态。

## 核心特性

- **准确性优先** — RAG 必须，不幻觉，检索不到就转人工
- **速度优先** — 全链路流式，延迟有上限
- **可嵌入可独立** — 既是 Rust SDK（OCC 集成），也可作为独立 gRPC 服务运行

## 快速开始

### 作为库（推荐）

```rust
use seat_agent_core::{Agent, AgentConfig};

let mut agent = Agent::new(AgentConfig::default()).await?;
agent.register_tool(Box::new(KnowledgeSearchTool));

let response = agent.chat("我想退货").await?;
```

### 作为独立服务

```bash
seat-agent-server --config agent.yaml
```

## 技术栈

| 层级 | 技术 |
|------|------|
| 核心 Runtime | Rust |
| Trait 实现 | reqwest（LLM/Embedding HTTP API）、PyO3（可选，复用 Python 生态） |
| 通信 | gRPC Bidi Streaming（独立服务模式） |
| 会话存储 | Redis（独立服务模式） |

## 架构

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

### Context 分层模型

```
Context
├── system: Vec<Message>              // 系统指令，不可截断
├── retrieval: Vec<SearchResult>      // 当前轮预检索结果，不可截断
├── history_summary: String           // 历史会话摘要，不可截断
├── history: VecDeque<Message>        // 当前会话对话，唯一可截断
└── long_term: Box<dyn KnowledgeStore> // 长期存储后端
```

### 两种模态

| | 文本模式 | 语音模式 |
|---|---|---|
| 工具调用轮次 | 最多 4 轮 | 最多 2 轮 |
| 模型选择 | 灵活 | 低延迟优先 |
| Agent Loop | 完整 | 受限 |

## 仓库结构

```
seat-agent/
├── crates/
│   ├── core/      # [lib] Agent Loop + Context + Trait 定义
│   ├── tools/     # [lib] 工具注册 + 分组 + 动态激活
│   ├── memory/    # [lib] 短期/长期记忆 + 摘要生成
│   └── server/    # [bin] 独立 gRPC 服务
├── docs/
└── examples/
```

## 硬性约束

1. Core 层零外部依赖
2. 全链路流式输出
3. 工具调用轮次硬上限
4. 不幻觉，检索不到必须转人工
5. Context 截断只作用于 history 层
6. 意图分类零延迟（Rust 规则）
7. 单会话单节点

## 与 OCC 集成

seat-agent 为 [OCC](https://github.com/JeeLin/OCC) 构建，通过 git 依赖集成：

```toml
# OCC/Cargo.toml
[workspace.dependencies]
seat-agent-core = { git = "ssh://git@ssh.github.com:443/JeeLin/seat-agent.git", path = "crates/core" }
seat-agent-tools = { git = "ssh://git@ssh.github.com:443/JeeLin/seat-agent.git", path = "crates/tools" }
```

本地开发时通过 `[patch]` 覆盖为本地路径。

## 开发

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
```

## License

MIT
