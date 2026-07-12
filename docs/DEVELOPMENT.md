# seat-agent 开发计划

## 里程碑规划

| 版本 | 标题 | 核心功能 | 状态 |
| v0.1.0 | 基础架构 | core crate（trait + Config + Context + Agent Loop）+ tools crate（ToolRegistry + JSON 配置加载） | ✅ 已完成 |
| v0.2.0 | LLM 集成 | OpenAI LLM 实现 + mock LLM + 基础示例 | ✅ 已完成 |
| v0.3.0 | 独立服务 | server crate（gRPC + 配置加载 + Redis） | ✅ 已完成 |
| v0.4.0 | 知识库集成 | VectorStore trait + Qdrant/内存实现 + 检索工具 | ✅ 已完成 |
| v0.5.0 | 工具完善 | 业务工具（订单/退款/投诉）+ 转人工规则 | ✅ 已完成 |
| v0.6.0 | 测试与示例 | 集成测试 + basic_chat/voice_chat 示例 | ✅ 已完成 |
| v1.0.0 | 生产就绪 | Memory 系统 + Server 接入 + API 文档 + 集成测试 | ✅ 已完成 |

## 当前状态

- ✅ v0.1.0 基础架构已完成
- ✅ v0.2.0 LLM 集成已完成
- ✅ v0.3.0 独立服务已完成
- ✅ v0.4.0 知识库集成已完成
- ✅ v0.5.0 工具完善已完成
- ✅ v0.6.0 测试与示例已完成
下一个里程碑：v1.0.0 生产就绪 ✅ 已完成
## 技术决策记录

| 决策 | 选择 | 理由 |
|---|---|---|
| 向量数据库 | Feature 控制，支持 Qdrant/Milvus/pgvector/ChromaDB | 灵活性，按需启用 |
| 工具配置 | JSON Schema 结构化 | 方便页面配置 |
| 转人工规则 | 严格条件 + 专业话术 | 平衡自动化率和用户体验 |
| 错误恢复 | v1 不重试 | 简单，上层决策 |
| 并发模型 | 每会话一个 tokio task | 隔离性好 |
