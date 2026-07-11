use std::pin::Pin;
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;
use tokio::sync::mpsc;
use tokio::time::sleep;

use crate::error::{AgentError, Result};
use crate::traits::{FinishReason, LlmClient, LlmRequest, LlmStreamChunk};

/// Mock LLM 客户端，用于测试
pub struct MockLlmClient {
    /// 预设响应序列
    responses: Vec<String>,
    /// 模拟延迟
    delay: Option<Duration>,
    /// 是否模拟错误
    simulate_error: bool,
    /// 当前响应索引
    index: std::sync::atomic::AtomicUsize,
}

impl MockLlmClient {
    /// 创建新的 MockLlmClient
    pub fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            delay: None,
            simulate_error: false,
            index: std::sync::atomic::AtomicUsize::new(0),
        }
    }

    /// 设置模拟延迟
    pub fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = Some(delay);
        self
    }

    /// 设置模拟错误
    pub fn with_error(mut self) -> Self {
        self.simulate_error = true;
        self
    }

    /// 获取下一个响应
    fn next_response(&self) -> String {
        let idx = self.index.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let response_idx = idx % self.responses.len();
        self.responses[response_idx].clone()
    }
}

#[async_trait]
impl LlmClient for MockLlmClient {
    async fn chat_stream(
        &self,
        _request: LlmRequest,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<LlmStreamChunk>> + Send>>> {
        if self.simulate_error {
            return Err(AgentError::Llm("Mock error".into()));
        }

        let (tx, rx) = mpsc::channel(32);
        let response = self.next_response();
        let delay = self.delay;

        tokio::spawn(async move {
            if let Some(d) = delay {
                sleep(d).await;
            }

            // 发送内容
            let _ = tx.send(Ok(LlmStreamChunk::Content(response))).await;

            // 发送完成信号
            let _ = tx
                .send(Ok(LlmStreamChunk::Done {
                    finish_reason: FinishReason::Stop,
                }))
                .await;
        });

        let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
        Ok(Box::pin(stream))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::LlmRequest;

    #[tokio::test]
    async fn test_mock_llm_returns_preset_responses() {
        let mock = MockLlmClient::new(vec!["a".into(), "b".into()]);
        let request = LlmRequest {
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            stream: true,
        };

        let stream1 = mock.chat_stream(request.clone()).await.unwrap();
        let chunks1: Vec<_> = tokio_stream::StreamExt::collect(stream1).await;
        assert!(!chunks1.is_empty());
        if let Ok(LlmStreamChunk::Content(text)) = &chunks1[0] {
            assert_eq!(text, "a");
        } else {
            panic!("Expected Content chunk");
        }

        let stream2 = mock.chat_stream(request.clone()).await.unwrap();
        let chunks2: Vec<_> = tokio_stream::StreamExt::collect(stream2).await;
        if let Ok(LlmStreamChunk::Content(text)) = &chunks2[0] {
            assert_eq!(text, "b");
        } else {
            panic!("Expected Content chunk");
        }
    }

    #[tokio::test]
    async fn test_mock_llm_cycles_responses() {
        let mock = MockLlmClient::new(vec!["first".into()]);
        let request = LlmRequest {
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            stream: true,
        };

        for _ in 0..3 {
            let stream = mock.chat_stream(request.clone()).await.unwrap();
            let chunks: Vec<_> = tokio_stream::StreamExt::collect(stream).await;
            if let Ok(LlmStreamChunk::Content(text)) = &chunks[0] {
                assert_eq!(text, "first");
            } else {
                panic!("Expected Content chunk");
            }
        }
    }

    #[tokio::test]
    async fn test_mock_llm_error_mode() {
        let mock = MockLlmClient::new(vec!["ok".into()]).with_error();
        let request = LlmRequest {
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            stream: true,
        };

        let result = mock.chat_stream(request).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_llm_with_delay() {
        let mock =
            MockLlmClient::new(vec!["delayed".into()]).with_delay(Duration::from_millis(10));
        let request = LlmRequest {
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            stream: true,
        };

        let start = std::time::Instant::now();
        let stream = mock.chat_stream(request).await.unwrap();
        let chunks: Vec<_> = tokio_stream::StreamExt::collect(stream).await;
        let elapsed = start.elapsed();

        assert!(!chunks.is_empty());
        assert!(
            elapsed >= Duration::from_millis(10),
            "Expected delay of at least 10ms",
        );
    }

    #[tokio::test]
    async fn test_mock_llm_done_chunk() {
        let mock = MockLlmClient::new(vec!["text".into()]);
        let request = LlmRequest {
            messages: vec![],
            tools: vec![],
            max_tokens: None,
            temperature: None,
            stream: true,
        };

        let stream = mock.chat_stream(request).await.unwrap();
        let chunks: Vec<_> = tokio_stream::StreamExt::collect(stream).await;

        assert!(!chunks.is_empty());
        let last = chunks.last().unwrap();
        assert!(
            matches!(last, Ok(LlmStreamChunk::Done { .. })),
            "Expected Done chunk, got: {:?}",
            last
        );
    }
}
