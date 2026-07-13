use std::io::{self, Write};
use std::sync::Arc;

use async_trait::async_trait;
use seat_agent_core::{
    Agent, AgentConfig, AgentEvent, AgentInput, BusinessBackend, LlmClient, LlmRequest,
    LlmStreamChunk, Message, MessageRole,
};
use seat_agent_tools::business::{
    ComplaintQueryTool, MockBusinessBackend, OrderQueryTool, RefundQueryTool,
};
use seat_agent_tools::llm::OpenAiLlmClient;
use seat_agent_tools::transfer::TransferToHumanTool;

// ============================================================================
// Mock LLM（无 API key 时使用）
// ============================================================================

struct JsonToolCallMock {
    responses: Vec<String>,
    idx: std::sync::atomic::AtomicUsize,
}

impl JsonToolCallMock {
    fn new(responses: Vec<String>) -> Self {
        Self {
            responses,
            idx: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

#[async_trait]
impl LlmClient for JsonToolCallMock {
    async fn chat_stream(
        &self,
        _req: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>,
        >,
    > {
        let i = self.idx.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let resp = &self.responses[i % self.responses.len()];
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(resp);
        let chunks: Vec<seat_agent_core::Result<LlmStreamChunk>> = match parsed {
            Ok(serde_json::Value::Object(obj)) if obj.contains_key("tool_calls") => {
                let calls = obj["tool_calls"].as_array().cloned().unwrap_or_default();
                let mut result = Vec::new();
                for call in &calls {
                    let id = call["id"].as_str().unwrap_or("unknown").to_string();
                    let name = call["function"]["name"]
                        .as_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let args = call["function"]["arguments"]
                        .as_str()
                        .unwrap_or("{}")
                        .to_string();
                    result.push(Ok(LlmStreamChunk::ToolCallStart { id, name }));
                    result.push(Ok(LlmStreamChunk::ToolCallDelta { arguments: args }));
                }
                result.push(Ok(LlmStreamChunk::Done {
                    finish_reason: seat_agent_core::FinishReason::ToolCalls,
                }));
                result
            }
            _ => vec![
                Ok(LlmStreamChunk::Content(resp.clone())),
                Ok(LlmStreamChunk::Done {
                    finish_reason: seat_agent_core::FinishReason::Stop,
                }),
            ],
        };
        Ok(Box::pin(tokio_stream::iter(chunks)))
    }
}

// ============================================================================
// 桥接 Arc → Box
// ============================================================================

struct LlmBridge(Arc<dyn LlmClient>);

#[async_trait]
impl LlmClient for LlmBridge {
    async fn chat_stream(
        &self,
        request: LlmRequest,
    ) -> seat_agent_core::Result<
        std::pin::Pin<
            Box<dyn futures::Stream<Item = seat_agent_core::Result<LlmStreamChunk>> + Send>,
        >,
    > {
        self.0.chat_stream(request).await
    }
}

// ============================================================================
// 音频采集（cpal）
// ============================================================================

#[cfg(feature = "audio")]

/// 从麦克风录音，返回 PCM f32 采样和采样率
fn record_audio() -> Result<(Vec<f32>, u32), Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or("找不到录音设备（请检查麦克风）")?;

    let config = device.default_input_config()?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels() as usize;

    let samples = Arc::new(std::sync::Mutex::new(Vec::<f32>::new()));
    let samples_clone = samples.clone();

    let err_fn = |err: cpal::StreamError| eprintln!("录音错误: {}", err);

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buf) = samples_clone.lock() {
                    // 简单降混：多声道 → 单声道
                    for chunk in data.chunks(channels) {
                        let mono: f32 = chunk.iter().sum::<f32>() / channels as f32;
                        buf.push(mono);
                    }
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buf) = samples_clone.lock() {
                    for chunk in data.chunks(channels) {
                        let mono: f32 =
                            chunk.iter().map(|&s| s as f32 / i16::MAX as f32).sum::<f32>()
                                / channels as f32;
                        buf.push(mono);
                    }
                }
            },
            err_fn,
            None,
        )?,
        cpal::SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _: &cpal::InputCallbackInfo| {
                if let Ok(mut buf) = samples_clone.lock() {
                    for chunk in data.chunks(channels) {
                        let mono: f32 = chunk
                            .iter()
                            .map(|&s| (s as f32 - 32768.0) / 32768.0)
                            .sum::<f32>()
                            / channels as f32;
                        buf.push(mono);
                    }
                }
            },
            err_fn,
            None,
        )?,
        fmt => return Err(format!("不支持的采样格式: {:?}", fmt).into()),
    };

    stream.play()?;
    println!("  [录音中...]");

    // 等待用户按 Enter 停止
    io::stdout().flush()?;
    let mut dummy = String::new();
    io::stdin().read_line(&mut dummy)?;

    drop(stream);

    let recorded = match samples.lock() {
        Ok(buf) => buf.clone(),
        Err(_) => return Err("录音锁失败".into()),
    };

    println!("  [录音完成] {}ms, {} 采样", recorded.len() * 1000 / sample_rate as usize, recorded.len());
    Ok((recorded, sample_rate))
}
#[cfg(not(feature = "audio"))]
fn record_audio() -> Result<(Vec<f32>, u32), Box<dyn std::error::Error>> {
    Err("音频功能未启用（编译时未包含 audio feature）".into())
}


fn pcm_to_wav(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let num_samples = samples.len() as u32;
    let byte_rate = sample_rate * 2; // 16-bit mono
    let block_align = 2u16;
    let data_size = num_samples * 2;

    let mut wav = Vec::with_capacity(44 + data_size as usize);

    // RIFF header
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");

    // fmt chunk
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes()); // chunk size
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes()); // bits per sample

    // data chunk
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());

    for &s in samples {
        let i16_sample = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        wav.extend_from_slice(&i16_sample.to_le_bytes());
    }

    wav
}

// ============================================================================
// STT — OpenAI Whisper API
// ============================================================================

/// 语音转文字（OpenAI Whisper API 兼容）
async fn speech_to_text(
    audio_wav: &[u8],
    api_key: &str,
    base_url: &str,
) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!("{}/audio/transcriptions", base_url);

    let part = reqwest::multipart::Part::bytes(audio_wav.to_vec())
        .file_name("audio.wav")
        .mime_str("audio/wav")?;

    let form = reqwest::multipart::Form::new()
        .part("file", part)
        .text("model", "whisper-1")
        .text("language", "zh");

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .multipart(form)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("STT 请求失败 ({}): {}", status, body).into());
    }

    let result: serde_json::Value = resp.json().await?;
    Ok(result["text"].as_str().unwrap_or("").to_string())
}

// ============================================================================
// TTS — OpenAI TTS API
// ============================================================================

/// 文字转语音（OpenAI TTS API 兼容），返回 MP3 字节
async fn text_to_speech(
    text: &str,
    api_key: &str,
    base_url: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let url = format!("{}/audio/speech", base_url);

    let body = serde_json::json!({
        "model": "tts-1",
        "input": text,
        "voice": "alloy",
        "response_format": "mp3",
    });

    let client = reqwest::Client::new();
    let resp = client
        .post(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&body)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("TTS 请求失败 ({}): {}", status, body).into());
    }

    Ok(resp.bytes().await?.to_vec())
}

// ============================================================================
// 扬声器播放（rodio）
// ============================================================================

#[cfg(feature = "audio")]

/// 播放 MP3 音频
fn play_audio(mp3_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    use rodio::{Decoder, OutputStream, Sink};
    use std::io::Cursor;

    let (_stream, handle) = OutputStream::try_default()?;
    let sink = Sink::try_new(&handle)?;

    let cursor = Cursor::new(mp3_data.to_vec());
    let decoder = Decoder::new(cursor)?;
    sink.append(decoder);

    println!("  [播放中...]");
    sink.sleep_until_end();
    Ok(())
}
#[cfg(not(feature = "audio"))]
fn play_audio(_mp3_data: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    Err("音频功能未启用（编译时未包含 audio feature）".into())
}


// ============================================================================
// Main — 语音交互循环
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let backend: Arc<dyn BusinessBackend> = Arc::new(MockBusinessBackend::new());

    // LLM
    let llm = OpenAiLlmClient::from_env()?;

    // STT（语音识别）— 独立配置，fallback 到 LLM
    let stt_api_key = std::env::var("STT_API_KEY")
        .or_else(|_| std::env::var("LLM_API_KEY"))
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|_| "请设置 STT_API_KEY 或 LLM_API_KEY 环境变量")?;
    let stt_base_url = std::env::var("STT_BASE_URL")
        .or_else(|_| std::env::var("LLM_BASE_URL"))
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    // TTS（语音合成）— 独立配置，fallback 到 LLM
    let tts_api_key = std::env::var("TTS_API_KEY")
        .or_else(|_| std::env::var("LLM_API_KEY"))
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|_| "请设置 TTS_API_KEY 或 LLM_API_KEY 环境变量")?;
    let tts_base_url = std::env::var("TTS_BASE_URL")
        .or_else(|_| std::env::var("LLM_BASE_URL"))
        .or_else(|_| std::env::var("OPENAI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.openai.com/v1".to_string());

    let llm = OpenAiLlmClient::from_env()?;
    println!("=== seat-agent 语音客服系统 ===");
    println!("LLM: {}", llm.model_name());
    println!("STT: Whisper API ({})", stt_base_url);
    println!("TTS: OpenAI TTS ({})", tts_base_url);
    println!("按 Enter 开始录音，再次按 Enter 停止，输入 quit 退出\n");

    let config = AgentConfig::voice();
    let agent = Agent::new(config, Box::new(LlmBridge(Arc::new(llm))));

    let mut agent = agent;
    agent.register_tool(Box::new(OrderQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(RefundQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(ComplaintQueryTool::new(backend.clone())));
    agent.register_tool(Box::new(TransferToHumanTool::new()));

    let agent = Arc::new(agent);
    let session_id = "voice-demo".to_string();
    let customer_id = "voice-customer".to_string();

    loop {
        print!("按 Enter 开始录音（或输入 quit 退出）: ");
        io::stdout().flush()?;

        let mut cmd = String::new();
        io::stdin().read_line(&mut cmd)?;
        let cmd = cmd.trim().to_string();

        if cmd == "quit" || cmd == "exit" {
            println!("再见！");
            break;
        }

        // 1. 录音
        let (samples, sample_rate) = match record_audio() {
            Ok(r) => r,
            Err(e) => {
                println!("  [录音失败] {}\n", e);
                continue;
            }
        };

        if samples.is_empty() {
            println!("  [未录到音频]\n");
            continue;
        }

        // 2. WAV 编码
        let wav = pcm_to_wav(&samples, sample_rate);

        // 3. STT 语音识别
        print!("  [识别中...] ");
        io::stdout().flush()?;
        let text = match speech_to_text(&wav, &stt_api_key, &stt_base_url).await {
            Ok(t) => t,
            Err(e) => {
                println!("失败: {}", e);
                continue;
            }
        };

        if text.trim().is_empty() {
            println!("(未识别到内容)\n");
            continue;
        }

        println!("\"{}\"", text);

        // 4. Agent 处理
        let (tx, mut rx) = tokio::sync::mpsc::channel(200);

        let input = AgentInput {
            session_id: session_id.clone(),
            customer_id: customer_id.clone(),
            message: Message {
                role: MessageRole::User,
                content: text,
                tool_calls: None,
                tool_call_id: None,
            },
        };

        print!("  [Agent] ");
        io::stdout().flush()?;

        let agent_clone = agent.clone();
        let handle = tokio::spawn(async move { agent_clone.on_message(input, tx).await });

        // 收集完整回复文本
        let mut reply = String::new();
        while let Some(event) = rx.recv().await {
            match event {
                AgentEvent::StreamStart => {}
                AgentEvent::Token(token) => {
                    print!("{}", token);
                    io::stdout().flush()?;
                    reply.push_str(&token);
                }
                AgentEvent::StreamEnd => println!(),
                AgentEvent::ToolCallStart {
                    tool_name,
                    arguments,
                } => {
                    println!("\n    [工具] {} ({})", tool_name, arguments);
                }
                AgentEvent::ToolCallEnd { tool_name, result } => {
                    println!("    [结果] {}: {}", tool_name, result);
                    print!("  [Agent] ");
                    io::stdout().flush()?;
                }
                AgentEvent::TransferToHuman { reason } => {
                    println!("\n    [转人工] {}", reason);
                }
                AgentEvent::Error(err) => {
                    println!("\n    [错误] {}", err);
                }
            }
        }

        if let Err(e) = handle.await? {
            println!("  [Agent 错误] {}", e);
            continue;
        }

        // 5. TTS 语音合成 + 播放
        if !reply.trim().is_empty() {
            print!("  [合成语音...] ");
            io::stdout().flush()?;
            match text_to_speech(&reply, &tts_api_key, &tts_base_url).await {
                Ok(mp3) => {
                    println!("OK ({}KB)", mp3.len() / 1024);
                    if let Err(e) = play_audio(&mp3) {
                        println!("  [播放失败] {}", e);
                    }
                }
                Err(e) => println!("失败: {}", e),
            }
        }

        println!();
    }

    Ok(())
}
