use anyhow::Result;
use log::{info, error, warn, debug};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::sink::SinkExt;
use futures::stream::StreamExt;

#[derive(Debug, Clone)]
pub struct AudioChunk {
    pub data: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ServerStatus {
    pub status: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct GameStateUpdate {
    pub score: u32,
    pub combo: u32,
    pub health: f32,
}

pub struct AudioStreamClient {
    sender: mpsc::UnboundedSender<Message>,
    audio_receiver: mpsc::UnboundedReceiver<AudioChunk>,
}

impl AudioStreamClient {
    /// Connect to the server's WebSocket audio streaming endpoint
    /// URL format: ws://server:port/ws/audio/{song_id}
    pub async fn connect_audio_stream(server_url: &str, song_id: &str) -> Result<Self> {
        let ws_url = format!("{}/ws/audio/{}", server_url, song_id);
        info!("Connecting to audio stream at {}", ws_url);
        
        let (ws_stream, response) = connect_async(&ws_url).await?;
        info!("WebSocket connected, response: {:?}", response.status());
        
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel();
        let (audio_tx, audio_rx) = mpsc::unbounded_channel();
        
        // Spawn task to handle outgoing messages (commands to server)
        tokio::spawn(async move {
            while let Some(msg) = cmd_rx.recv().await {
                if let Err(e) = ws_sender.send(msg).await {
                    error!("Failed to send WebSocket message: {}", e);
                    break;
                }
            }
            info!("WebSocket sender task terminated");
        });
        
        // Spawn task to receive messages from server
        tokio::spawn(async move {
            while let Some(result) = futures::stream::StreamExt::next(&mut ws_receiver).await {
                match result {
                    Ok(Message::Binary(data)) => {
                        debug!("Received audio chunk: {} bytes", data.len());
                        let chunk = AudioChunk { data };
                        if audio_tx.send(chunk).is_err() {
                            warn!("Audio receiver dropped, stopping stream");
                            break;
                        }
                    }
                    Ok(Message::Text(text)) => {
                        match serde_json::from_str::<ServerStatus>(&text) {
                            Ok(status) => {
                                if let Some(s) = status.status {
                                    info!("Server status: {}", s);
                                    if s == "complete" {
                                        info!("Audio stream complete");
                                        break;
                                    }
                                }
                                if let Some(e) = status.error {
                                    error!("Server error: {}", e);
                                    break;
                                }
                            }
                            Err(e) => {
                                warn!("Failed to parse status message: {} - {}", e, text);
                            }
                        }
                    }
                    Ok(Message::Close(reason)) => {
                        info!("WebSocket closed: {:?}", reason);
                        break;
                    }
                    Ok(Message::Ping(_data)) => {
                        debug!("Received ping, sending pong");
                        // Pong is automatically sent by tungstenite
                    }
                    Ok(Message::Pong(_)) => {
                        debug!("Received pong");
                    }
                    Ok(msg) => {
                        debug!("Received other message: {:?}", msg);
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
            info!("WebSocket receiver task terminated");
        });
        
        Ok(Self {
            sender: cmd_tx,
            audio_receiver: audio_rx,
        })
    }
    
    /// Start the audio stream (send "start" command to server)
    pub fn start_stream(&self) -> Result<()> {
        info!("Sending start command to server");
        self.sender.send(Message::Text("start".to_string()))?;
        Ok(())
    }
    
    /// Stop the audio stream (send "stop" command to server)
    pub fn stop_stream(&self) -> Result<()> {
        info!("Sending stop command to server");
        self.sender.send(Message::Text("stop".to_string()))?;
        Ok(())
    }
    
    /// Receive next audio chunk (non-blocking)
    pub fn recv_audio_chunk(&mut self) -> Option<AudioChunk> {
        self.audio_receiver.try_recv().ok()
    }
    
    /// Receive all available audio chunks
    pub fn recv_all_chunks(&mut self) -> Vec<AudioChunk> {
        let mut chunks = Vec::new();
        while let Ok(chunk) = self.audio_receiver.try_recv() {
            chunks.push(chunk);
        }
        chunks
    }
    
    /// Wait for next audio chunk (blocking)
    pub async fn recv_audio_chunk_async(&mut self) -> Option<AudioChunk> {
        self.audio_receiver.recv().await
    }
}

/// General purpose WebSocket client for game state updates
pub struct WebSocketClient {
    sender: mpsc::UnboundedSender<Message>,
}

impl WebSocketClient {
    pub async fn new(url: &str) -> Result<Self> {
        info!("Connecting to WebSocket server at {}", url);
        
        let (ws_stream, _) = connect_async(url).await?;
        let (mut ws_sender, mut ws_receiver) = ws_stream.split();
        
        let (tx, mut rx) = mpsc::unbounded_channel();
        
        // Spawn task to handle outgoing messages
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                if let Err(e) = ws_sender.send(msg).await {
                    error!("Failed to send message: {}", e);
                    break;
                }
            }
        });
        
        // Spawn task to receive messages
        tokio::spawn(async move {
            while let Some(result) = futures::stream::StreamExt::next(&mut ws_receiver).await {
                match result {
                    Ok(msg) => {
                        debug!("Received message: {:?}", msg);
                    }
                    Err(e) => {
                        error!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        });
        
        Ok(Self { sender: tx })
    }
    
    pub fn send_game_state(&self, state: GameStateUpdate) -> Result<()> {
        let msg = serde_json::to_string(&state)?;
        self.sender.send(Message::Text(msg))?;
        Ok(())
    }
}
