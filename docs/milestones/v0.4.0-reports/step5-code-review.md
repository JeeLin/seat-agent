# Step 5：代码审查（v0.4.0 知识库集成）

## 审查范围

本步骤审查 v0.4.0 步骤3/4 引入的全部代码变更：

- `crates/tools/src/qdrant_store.rs`（新增 QdrantVectorStore，feature = qdrant）
- `crates/tools/Cargo.toml`、`crates/tools/src/lib.rs`（feature / 模块）
- `crates/tools/src/embedding.rs`（OpenAiEmbeddingClient，本里程碑步骤2引入，纳入审查）
- `crates/tools/src/knowledge.rs`（KnowledgeSearchTool，本里程碑步骤3引入，纳入审查）
- `crates/server/src/config.rs`、`crates/server/src/main.rs`（接入）
- `crates/core/src/lib.rs`（导出 MockLlmClient）

## 发现与严重程度

### 🔴 必须修复

无。

### 🟡 应该修复

1. **`OpenAiEmbeddingClient::new` 在 HTTP 客户端构建失败时 `panic`**
   - 位置：`crates/tools/src/embedding.rs:42`
   - 问题：`.build().expect("Failed to create HTTP client")` 在极端环境（如 TLS 后端缺失）下会直接 panic，
     不符合 Agent 错误应走 `Result` 的约定。
   - 建议：改为返回 `Result<Self>`，将错误映射为 `AgentError::Embedding`；或至少移除 `expect` 改为
     `.unwrap_or_else` + 日志。当前因 workspace reqwest 已启用 `rustls-tls`，实际不会触发，
     但属健壮性隐患。
   - 决策：本步骤不阻断（默认构建由 rustls-tls 保证可用），记录为后续改进项。

2. **`QdrantVectorStore::new` 构造时执行兼容性健康检查**
   - 位置：`crates/tools/src/qdrant_store.rs:25`（`Qdrant::from_url(url).build()`）
   - 问题：默认 `QdrantBuilder` 的 `check_compatibility = true`，`build()` 内部会同步发起一次对
     Qdrant 服务的健康检查并阻塞。若服务暂不可达，`new` 会失败。
   - 建议：对于异步运行时下的连接构造函数可接受；如需弱连接语义，可在 `QdrantBuilder` 上
     `.set_check_compatibility(false)`。当前行为（连接即校验）对独立服务场景合理，记录为注意项。

### 🟢 可选改进

1. `qdrant_store::metadata_to_payload` 对 `HashMap` 做了一次 `clone`，可改为接收 `HashMap` 所有权以避免复制
   （当前 `VectorStore::upsert` trait 签名已接收 `HashMap<String, Value>`，调用方本就拥有所有权）。

2. `qdrant_value_to_json` 对 `Value::kind` 逐变体转换，逻辑完整且不可省略，保持现状。

## 结论

审查未发现 🔴 必须修复项。🟡 两项均为健壮性/语义注意项，不影响默认构建与测试，不阻断流程。
门禁通过。✅
