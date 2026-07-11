# 步骤2：设计核对报告

> 版本：v0.6.0 测试与示例
> 审查日期：2026-07-11

## 审查维度

### 1. 产品对齐 ✅

- v0.6.0 聚焦测试与示例，是 v0.1-v0.5 功能积累后的自然验证阶段
- 架构设计文档中多处提及示例和测试（`examples/basic_chat/`、Mock 实现用于测试）
- 双模态（Text + Voice）设计在 v0.6.0 通过两个示例得到完整演示
- 不新增公共 API，不改变已有行为，符合"测试与示例"的定位

### 2. 子任务拆分 ✅

| 子任务 | 范围 | 粒度评估 |
|--------|------|----------|
| 1 Core 单元测试 | 5 个模块的测试用例 | 合理，每个模块一组测试 |
| 2 Tools 补充测试 | registry.rs 测试 | 合理，单一模块 |
| 3 集成测试 | 跨 crate 流程 | 合理，6 个端到端场景 |
| 4 basic_chat 增强 | 替换 GreetTool 为业务工具 | 合理，升级现有示例 |
| 5 voice_chat 示例 | 新建语音示例 | 合理，与 basic_chat 对称 |

粒度适中，每个子任务对应一个 commit。

### 3. 接口设计 ✅

- 测试用例设计覆盖正常路径、边界条件、错误场景
- Mock 实现（MockLlmClient、MockBusinessBackend、InMemoryVectorStore）已存在，可直接复用
- 示例代码复用现有 crate 公共 API，不引入新的内部接口

### 4. 产品边界 ✅

**做什么**（合理）：
- Core/Tools 单元测试 → 补齐测试覆盖短板
- 集成测试 → 验证跨 crate 协作
- basic_chat 增强 → 从 demo 升级为完整客服场景
- voice_chat → 补齐双模态示例

**不做什么**（合理）：
- server crate 测试 → 依赖 Redis/gRPC 基础设施，属于独立里程碑
- memory crate → 空壳 crate，无需测试
- 性能测试 → 非本阶段目标
- 不修改已有 API → 测试与示例不改变公共接口

### 5. AGENTS.md 约束合规 ✅

- Rust 依赖声明在根 Cargo.toml → 集成测试不引入新依赖
- Domain 层零外部依赖 → 测试代码在 core 内联或 tests/ 目录，不破坏依赖关系
- 工具调用轮次上限 → voice_chat 示例展示 max_rounds=2 限制
- 不幻觉 → 测试验证 Mock 实现的确定性输出

### 6. 依赖关系 ✅

- 子任务 1-2 无外部依赖，可并行
- 子任务 3 依赖 1-2（集成测试需要单元测试验证的基础组件）
- 子任务 4 依赖 tools crate 已有实现
- 子任务 5 与 4 并行，无依赖

## 发现的问题

### 🟢 小问题（不阻塞）

1. **basic_chat 新增 seat-agent-tools 依赖**：当前 basic_chat 仅依赖 core，增强后需新增 tools 依赖。这是合理的，因为示例的目的是演示完整功能。**处理**：在详细设计中已考虑 Cargo.toml 更新。

2. **集成测试目录**：文档未明确是 workspace 根目录的 `tests/` 还是 crate 内的 `tests/`。按 Rust 惯例，跨 crate 集成测试应放在 workspace 根目录。**处理**：确认放在 `/workspace/seat-agent/tests/` 目录。

## 结论

✅ 审查通过。里程碑文档的产品边界合理、子任务拆分粒度适当、接口设计清晰、与 AGENTS.md 约束无冲突。上述小问题不影响实施，可在开发阶段直接处理。
