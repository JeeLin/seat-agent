# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.5.0] - 2026-07-11

### Added
- 业务后端抽象：`BusinessBackend` trait 定义在 core crate（零外部依赖），提供 `MockBusinessBackend` 用于测试与示例
- 订单查询工具 `OrderQueryTool`：根据订单号查询并格式化订单信息（状态、金额、下单时间）
- 退款查询工具 `RefundQueryTool`：根据退款单号查询并格式化退款信息（状态、金额、原因、进度）
- 投诉查询工具 `ComplaintQueryTool`：根据投诉单号查询并格式化投诉处理进度（状态、渠道、进度、责任人）
- 转人工工具 `TransferToHumanTool`：格式化转人工原因与回复话术，标记转人工出口（`<<TRANSFER>>`）
- server 启动流程接入全部业务工具与转人工工具，与知识库工具共存

### Changed
- 业务查询工具提取 `get_field` / `get_field_or` / `require_arg` 辅助函数，消除重复代码（business.rs -13%）

## [0.4.0] - 2026-07-10

### Added
- 知识库集成：实现内存版 `VectorStore`（`InMemoryVectorStore`，余弦相似度检索），放置于 core crate 以遵循零外部依赖约束
-  Embedding 客户端：`OpenAiEmbeddingClient`（OpenAI 兼容 `/embeddings` 接口，rustls-tls）与 `MockEmbeddingClient`（确定性伪向量，测试用）
- 知识库检索工具 `KnowledgeSearchTool`：串联 embed → 向量检索 → 结果格式化，空结果明确提示不编造（RAG「准确性优先」信息基础）
- 可选 Qdrant 向量库实现 `QdrantVectorStore`，通过 `qdrant` Cargo feature 启用，默认构建不引入 `qdrant-client` 依赖
- server 启动流程接入知识库工具：新增 Embedding / Knowledge 配置段，按配置选择向量存储后端（内存默认，Qdrant 经 feature 切换）并在 `ToolRegistry` 注册 `KnowledgeSearchTool`

### Changed
- core 重新导出 `MockLlmClient`，修复 `examples/basic_chat` 编译

[0.4.0]: https://github.com/JeeLin/seat-agent/releases/tag/v0.4.0
