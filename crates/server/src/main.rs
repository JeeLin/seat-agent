mod config;
mod grpc;
mod redis_store;

use anyhow::Result;
use config::AppConfig;
use redis_store::RedisSessionStore;

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
    let embedding_client =
        std::sync::Arc::new(seat_agent_tools::embedding::OpenAiEmbeddingClient::new(
            config.embedding.api_key.clone(),
            Some(config.embedding.base_url.clone()),
            Some(config.embedding.model.clone()),
        ));
    tracing::info!(
        "Embedding 客户端就绪: model={} dim={}",
        config.embedding.model,
        config.embedding.dim
    );

    // 构建向量存储：默认内存实现，可切换为 Qdrant（需 tools 启用 qdrant feature）
    let vector_store: std::sync::Arc<dyn seat_agent_core::VectorStore> =
        if config.knowledge.vector_store == "qdrant" {
            #[cfg(feature = "qdrant")]
            {
                std::sync::Arc::new(seat_agent_tools::qdrant_store::QdrantVectorStore::new(
                    &config.knowledge.qdrant_url,
                    config.knowledge.qdrant_collection.clone(),
                )?)
            }
            #[cfg(not(feature = "qdrant"))]
            {
                tracing::warn!(
                    "vector_store=qdrant 需要以 --features qdrant 构建，回退到内存向量存储"
                );
                std::sync::Arc::new(seat_agent_core::vector_store::InMemoryVectorStore::new())
            }
        } else {
            std::sync::Arc::new(seat_agent_core::vector_store::InMemoryVectorStore::new())
        };

    // 构建并注册知识库检索工具（RAG 信息基础）
    let knowledge_tool = seat_agent_tools::knowledge::KnowledgeSearchTool::new(
        vector_store.clone(),
        embedding_client.clone(),
        config.knowledge.top_k,
    );
    let mut registry = seat_agent_tools::registry::ToolRegistry::new();
    registry.register(Box::new(knowledge_tool));
    tracing::info!(
        "已注册工具: {}",
        registry
            .tool_definitions()
            .iter()
            .map(|d| d.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );

    // TODO: 创建 LlmClient
    // let llm_client = OpenAiClient::new(&config.llm.base_url, &config.llm.api_key, &config.llm.model);
    // let agent = Agent::new(config.agent.clone(), Box::new(llm_client));

    // 启动 gRPC 服务器
    let addr = config.server.addr.clone();
    tracing::info!("gRPC 服务器启动在 {}", addr);

    // TODO: 实现完整的 gRPC 服务器启动
    // let grpc_server = grpc::AgentGrpcServer::new(agent);
    // tonic::transport::Server::builder()
    //     .add_service(grpc_server.into_service())
    //     .serve(addr.parse()?)
    //     .await?;

    tracing::info!("服务器启动完成（简化模式）");
    Ok(())
}
