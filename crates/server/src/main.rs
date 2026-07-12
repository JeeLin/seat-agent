mod config;
mod grpc;
mod memory_store;
mod llm;
mod redis_store;

use anyhow::Result;
use config::AppConfig;
use memory_store::MemoryManagerImpl;
use redis_store::RedisSessionStore;
use seat_agent_core::vector_store::InMemoryVectorStore;
use seat_agent_core::{Agent, BusinessBackend, LlmClient, MemoryManager, VectorStore};
use seat_agent_tools::business::{
    ComplaintQueryTool, MockBusinessBackend, OrderQueryTool, RefundQueryTool,
};
use seat_agent_tools::embedding::OpenAiEmbeddingClient;
use seat_agent_tools::knowledge::KnowledgeSearchTool;
use llm::OpenAiClient;
use seat_agent_tools::registry::ToolRegistry;
use seat_agent_tools::transfer::TransferToHumanTool;
use std::sync::Arc;

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    // 加载配置
    let config = AppConfig::from_file("config/agent.yaml")?.with_env_overrides();
    tracing::info!("配置加载完成");

    // 创建 Redis 会话存储
    let _session_store = RedisSessionStore::new(&config.redis.url)
        .await?
        .with_ttl(config.redis.session_ttl);
    tracing::info!("Redis 连接成功");

    // 构建 Embedding 客户端（OpenAI 兼容接口）
    let embedding_client = Arc::new(OpenAiEmbeddingClient::new(
        config.embedding.api_key.clone(),
        Some(config.embedding.base_url.clone()),
        Some(config.embedding.model.clone()),
    ));
    tracing::info!(
        "Embedding 客户端就绪: model={} dim={}",
        config.embedding.model,
        config.embedding.dim
    );

    // 构建向量存储：默认内存实现，可切换为 Qdrant（需以 --features qdrant 构建）
    let vector_store: Arc<dyn VectorStore> = if config.knowledge.vector_store == "qdrant" {
        #[cfg(feature = "qdrant")]
        {
            Arc::new(seat_agent_tools::qdrant_store::QdrantVectorStore::new(
                &config.knowledge.qdrant_url,
                config.knowledge.qdrant_collection.clone(),
            )?)
        }
        #[cfg(not(feature = "qdrant"))]
        {
            tracing::warn!("vector_store=qdrant 需要以 --features qdrant 构建，回退到内存向量存储");
            Arc::new(InMemoryVectorStore::new())
        }
    } else {
        Arc::new(InMemoryVectorStore::new())
    };

    // 构建并注册知识库检索工具（RAG 信息基础）
    let knowledge_tool = KnowledgeSearchTool::new(
        vector_store.clone(),
        embedding_client.clone(),
        config.knowledge.top_k,
    );
    let mut registry = ToolRegistry::new();
    registry.register(Box::new(knowledge_tool));

    // 构建业务后端（默认 Mock，真实 HTTP 后端为后续里程碑）
    let backend: Arc<dyn BusinessBackend> = Arc::new(MockBusinessBackend::new());

    // 注册业务查询工具（订单/退款/投诉）+ 转人工工具
    registry.register(Box::new(OrderQueryTool::new(backend.clone())));
    registry.register(Box::new(RefundQueryTool::new(backend.clone())));
    registry.register(Box::new(ComplaintQueryTool::new(backend.clone())));
    registry.register(Box::new(TransferToHumanTool::new()));
    tracing::info!(
        "已注册工具: {}",
        registry
            .tool_definitions()
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // 构建 LLM 客户端（OpenAI 兼容接口）
    let llm_client: Arc<dyn LlmClient> = Arc::new(OpenAiClient::new(
        config.llm.api_key.clone(),
        Some(config.llm.base_url.clone()),
        Some(config.llm.model.clone()),
    ));
    tracing::info!("LLM 客户端就绪: model={}", config.llm.model);

    // 构建 Memory 管理器（长期记忆向量检索 + 会话摘要生成）
    let memory_manager: Arc<dyn MemoryManager> = Arc::new(MemoryManagerImpl::new(
        &config.memory,
        vector_store.clone(),
        embedding_client.clone(),
        llm_client.clone(),
    ));
    tracing::info!(
        "Memory 管理器就绪: short_term_max={} long_term_top_k={}",
        config.memory.short_term_max,
        config.memory.long_term_top_k
    );

    // 构建 Agent
    let llm_bridge = LlmBridge(llm_client.clone());
    let mut agent = Agent::new(config.agent.clone(), Box::new(llm_bridge));

    // 注册工具到 Agent（直接构建，因为 ToolRegistry 不支持 drain）
    agent.register_tool(Box::new(KnowledgeSearchTool::new(
        vector_store.clone(),
        embedding_client.clone(),
        config.knowledge.top_k,
    )));
    agent.register_tool(Box::new(OrderQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(RefundQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(ComplaintQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(TransferToHumanTool::new()));

    // 设置知识库（供 Agent 预检索）
    agent.set_knowledge(Box::new(KnowledgeSearchTool::new(
        vector_store.clone(),
        embedding_client.clone(),
        config.knowledge.top_k,
    )));

    // 设置记忆管理器
    agent.set_memory(Box::new(MemoryManagerImpl::new(
        &config.memory,
        vector_store.clone(),
        embedding_client,
        llm_client,
    )));

    tracing::info!("Agent 构建完成");

    // 启动 gRPC 服务器
    let addr = config.server.addr.clone();
    tracing::info!("gRPC 服务器启动在 {}", addr);

    let grpc_server = grpc::AgentGrpcServer::new(agent);
    tonic::transport::Server::builder()
        .add_service(grpc_server.into_service())
        .serve(addr.parse()?)
        .await?;

    Ok(())
}

/// 桥接 Arc<dyn LlmClient> 到 Box<dyn LlmClient>
///
/// `Agent::new` 接受 `Box<dyn LlmClient>`，但后续多个组件共享同一 LLM 客户端实例。
/// 此结构体持有 Arc，在调用时自动 defer。
struct LlmBridge(Arc<dyn LlmClient>);

#[async_trait::async_trait]
impl LlmClient for LlmBridge {
    async fn chat_stream(
        &self,
        request: seat_agent_core::LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<
            Box<
                dyn futures::Stream<Item = seat_agent_core::Result<seat_agent_core::LlmStreamChunk>>
                    + Send,
            >,
        >,
    > {
        self.0.chat_stream(request).await
    }
}
