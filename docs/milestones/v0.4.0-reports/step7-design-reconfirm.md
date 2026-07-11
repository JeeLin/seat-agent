# Step 7：设计再确认（v0.4.0 知识库集成）

## 确认范围

按审查框架（开发后）逐项核对：已实现的代码 vs 里程碑文档 `v0.4.0-知识库集成.md` 的详细设计。

## 核对项

| 里程碑设计点 | 实现位置 | 结论 |
|------|----------|------|
| 子任务1：内存版 `InMemoryVectorStore`，余弦相似度，`RwLock<HashMap>` | `crates/core/src/vector_store.rs` | ✅ 一致 |
| 子任务2：`OpenAiEmbeddingClient`（POST `/embeddings`）+ `MockEmbeddingClient`（确定性伪向量） | `crates/tools/src/embedding.rs` | ✅ 一致 |
| 子任务3：`KnowledgeSearchTool` 实现 `Tool`，embed→search→格式化，空结果不编造 | `crates/tools/src/knowledge.rs` | ✅ 一致 |
| 子任务4：`QdrantVectorStore` 在 `qdrant` feature 下，`qdrant-client` 为 optional 依赖 | `crates/tools/src/qdrant_store.rs` + `Cargo.toml` | ✅ 一致 |
| 子任务5：server 启动时在 `ToolRegistry` 注册 `KnowledgeSearchTool` | `crates/server/src/main.rs` | ✅ 一致 |
| 放置说明：内存实现放 core（零依赖），外部依赖实现放 tools | core / tools 划分 | ✅ 一致 |
| 签名说明：`VectorStore::search` 按实际 trait（无 `filter` 参数）实现 | `vector_store.rs` / `qdrant_store.rs` | ✅ 一致 |

## 偏离与注意点（非阻断）

1. **运行时 `ToolDefinition` 无 `intermediate_reply` / `error_reply` 字段**
   - 里程碑文档子任务3 提到 `definition()` 复用 `example_tool_config()` 的 `knowledge_search` 结构
     （含 `intermediate_reply` / `error_reply`）。但 `core` 中实际的 `ToolDefinition` 结构体仅含
     `name` / `description` / `parameters` 三个字段（见 `traits.rs:183`）。
   - 实现按**实际 trait** 构建 `ToolDefinition`（覆盖 name/description/parameters），与运行时契约一致。
   - "不编造" 的 `error_reply` 语义由 `KnowledgeSearchTool::execute` 在空结果时返回明确文案承担，
     语义等价。属文档描述与运行时结构的预期差异，不影响功能。

2. **server 接入使用 `OpenAiEmbeddingClient` 而非 Mock**
   - 子任务5 详细设计中示例代码用了 `MockEmbeddingClient::new(384)`，但 server 是真实服务入口，
     使用 `OpenAiEmbeddingClient`（由配置驱动）更符合独立服务定位。`InMemoryVectorStore` 作为默认
     向量存储与示例一致；生产可用 `Qdrant` 经 feature 切换。属合理适配。

## 结论

代码实现与里程碑文档的核心设计点一致，两条偏离均为文档描述 vs 运行时契约的预期差异，不影响
"准确性优先" 与 RAG 信息基础功能。确认通过。✅
