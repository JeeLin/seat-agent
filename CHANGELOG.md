# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [0.6.0] - 2026-07-11

### Added
- Core crate 单元测试：Agent、Context、Config、Error、Mock 共 41 个测试
- Tools crate 测试：ToolRegistry 注册/查找/分组/激活共 11 个测试
- 跨 crate 集成测试：Agent + Tools 端到端流程共 9 个测试
- basic_chat 示例增强：集成 OrderQueryTool、RefundQueryTool、ComplaintQueryTool、TransferToHumanTool
- voice_chat 示例：语音模式演示（max_rounds=2，转人工场景）
- Core lib.rs 导出 FinishReason 类型

### Changed
- Version bumped from 0.5.0 to 0.6.0

## [0.5.0] - 2026-07-11

### Added
- 业务工具：OrderQueryTool、RefundQueryTool、ComplaintQueryTool
- 转人工工具：TransferToHumanTool
- BusinessBackend trait 及 MockBusinessBackend
- server crate 集成业务工具和转人工工具

## [0.4.0] - 2026-07-11

### Added
- VectorStore trait 及 InMemoryVectorStore 实现
- Qdrant VectorStore（feature flag `qdrant`）
- KnowledgeSearchTool 知识库检索工具

## [0.3.0] - 2026-07-11

### Added
- 独立 gRPC 服务（Bidi Streaming）
- Redis 会话存储
- 配置加载（YAML）
- TTS client 接口

## [0.2.0] - 2026-07-11

### Added
- OpenAI LLM 客户端（流式输出）
- MockLlmClient 测试辅助
- basic_chat 基础示例

## [0.1.0] - 2026-07-11

### Added
- Core crate：Agent Loop、Context 分层模型、AgentConfig
- Tools crate：ToolRegistry、JSON 配置加载
- 核心 trait 定义（LlmClient、Tool、KnowledgeStore、MemoryStore、VectorStore）
- 错误类型体系
