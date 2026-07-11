# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/),
and this project adheres to [Semantic Versioning](https://semver.org/).

## [Unreleased]

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
