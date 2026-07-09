# seat-agent — 坐席 Agent Runtime

> 面向客服接待场景的 Agent Runtime，支持文本和语音两种对话模态。

## 定位

坐席 Agent 是一个 **agent runtime**，不是业务系统。它解决的问题是：

> 给定一段客户消息，如何准确、快速地生成回复？

类似 OMP/OpenClaw 的 agent 架构，但专为客服场景设计：
- **准确性优先**：RAG 知识检索是必须的，不幻觉
- **速度优先**：全链路流式，延迟有上限
- **开发者友好**：Rust API + 配置文件，PyO3 桥接 Python AI 生态

## 核心架构

```
客户消息 → 预检索（并行：知识库 + 意图分类）
           ↓
         Agent Loop（最多 N 轮）:
           ├── 输入：原始消息 + 预检索结果 + 工具列表
           ├── LLM 决策：直接回复 / 再查一次 / 转人工
           └── 流式输出
```

## 设计原则

### 1. 准确性：只回答有依据的内容

- 预检索阶段必须执行，知识库内容是回复的信息基础
- 检索不到相关信息 → 转人工，不编造
- Agent Loop 中的每次工具调用结果都注入上下文

### 2. 速度：延迟可预测

- 预检索并行执行（目标 <200ms）
- LLM 首 token（目标 <500ms）
- 工具调用轮次有硬上限（文本 2 轮，语音 1 轮）
- 全链路流式输出

### 3. 两种模态，一个核心

| | 文本模式 | 语音模式 |
|---|---|---|
| 工具调用轮次 | 最多 2 轮 | 最多 1 轮 |
| 模型选择 | 灵活 | 低延迟优先 |
| Agent Loop | 完整 | 受限 |
| 输出 | 流式文本 | 流式文本 → TTS |

## 技术栈

| 层级 | 技术 |
|------|------|
| 核心 Runtime | Rust |
| AI 生态桥接 | PyO3（embedding、向量库、LLM SDK） |
| 配置 | YAML/TOML + Rust API |
| 通信 | gRPC（对外接口） |

## 项目结构

```
seat-agent/
├── Cargo.toml
├── AGENTS.md
├── crates/
│   ├── core/          # Agent runtime 核心：循环、上下文、决策引擎
│   ├── tools/         # 工具注册与调用：知识库、业务 API、转人工
│   ├── memory/        # 记忆系统：短期对话 + 长期向量检索
│   └── bridge/        # PyO3 桥接：Python AI 生态
├── docs/
│   └── ARCHITECTURE.md
└── examples/
    └── basic_chat/    # 基础对话示例
```

## 使用方式

```rust
use seat_agent_core::{Agent, AgentConfig};

let config = AgentConfig::from_file("agent.yaml")?;
let agent = Agent::new(config).await?;

let response = agent.chat("我想退货").await?;
```

## 开发命令

```bash
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets
cargo fmt --check
```
