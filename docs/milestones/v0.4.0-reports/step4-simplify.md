# Step 4：代码精简检查（v0.4.0 知识库集成）

## 检查范围

本步骤对步骤3引入的全部改动做精简审查，确保功能行为不变、仅优化组织方式。

改动文件：

- `crates/tools/src/qdrant_store.rs`（新增，feature = qdrant）
- `crates/tools/Cargo.toml`、`crates/tools/src/lib.rs`（feature 声明 / 模块导出）
- `crates/server/src/config.rs`（新增 Embedding / Knowledge 配置段）
- `crates/server/src/main.rs`（构建并注册 KnowledgeSearchTool）
- `crates/core/src/lib.rs`（重新导出 `MockLlmClient`，修复 basic_chat 示例编译）

## 精简动作

1. **`main.rs` 路径简化**：为 OpenAI Embedding 客户端、KnowledgeSearchTool、ToolRegistry、
   InMemoryVectorStore、VectorStore 等引入 `use` 导入，消除函数体内重复的
   `std::sync::Arc::new(seat_agent_tools::...)` 全限定路径，可读性提升，行为不变。

2. **`qdrant_store.rs` 转换函数收敛**：`metadata_to_payload` 原使用 `Payload::try_from`（fallible），
   经 clippy 提示改为 `Payload::from`（infallible），去掉无意义的 `?` 与错误分支；
   `qdrant_value_to_json` 显式补全覆盖 `value::Kind::NullValue` 变体，避免非穷尽匹配。

3. **无关重复清理**：步骤3中误写入根 `Cargo.toml` 的两段重复 `[features]` 已移除；
   `server/Cargo.toml` 仅保留一处 `qdrant = ["seat-agent-tools/qdrant"]`。

未做改动（保留理由）：

- `qdrant_value_to_json` 的逐字段转换逻辑必须完整保留，否则 payload 元数据在 round-trip 时丢失，
  影响 RAG 检索结果中的 `content` 提取。
- `main.rs` 中 `cfg(feature = "qdrant")` 与 `else` 两条分支语义不同（前者 warn 回退、后者静默默认），
  未强行合并，以保持行为精确。

## 结论

- 编译检查：`cargo clippy --workspace --all-targets` 无 warning / error
- qdrant feature：`cargo clippy -p seat-agent-server --features qdrant` 无 warning / error
- 格式化：`cargo fmt --check` 通过
- 测试：`cargo test --workspace` 全部通过

精简未改变任何功能行为，门禁通过。✅
