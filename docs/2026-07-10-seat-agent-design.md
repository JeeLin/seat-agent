# seat-agent 架构设计

> 2026-07-10 · 基于团队讨论确定的架构决策

---

## 1. 项目定位

seat-agent 是面向客服接待场景的 **Agent Runtime**，支持文本和语音两种对话模态。

核心特性：
- **准确性优先** — RAG 必须，不幻觉，检索不到就转人工
- **速度优先** — 全链路流式，延迟有上限
- **可嵌入可独立** — 既是 Rust SDK，也可作为独立 gRPC 服务运行

---

## 2. 部署形态

SDK + 独立服务双模式。

```
┌──────────────────────────────────────────────────────────┐
│  seat-agent (library / SDK)                              │
│  ┌────────────┐  ┌──────────┐  ┌──────────┐  ┌────────┐ │
│  │    core    │  │  tools   │  │  memory  │  │ bridge │ │
│  │ (纯库)     │  │ (工具)   │  │ (记忆)   │  │ (PyO3) │ │
│  └────────────┘  └──────────┘  └──────────┘  └────────┘ │
│                          ↑ 全部通过 trait 解耦              │
│                                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │              server (可选 crate)                      │ │
│  │  gRPC server + 配置热加载 + 生命周期管理              │ │
│  └─────────────────────────────────────────────────────┘ │
└──────────────────────────────────────────────────────────┘
```

- **SDK 模式**：集成方自行创建 `Agent` 实例，调用 `agent.chat(message)` 获取流式响应，自己管理通信层（HTTP/gRPC/WebSocket 等）
- **独立服务**：`server` crate 封装 gRPC 服务，集成方只需 `seat_agent_server::serve(config).await`

---

## 3. Context 分层模型

上下文按职责分层，**截断历史时只截 history 层**：
- system、retrieval、history_summary → 只读保护区，不可截断
- history → 滑动窗口，是唯一可截断的层
- long_term → 存储后端，支撑 retrieval 和 summary 的数据来源

```
Context
├── system: Vec<Message>              // 系统指令 + 角色设定，始终保留，不可截断
├── retrieval: Vec<SearchResult>      // 当前轮预检索结果（来自 long_term + 知识库），不可截断
├── history_summary: String           // 历史会话摘要，从 long_term 加载，始终保留
├── history: VecDeque<Message>        // 当前会话对话，滑动窗口 + token 预算截断
└── long_term: Box<dyn KnowledgeStore> // 存储后端（支撑 retrieval 和 summary 的数据来源）
```

### 3.1 Token 预算分配

每轮 LLM 调用前，根据总 token 上限分配：

```
总预算 = total_token_limit
  - system 固定开销
  - history_summary（固定长度，通常 <200 token）
  - retrieval 结果（当前轮检索结果）
  ──────────────────────────
  = history 可用预算（滑动窗口截断）
```

### 3.2 截断策略

1. 先计算 system + retrieval + history_summary 的 token 总量
2. 剩余预算分配给 history
3. history 从旧到新丢弃消息，但不能低于 `min_history_messages`（默认 2 条，保证最近的上下文不丢失）
4. 如果最小窗口都放不下，触发异常告警（系统配置错误）

---

## 4. 对话历史摘要机制

### 4.1 摘要生成时机

**会话结束时生成/修正摘要**，不增加实时延迟：

```
会话 N 结束
  → 将 history_summary + 当前会话 history 一起传给 LLM
  → 生成新摘要（覆盖旧摘要）
  → 存入长期记忆（向量存储）
```

### 4.2 实时矛盾解决

会话过程中遇到历史摘要信息过时的情况，**通过工具调用实时验证**，不需要修正摘要：

```
客户："退款还没到账"
  ↓ 长期记忆检索 → 历史摘要："客户上次咨询退款问题"
  ↓ 工具调用 → 查询订单系统 → 退款已于03-10到账
  ↓ 回复："您的退款已于3月10日到账"
  ↓ （会话结束时摘要自然更新为新事实）
```

### 4.3 摘要存储格式（草案）

```yaml
# long-term memory 中的一条摘要
customer_id: "CUST_12345"
session_id: "sess_20260308_refund"
created_at: "2026-03-08T14:30:00Z"
summary: "客户张三，2026-03-08 咨询订单 #12345 退款问题，已提交退款申请，等待 3-5 个工作日。"
intent_tags: ["退款", "订单查询"]
```

---

## 5. Agent Loop 设计

### 5.1 整体流程

```
客户消息
  ↓
┌─────────────────────────────────────────┐
│  预检索阶段（并行）                       │
│  ├── 知识库检索 (KnowledgeStore)          │
│  ├── 意图分类 (Rust 规则，0ms)            │
│  └── 长期记忆检索（按需）                 │
│                                         │
│  前置规则检查（Rust，毫秒级）:            │
│  └── 检索结果为空或无相关度 → 转人工       │
│       （不进 Agent Loop，节省 LLM 调用）  │
└─────────────────────────────────────────┘
  ↓
┌─────────────────────────────────────────┐
│  构建 Context                            │
│  system + retrieval + history_summary   │
│  + intent 标签 + 用户消息               │
└─────────────────────────────────────────┘
  ↓
┌─────────────────────────────────────────┐
│  Agent Loop（最多 N 轮）                 │
│  ├── 第 1 轮 LLM 调用                    │
│  │   ├── 有 tool_call? → 执行工具         │
│  │   │   → 流式输出中间回复               │
│  │   │   → 注入结果，进入第 2 轮           │
│  │   └── 无 tool_call? → 最终回复，结束   │
│  ├── 第 2 轮 ...                         │
│  └── 超过轮次上限 → 强制结束，返回当前回复 │
└─────────────────────────────────────────┘
  ↓
  流式输出最终回复
```

### 5.2 中间回复模板

工具注册时声明中间回复，Agent 执行工具后自动流式输出，零 LLM 延迟：

```rust
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub intermediate_reply: Option<String>,  // "正在查询您的订单..."
    pub parameters: JsonSchema,
}
```

示例：
| 工具 | intermediate_reply |
|---|---|
| knowledge_search | "正在查阅知识库..." |
| order_query | "正在查询您的订单，请稍候..." |
| transfer_to_human | "正在为您转接人工客服..." |

### 5.3 工具调用轮次上限

可配置，按模态区分：

| 模态 | 默认上限 | 说明 |
|---|---|---|
| 文本 | 4 轮 | 每轮间有中间回复，客户感知为正常等待 |
| 语音 | 2 轮 | 语音场景更敏感，上限更低 |

超限时强制结束 Agent Loop，返回当前已有内容。

### 5.4 LLM 响应格式解析

**Trait 抽象，不绑定具体 LLM 供应商**：

```rust
#[async_trait]
pub trait LlmClient: Send + Sync {
    async fn complete(&self, request: CompletionRequest) -> LlmResponse;
    async fn stream(&self, request: CompletionRequest) -> Pin<Box<dyn Stream<Item = LlmChunk>>>;
}

pub enum LlmResponse {
    Text(String),                          // 最终回复
    ToolCalls(Vec<ToolCallRequest>),       // 工具调用
}

pub struct ToolCallRequest {
    pub tool_name: String,
    pub arguments: serde_json::Value,
}
```

具体 LLM 的 function calling / JSON mode / tool_use 格式适配，由 bridge 层实现 trait。

---

## 6. Trait 抽象层（可插拔架构）

`core` crate 定义以下 trait，不依赖任何具体实现：

| Trait | 职责 | 可能的实现 |
|---|---|---|
| `LlmClient` | LLM 推理调用 | PyO3 (litellm) / reqwest (OpenAI API) / 本地模型 |
| `EmbeddingClient` | 文本向量化 | PyO3 (sentence-transformers) / 原生 Rust |
| `KnowledgeStore` | 知识库存储与检索 | 内存 HNSW / Qdrant / Milvus |
| `Tool` | 工具定义与执行 | 各具体工具实现 |

`core` crate **零外部依赖**（除 async runtime），不 import infra/bridge。

---

## 7. 仓库结构（更新）

```
seat-agent/
├── Cargo.toml                  # workspace 根配置
├── AGENTS.md
├── crates/
│   ├── core/                   # Agent runtime 核心（纯库，trait 定义）
│   │   └── src/
│   │       ├── agent.rs        # Agent 主循环（Agent Loop）
│   │       ├── context.rs      # Context 分层模型 + token 预算
│   │       ├── config.rs       # AgentConfig（模态、轮次、模型选择）
│   │       ├── error.rs        # 错误类型
│   │       └── traits.rs       # 核心 trait 定义
│   ├── tools/                  # 工具注册与执行
│   │   └── src/
│   │       ├── registry.rs     # 工具注册表（ToolInfo + intermediate_reply）
│   │       ├── knowledge.rs    # 知识库检索工具
│   │       ├── business.rs     # 业务系统查询工具
│   │       └── transfer.rs     # 转人工工具
│   ├── memory/                 # 记忆系统
│   │   └── src/
│   │       ├── short_term.rs   # 短期记忆（当前会话 history，滑动窗口）
│   │       ├── long_term.rs    # 长期记忆（会话摘要存储）
│   │       └── summary.rs      # 会话摘要生成/修正
│   ├── bridge/                 # PyO3 桥接（具体 trait 实现）
│   │   └── src/
│   │       ├── embedding.rs    # EmbeddingClient trait 实现
│   │       ├── llm.rs          # LlmClient trait 实现（litellm/本地）
│   │       └── store.rs        # KnowledgeStore trait 实现（Qdrant等）
│   └── server/                 # 独立服务（可选）
│       └── src/
│           ├── grpc.rs         # gRPC 服务实现
│           └── main.rs         # 入口
├── docs/
│   └── 2026-07-10-seat-agent-design.md  # 本文件
└── examples/
    └── basic_chat/             # SDK 模式示例
```

---

## 8. 硬性约束（汇总）

1. **Rust 依赖声明在根 Cargo.toml**，子 crate 用 `workspace = true`
2. **Domain 层零外部依赖**：core crate 不依赖 infra/bridge/server
3. **全链路流式**：Agent 响应天然是 token 流，不是完整字符串
4. **工具调用轮次硬上限**：由 AgentConfig.max_tool_rounds 控制，不可绕过
5. **不幻觉**：检索不到知识时，必须转人工或明确告知，不编造
6. **Context 截断安全**：永远只截断 history 层，system/retrieval/summary 不可截断
7. **意图分类零延迟**：由 Rust 规则实现，不走 LLM

---

## 9. 延迟预算

| 阶段 | 目标延迟 | 实现方式 |
|---|---|---|
| 预检索（并行） | <200ms | Rust 规则意图分类 + 知识库向量检索并行 |
| LLM 首 token | <500ms | 流式 SSE，首次 chunk 到达即开始输出 |
| 工具执行 | <500ms | 工具自身实现需满足，超时则跳过 |
| 中间回复 | 0ms | 模板注入，不调 LLM |
| 会话摘要生成 | 不计入响应延迟 | 会话结束后异步执行 |

---

## 10. 待讨论项

- [ ] gRPC 协议设计（service definition，request/response schema）
- [ ] 配置文件格式（YAML/TOML 的具体 schema）
- [ ] 并发模型（tokio runtime 配置，per-session 隔离）
- [ ] 监控与可观测性（tracing、metrics）
- [ ] 测试策略（单元测试、集成测试、mock LLM）
