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
