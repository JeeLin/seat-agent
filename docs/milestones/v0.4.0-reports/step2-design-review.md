# v0.4.0 步骤2：设计核对报告

## 审查对象
- 里程碑文档：`docs/milestones/v0.4.0-知识库集成.md`
- 对照文档：`docs/2026-07-10-seat-agent-design.md`（设计文档 §17.5-17.8、§16 工具配置、§12 Feature 控制）
- 现状代码：`crates/core/src/traits.rs`、`crates/tools/src/registry.rs`

## 审查维度（按 AGENTS.md 约定 + 设计文档审查框架）

### 1. 产品边界一致性 ✅
- 里程碑「做什么」覆盖了 DEVELOPMENT.md v0.4.0 的全部核心功能：VectorStore 实现、检索工具、Feature 控制。
- 「不做什么」与设计文档一致：不采集/预处理知识库内容（OCC 职责）、不部署向量服务端、不实现意图分类。

### 2. 准确性优先约束 ✅
- 知识库检索是 RAG 的信息基础，符合「准确性优先」硬约束（设计文档 §1、§5-4）。
- `KnowledgeSearchTool` 明确「无结果返回明确提示，不编造」，与设计文档 §5-4「检索不到知识时，必须转人工或明确告知，不编造」一致。

### 3. 速度优先约束 ✅
- 内存版 VectorStore 为 O(n) 余弦扫描，知识库规模可控时满足预检索 <200ms（设计文档 §13 延迟预算）。
- Mock/内存实现默认启用，无外部网络依赖，零额外延迟。

### 4. 架构原则 / 硬性约束 ✅
- **core crate 零外部依赖**：内存实现放在 `crates/core/src/vector_store.rs`（零依赖），外部依赖实现（Qdrant/OpenAI）放在 `crates/tools`，符合 AGENTS.md 硬性约束 2「Domain 层零外部依赖」。
  - 设计文档 §17.6 将实现写在 `crates/server/src/vector_store.rs`，本里程碑做了偏离调整，已在文档「放置说明」中标注理由（见子任务1）。此为合理偏离，core 更适合持有零依赖实现。
- **Feature 控制**：Qdrant 实现用 `cfg(feature = "qdrant")`，与 DEVELOPMENT.md 技术决策「向量数据库 Feature 控制」及设计文档 §12/§17 一致；默认构建不引入 `qdrant-client`。

### 5. 接口/签名一致性 ⚠️→已修正
- 设计文档 §17.5 的 `VectorStore::search` 签名含 `filter: Option<Filter>`，但当前 `traits.rs` 实际签名为 `search(&self, embedding, limit)`（无 filter）。
- 本里程碑按**实际 trait**实现，并在文档「签名说明」中记录该偏离，Filter 能力留待后续里程碑。合理。

### 6. 工具定义一致性 ✅
- `KnowledgeSearchTool::definition()` 复用 `registry.rs` 中 `example_tool_config()` 的 `knowledge_search` 结构（name/description/parameters/intermediate_reply/error_reply），与设计文档 §16 工具配置样例完全一致。

### 7. 子任务拆分粒度 ✅
- 5 个子任务，每个对应 1 个独立 commit，粒度合适（1-2 个 commit/子任务）。
- 子任务清单编号与详细设计一一对应。

## 结论
✅ **通过**。里程碑文档设计与产品文档、设计文档、现状代码一致。

### 核对中修正的小问题（保持步骤1勾选，直接修正文档）
1. 内存实现命名由 `MemoryVectorStore` 统一为 `InMemoryVectorStore`（对齐设计文档 §17.5/§17.6 命名）。
2. 子任务1 补充「放置说明」「签名说明」，明确与设计文档的两处偏离及理由。
3. 设计核对点同步更新命名。

无需打回。
