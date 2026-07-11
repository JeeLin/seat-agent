# 步骤2 设计核对：v0.5.0 工具完善

> 审查对象：里程碑文档 `docs/milestones/v0.5.0-工具完善.md`
> 对照基准：`AGENTS.md` + `docs/DEVELOPMENT.md`
> 注：`docs/PRODUCT.md` 缺失（已与用户确认以 AGENTS.md + DEVELOPMENT.md 为基准），本步骤产品边界交叉校验降级，仅做设计合理性与约束一致性审查。

## 审查项

### 1. 与开发计划（DEVELOPMENT.md）一致性
- v0.5.0 规划核心功能：「业务工具（订单/退款/投诉）+ 转人工规则」。
- 里程碑覆盖：订单/退款/投诉查询工具（子任务1-3）+ 转人工工具（子任务4）。✅ 对齐。
- 子任务5 将全部工具接入 server 启动链路，闭环「工具完善」。✅

### 2. 与 AGENTS.md 硬性约束一致性
| 约束 | 检查结果 |
|---|---|
| Rust 依赖声明在根 Cargo.toml，子 crate 用 `workspace = true` | 新增 trait 在 core、实现在 tools、依赖经 workspace；符合。✅ |
| Domain 层零外部依赖（core 不依赖 server/infra） | `BusinessBackend` trait 放 core，仅用 `serde_json::Value`/`async_trait`（core 已有）；Mock/真实实现放 tools。✅ |
| 全链路流式（工具返回 token 流，非字符串） | 工具 `execute` 仍返回 `Result<String>`（结果注入上下文，未破坏流式边界），与 v0.4.0 `KnowledgeSearchTool` 一致。✅ |
| 不幻觉：检索/查询无结果不编造 | 三个业务工具与 transfer 均明确「缺失参数→error；无结果→明确提示」，transfer 仅格式化出口。✅ |
| Context 截断安全 | 本里程碑不涉及 context 分层。N/A |
| 意图分类零延迟（Rust 规则） | 里程碑明确「不实现意图分类，工具由 LLM 选择」。✅ |
| 工具调用轮次硬上限 | 不涉及 Agent Loop。N/A |
| 单会话单节点 | 不涉及跨节点。N/A |

### 3. 子任务拆分粒度
- 5 个子任务，每个对应 1 个 commit（提交信息已在详细设计中给出），粒度合理、可独立验证。✅

### 4. 接口设计与现状一致性
- 三个业务工具 + transfer 均实现 `Tool` trait（`definition() + async execute(Value) -> Result<String>`），与 `KnowledgeSearchTool` 同构。✅
- `definition()` 复用 `example_tool_config()` 配置（order_query/refund_query/complaint_query 需在子任务内补齐；transfer_to_human 配置已存在）。✅
- `BusinessBackend` 返回 `serde_json::Value`，格式化下沉工具层，避免 core 膨胀业务结构。✅

### 5. 产品边界合理性（降级项）
- 「不做什么」清晰：不实现真实业务后端、不实现 Agent Loop 编排、不实现意图分类、不做敏感写操作。边界与现状（server 仍为简化模式）吻合。✅
- 提示：DEVELOPMENT.md 表述「转人工规则」，本里程碑实现的是**转人工工具出口**（格式化 + 出口标记）；「严格条件触发」属 Agent Loop 决策层，超出本里程碑范围，已写入「不做什么」。这是合理拆分，非缺陷。

## 结论

✅ 设计合理，无需修改。子任务拆分、约束一致性、接口对齐均满足开发条件。可进入步骤3 开发。
