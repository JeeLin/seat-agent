pub mod proto {
    tonic::include_proto!("seat_agent");
}

use std::pin::Pin;
use std::sync::Arc;

use futures::Stream;
use seat_agent_core::agent::Agent;
use seat_agent_core::context::{AgentEvent, AgentInput, Message};
use seat_agent_core::traits::MessageRole;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

use proto::agent_service_server::{AgentService, AgentServiceServer};
use proto::{ChatRequest, ChatResponse, TokenEvent};

/// gRPC Agent 服务实现
pub struct AgentGrpcServer {
    agent: Arc<Agent>,
}

impl AgentGrpcServer {
    pub fn new(agent: Agent) -> Self {
        Self {
            agent: Arc::new(agent),
        }
    }

    pub fn into_service(self) -> AgentServiceServer<Self> {
        AgentServiceServer::new(self)
    }
}

#[tonic::async_trait]
impl AgentService for AgentGrpcServer {
    type ChatStream = Pin<Box<dyn Stream<Item = Result<ChatResponse, Status>> + Send>>;

    async fn chat(
        &self,
        request: Request<Streaming<ChatRequest>>,
    ) -> Result<Response<Self::ChatStream>, Status> {
        let mut stream = request.into_inner();
        let (tx, rx) = mpsc::channel(128);
        let agent = self.agent.clone();

        // 处理客户端消息流
        tokio::spawn(async move {
            while let Some(result) = stream.message().await.transpose() {
                match result {
                    Ok(req) => {
                        if let Some(message) = req.message {
                            let session_id = req.session_id.clone();
                            let agent_input = match message {
                                proto::chat_request::Message::Text(text) => AgentInput {
                                    session_id: req.session_id,
                                    customer_id: req.customer_id,
                                    message: Message {
                                        role: MessageRole::User,
                                        content: text.content,
                                        tool_calls: None,
                                        tool_call_id: None,
                                    },
                                },
                            };

                            // 处理 Agent 事件流
                            let (event_tx, mut event_rx) = mpsc::channel(128);
                            if let Err(e) = agent.on_message(agent_input, event_tx).await {
                                let _ = tx
                                    .send(Err(Status::internal(format!("Agent 处理错误: {}", e))))
                                    .await;
                                continue;
                            }

                            while let Some(event) = event_rx.recv().await {
                                let response = match event {
                                    AgentEvent::StreamStart => {
                                        Some(proto::chat_response::Event::StreamStart(
                                            proto::StreamStartEvent {},
                                        ))
                                    }
                                    AgentEvent::StreamEnd => {
                                        Some(proto::chat_response::Event::StreamEnd(
                                            proto::StreamEndEvent {},
                                        ))
                                    }
                                    AgentEvent::Token(content) => {
                                        Some(proto::chat_response::Event::Token(TokenEvent {
                                            content,
                                        }))
                                    }
                                    AgentEvent::ToolCallStart {
                                        tool_name,
                                        arguments,
                                    } => Some(proto::chat_response::Event::ToolCallStart(
                                        proto::ToolCallStartEvent {
                                            tool_name,
                                            arguments,
                                        },
                                    )),
                                    AgentEvent::ToolCallEnd { tool_name, result } => {
                                        Some(proto::chat_response::Event::ToolCallEnd(
                                            proto::ToolCallEndEvent { tool_name, result },
                                        ))
                                    }
                                    AgentEvent::TransferToHuman { reason } => {
                                        Some(proto::chat_response::Event::Transfer(
                                            proto::TransferEvent { reason },
                                        ))
                                    }
                                    AgentEvent::Error(msg) => {
                                        Some(proto::chat_response::Event::Error(
                                            proto::ErrorEvent { message: msg },
                                        ))
                                    }
                                };

                                if let Some(event) = response {
                                    if tx
                                        .send(Ok(ChatResponse {
                                            session_id: session_id.clone(),
                                            event: Some(event),
                                        }))
                                        .await
                                        .is_err()
                                    {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        let _ = tx
                            .send(Err(Status::internal(format!("流处理错误: {}", e))))
                            .await;
                        break;
                    }
                }
            }
        });

        let stream = ReceiverStream::new(rx);
        Ok(Response::new(Box::pin(stream)))
    }
}
