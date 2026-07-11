use async_trait::async_trait;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;
use seat_agent_core::error::{AgentError, Result};
use seat_agent_core::traits::{Session, SessionStore};

/// Redis 会话存储
pub struct RedisSessionStore {
    manager: ConnectionManager,
    prefix: String,
    ttl: u64,
}

impl RedisSessionStore {
    /// 创建新的 Redis 会话存储
    pub async fn new(redis_url: &str) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| AgentError::Internal(format!("Redis 连接失败: {}", e)))?;
        let manager = ConnectionManager::new(client)
            .await
            .map_err(|e| AgentError::Internal(format!("Redis 连接管理器创建失败: {}", e)))?;

        Ok(Self {
            manager,
            prefix: "seat-agent:session:".to_string(),
            ttl: 3600, // 默认 1 小时过期
        })
    }

    /// 设置键前缀
    pub fn with_prefix(mut self, prefix: &str) -> Self {
        self.prefix = prefix.to_string();
        self
    }

    /// 设置 TTL（秒）
    pub fn with_ttl(mut self, ttl: u64) -> Self {
        self.ttl = ttl;
        self
    }

    fn key(&self, session_id: &str) -> String {
        format!("{}{}", self.prefix, session_id)
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn get(&self, session_id: &str) -> Result<Option<Session>> {
        let mut conn = self.manager.clone();
        let key = self.key(session_id);

        // 获取会话数据
        let data: Option<String> = conn
            .get(&key)
            .await
            .map_err(|e| AgentError::Internal(format!("Redis GET 失败: {}", e)))?;

        match data {
            Some(json) => {
                let session: Session = serde_json::from_str(&json)
                    .map_err(|e| AgentError::Internal(format!("会话反序列化失败: {}", e)))?;

                // 刷新 TTL，防止活跃会话过期
                let _: () = conn
                    .expire(&key, self.ttl as usize)
                    .await
                    .map_err(|e| AgentError::Internal(format!("Redis EXPIRE 失败: {}", e)))?;

                Ok(Some(session))
            }
            None => Ok(None),
        }
    }

    async fn set(&self, session_id: &str, session: &Session) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = self.key(session_id);
        let json = serde_json::to_string(session)
            .map_err(|e| AgentError::Internal(format!("会话序列化失败: {}", e)))?;

        // 使用 SETEX 设置值并设置过期时间
        let _: () = conn
            .set_ex(&key, json, self.ttl as usize)
            .await
            .map_err(|e| AgentError::Internal(format!("Redis SETEX 失败: {}", e)))?;

        Ok(())
    }

    async fn delete(&self, session_id: &str) -> Result<()> {
        let mut conn = self.manager.clone();
        let key = self.key(session_id);
        let _: () = conn
            .del(&key)
            .await
            .map_err(|e| AgentError::Internal(format!("Redis DEL 失败: {}", e)))?;

        Ok(())
    }
}
