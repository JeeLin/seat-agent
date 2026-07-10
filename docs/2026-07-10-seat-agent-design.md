# seat-agent 架构设计

> 2026-07-10 · 基于团队讨论确定的架构决策

---

## 1. 项目定位

seat-agent 是面向客服接待场景的 **Agent Runtime**，支持文本和语音两种对话模态。

核心特性：
- **准确性优先** — RAG 必须，不幻觉，检索不到就转人工
- **速度优先** — 全链路流式，延迟有上限
- **可嵌入可独立** — 既是 Rust SDK（OCC 集成），也可作为独立 gRPC 服务运行

### 与 OCC 的关系

seat-agent 是为 [OCC（全渠道客服工作台）](https://github.com/JeeLin/OCC) 构建的 Agent Runtime 引擎。OCC 已有多租户（RLS）、Redis、gRPC（tonic）、WebSocket 等完整基础设施，seat-agent 只负责 Agent 核心逻辑。

**OCC 通过 git 依赖引用 seat-agent**：

```toml
# OCC/Cargo.toml
[workspace.dependencies]
seat-agent-core = { git = "ssh://git@ssh.github.com:443/JeeLin/seat-agent.git", path = "crates/core" }
seat-agent-tools = { git = "ssh://git@ssh.github.com:443/JeeLin/seat-agent.git", path = "crates/tools" }
seat-agent-memory = { git = "ssh://git@ssh.github.com:443/JeeLin/seat-agent.git", path = "crates/memory" }
```

本地开发时通过 `[patch]` 覆盖为本地路径，seat-agent 改动即时生效。

---

## 2. 使用模式

### 2.1 作为库（OCC 集成）

OCC 只依赖 lib crate，自己管理 gRPC / Redis / 配置：

```rust
use seat_agent_core::{Agent, AgentConfig};

let config = AgentConfig {
    modality: Modality::Text,
    max_tool_rounds: 4,
    // ...
};
let mut agent = Agent::new(config).await?;
agent.register_tool(Box::new(KnowledgeSearchTool));

let response = agent.chat("我想退货").await?;
```

### 2.2 作为独立服务

直接运行 `seat-agent-server` 二进制，自带 gRPC server + Redis + 配置加载：

```bash
seat-agent-server --config agent.yaml
```

---

## 3. 仓库结构

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
│   │       └── traits.rs       #   核心 trait 定义（LlmClient / Tool / ...）
│   ├── tools/                  # [lib] 工具注册 + 分组 + 动态激活
│   │   └── src/
│   │       ├── registry.rs     #   ToolRegistry（分组 + 激活逻辑）
│   │       ├── knowledge.rs    #   内置工具：知识库检索
│   │       ├── business.rs     #   内置工具：业务系统查询
│   │       └── transfer.rs     #   内置工具：转人工
│   ├── memory/                 # [lib] 短期记忆 + 长期记忆 + 摘要生成
│   │   └── src/
│   │       ├── short_term.rs   #   短期记忆（滑动窗口）
│   │       ├── long_term.rs    #   长期记忆（向量检索）
│   │       └── summary.rs      #   会话摘要生成/修正
│   └── server/                 # [bin] 独立服务
│       └── src/
│           ├── main.rs         #   入口
│           ├── grpc.rs         #   gRPC Bidi Streaming server
│           ├── config.rs       #   ConfigProvider 实现（YAML 文件）
│           ├── redis_store.rs  #   SessionStore 实现（Redis）
│           ├── embedding.rs    #   EmbeddingClient 实现
│           ├── llm.rs          #   LlmClient 实现（reqwest）
│           └── tts.rs          #   TtsClient 实现
├── docs/
│   └── 2026-07-10-seat-agent-design.md
└── examples/
    └── basic_chat/             # SDK 模式示例
```

**职责划分**：

| crate | 类型 | 谁用 |
|---|---|---|
| `core` | lib | OCC / 任何集成方 |
| `tools` | lib | OCC / 任何集成方 |
| `memory` | lib | OCC / 任何集成方 |
| `server` | bin | 独立部署 / 演示 / 测试 |

---

## 4. Trait 抽象层

`core` crate 定义以下 trait，不依赖任何具体实现：

| Trait | 职责 | 可能的实现 |
|---|---|---|
| `LlmClient` | LLM 推理调用 | reqwest (OpenAI API) / PyO3 (litellm) |
| `EmbeddingClient` | 文本向量化 | reqwest (外部 API) / PyO3 (sentence-transformers) |
| `KnowledgeStore` | 知识库存储与检索 | 内存 HNSW / Qdrant / Milvus |
| `TtsClient` | 文本转语音 | Azure TTS / ElevenLabs / 本地 TTS |
| `Tool` | 工具定义与执行 | 各具体工具实现 |

`core` crate **零外部依赖**（除 async runtime），不 import server。PyO3 是可选实现方式，不强制依赖。

---

## 5. Context 分层模型

上下文按职责分层，**截断历史时只截 history 层**：
- system、retrieval、history_summary → 只读保护区，不可截断
- history → 滑动窗口，是唯一可截断的层
- long_term → 存储后端，支撑 retrieval 和 summary 的数据来源

```
Context
├── system: Vec<Message>              // 系统指令 + 角色设定，始终保留
├── retrieval: Vec<SearchResult>      // 当前轮预检索结果，不可截断
├── history_summary: String           // 历史会话摘要，从 long_term 加载
├── history: VecDeque<Message>        // 当前会话对话，滑动窗口截断
└── long_term: Box<dyn KnowledgeStore> // 存储后端
```

### 5.1 Token 预算分配

```
总预算 = total_token_limit
  - system 固定开销
  - history_summary（固定长度，通常 <200 token）
  - retrieval 结果（当前轮检索结果）
  ──────────────────────────
  = history 可用预算（滑动窗口截断）
```

### 5.2 截断策略

1. 先计算 system + retrieval + history_summary 的 token 总量
2. 剩余预算分配给 history
3. history 从旧到新丢弃消息，但不能低于 `min_history_messages`（默认 2 条）
4. 如果最小窗口都放不下，触发异常告警

---

## 6. 对话历史摘要机制

### 6.1 摘要生成时机

**会话结束时生成/修正摘要**，不增加实时延迟：

```
会话 N 结束
  → 将 history_summary + 当前会话 history 一起传给 LLM
  → 生成新摘要（覆盖旧摘要）
  → 存入长期记忆（向量存储）
```

### 6.2 实时矛盾解决

会话过程中遇到历史摘要信息过时的情况，**通过工具调用实时验证**：

```
客户："退款还没到账"
  ↓ 长期记忆检索 → 历史摘要："客户上次咨询退款问题"
  ↓ 工具调用 → 查询订单系统 → 退款已于03-10到账
  ↓ 回复："您的退款已于3月10日到账"
  ↓ （会话结束时摘要自然更新为新事实）
```

### 6.3 摘要存储格式

```yaml
customer_id: "CUST_12345"
session_id: "sess_20260308_refund"
created_at: "2026-03-08T14:30:00Z"
summary: "客户张三，2026-03-08 咨询订单 #12345 退款问题，已提交退款申请，等待 3-5 个工作日。"
intent_tags: ["退款", "订单查询"]
```

---

## 7. Agent Loop 设计

### 7.1 整体流程

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
│                                         │
│  工具分组激活：                           │
│  intent_tags → 激活相关工具组            │
│  → 只传激活的工具给 LLM                  │
└─────────────────────────────────────────┘
  ↓
┌─────────────────────────────────────────┐
│  Agent Loop（最多 N 轮）                 │
│  ├── 第 1 轮 LLM 调用                    │
│  │   ├── 有 tool_call? → 执行工具         │
│  │   │   → 流式输出中间回复               │
+│  │   │   → 注入结果，进入第 2 轮           │
│  │   └── 无 tool_call? → 最终回复，结束   │
│  ├── 第 2 轮 ...                         │
│  └── 超过轮次上限 → 强制结束，返回当前回复 │
└─────────────────────────────────────────┘
  ↓
  流式输出最终回复（文本 / TTS 音频）
```

### 7.2 工具分组与动态激活

工具按业务域分组，只激活当前对话需要的组，避免工具过多导致 token 开销和 LLM 选择能力下降：

```rust
pub struct ToolRegistry {
    tools: HashMap<String, Box<dyn Tool>>,
    groups: HashMap<String, ToolGroup>,
}

pub struct ToolGroup {
    pub name: String,                     // 分组名（如 "order"）
    pub tools: Vec<String>,               // 该组包含的工具名
    pub trigger: Vec<String>,             // 意图关键词（空 = 始终激活）
    pub always_active: bool,              // true = 始终激活
}
```

分组配置示例：

```yaml
groups:
  knowledge:
    always_active: true
    tools:
      - name: knowledge_search
        description: "搜索知识库获取答案"
        intermediate_reply:
          text: "正在查阅知识库..."
          audio_cue: "sounds/keyboard_typing.mp3"

  order:
    trigger: ["订单", "退款", "退货", "物流"]
    tools:
      - name: order_query
        description: "查询订单状态"
        intermediate_reply:
          text: "稍等，我来查一下工单信息"
          audio_cue: "sounds/keyboard_typing.mp3"
      - name: refund_apply
        description: "申请退款"

  transfer:
    always_active: true
    tools:
      - name: transfer_to_human
        description: "转接人工客服"
        intermediate_reply:
          text: "正在为您转接人工客服..."
          audio_cue: "sounds/ringing.mp3"
```

激活逻辑：

```rust
impl ToolRegistry {
    pub fn activate_tools(&self, intent_tags: &[String]) -> Vec<&dyn Tool> {
        let mut active = Vec::new();
        for group in self.groups.values() {
            let should_activate = group.always_active
                || group.trigger.iter().any(|t| intent_tags.iter().any(|tag| tag.contains(t)));
            if should_activate {
                for name in &group.tools {
                    if let Some(tool) = self.tools.get(name) {
                        active.push(tool.as_ref());
                    }
                }
            }
        }
        active
    }
}
```

### 7.3 中间回复提示词

工具注册时声明中间提示词，按当前模态选择文本或音频，Agent 执行工具后自动输出，零 LLM 延迟：

```rust
pub struct IntermediateReply {
    pub text: Option<String>,          // "稍等，我来查一下工单信息"
    pub audio_cue: Option<String>,     // "sounds/keyboard_typing.mp3"
}
```

### 7.4 工具调用轮次上限

| 模态 | 默认上限 | 说明 |
|---|---|---|
| 文本 | 4 轮 | 每轮间有中间回复，客户感知为正常等待 |
| 语音 | 2 轮 | 语音场景更敏感，上限更低 |

超限时强制结束 Agent Loop，返回当前已有内容。

---

## 8. 输入输出设计

### 8.1 输入类型

Agent 核心只接收文本，多模态输入由上层预处理：

| 输入类型 | Agent 收到的格式 | 预处理 |
|---|---|---|
| 纯文本 | `Content::Text` | 无需处理 |
| 语音 | `Content::Text` | ASR 服务转文字（Agent 不感知） |
| 文本 + 图片 | `Content::Text + Content::Image` | 直接透传给 Vision 模型 |
| 文件/PDF | `Content::Text` | 上层系统提取文本后传入 |

```rust
pub enum Content {
    Text(String),
    Image { url: String, detail: String },
}
```

### 8.2 输出模式

```rust
pub enum OutputMode {
    Text,                           // 纯文本（文本模式）
    TextToSpeech { voice_id: Option<String> },  // 文本 + TTS（语音模式）
    Adaptive,                       // 根据输入模态自动选择
}
```

Agent Loop 核心只产出文本，TTS 在输出层按 `OutputMode` 决定是否调用。

---

## 9. gRPC 通信协议

独立服务模式下，采用 **Bidirectional Streaming**：

```protobuf
service SeatAgent {
  rpc Chat (stream ChatMessage) returns (stream ChatChunk);
}

message ChatMessage {
  string session_id = 1;
  string content = 2;
  Modality modality = 3;
}

message ChatChunk {
  oneof payload {
    string token = 1;           // 流式文本 token
    AudioCue audio = 2;         // 语音输出（TTS / 中间提示音）
    ToolEvent tool_event = 3;   // 工具调用事件
    ChatEnd end = 4;            // 对话结束信号
  }
}

message AudioCue {
  string cue_id = 1;
  bytes audio_data = 2;
}

enum Modality { TEXT = 0; VOICE = 1; }
```

---

## 10. 会话管理

### 10.1 连接即会话

独立服务模式下，每个 gRPC 双向流连接 = 一个会话。同一客户同一时间只允许一个连接，新消息到达时取消旧 Agent Loop。

### 10.2 会话持久化

独立服务模式下，Redis 存储会话信息（context 快照），每轮 Agent Loop 结束后异步写入。OCC 集成模式下，由 OCC 自己的 Redis 基础设施负责。

### 10.3 断线恢复

```
连接断开 → 节点保留会话状态 30s
  → 客户重连，携带 session_id
  → 从 Redis 加载 session → 重新进入 Agent Loop
  → 30s 内未重连 → 会话过期，摘要异步生成
```

### 10.4 打断处理

同一连接内，客户发新消息即为打断：

```
Agent Loop 正在处理 M1
  → 客户发送 M2
  → 取消 M1 的 LLM 调用和工具执行（CancellationToken）
  → M1 已输出内容不写入 history
  → 以 M2 为最新消息，重新进入 Agent Loop
```

---

## 11. 硬性约束

1. **Rust 依赖声明在根 Cargo.toml**，子 crate 用 `workspace = true`
2. **Domain 层零外部依赖**：core crate 不依赖 server/infra
3. **全链路流式**：Agent 响应天然是 token 流，不是完整字符串
4. **工具调用轮次硬上限**：由 AgentConfig.max_tool_rounds 控制，不可绕过
5. **不幻觉**：检索不到知识时，必须转人工或明确告知，不编造
6. **Context 截断安全**：永远只截断 history 层，system/retrieval/summary 不可截断
7. **意图分类零延迟**：由 Rust 规则实现，不走 LLM
8. **单会话单节点**：一次会话的 Agent Loop 在单个节点内存中运行，不跨节点

---

## 12. 延迟预算

| 阶段 | 目标延迟 | 实现方式 |
|---|---|---|
| 预检索（并行） | <200ms | Rust 规则意图分类 + 知识库向量检索并行 |
| LLM 首 token | <500ms | 流式 SSE，首次 chunk 到达即开始输出 |
| 工具执行 | <500ms | 工具自身实现需满足，超时则跳过 |
| 中间回复 | 0ms | 模板注入，不调 LLM |
| 会话摘要生成 | 不计入响应延迟 | 会话结束后异步执行 |

---

## 13. 待讨论项

- [ ] 配置文件格式（YAML/TOML 的具体 schema）
- [ ] 监控与可观测性（tracing、metrics）
- [ ] 测试策略（单元测试、集成测试、mock LLM）
- [ ] 语音模式完整链路（ASR → Agent → TTS）
- [ ] OCC 集成方式（git 依赖的 patch 配置）
