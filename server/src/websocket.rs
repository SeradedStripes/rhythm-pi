use actix::{Actor, StreamHandler, AsyncContext, ActorContext};
use actix_web::{web, HttpRequest, HttpResponse, Error};
use actix_web_actors::ws;
use std::path::PathBuf;
use std::time::Duration;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use bytes::Bytes;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const CLIENT_TIMEOUT: Duration = Duration::from_secs(10);
const CHUNK_SIZE: usize = 8192; // 8KB chunks for streaming

/// WebSocket actor for audio streaming
pub struct AudioStreamWs {
    song_id: String,
    hb: std::time::Instant,
    started: bool,
}

impl AudioStreamWs {
    fn new(song_id: String) -> Self {
        Self {
            song_id,
            hb: std::time::Instant::now(),
            started: false,
        }
    }

    /// Helper method to send heartbeat ping
    fn heartbeat(&self, ctx: &mut ws::WebsocketContext<Self>) {
        ctx.run_interval(HEARTBEAT_INTERVAL, |act, ctx| {
            if std::time::Instant::now().duration_since(act.hb) > CLIENT_TIMEOUT {
                log::warn!("WebSocket client heartbeat timeout, disconnecting");
                ctx.stop();
                return;
            }
            ctx.ping(b"");
        });
    }

    /// Stream audio file in chunks
    fn stream_audio(&mut self, ctx: &mut ws::WebsocketContext<Self>) {
        let song_id = self.song_id.clone();
        let songs_dir = std::env::var("SONGS_DIR")
            .unwrap_or_else(|_| "server/assets/songs".to_string());

        // Find the audio file
        let audio_path = match find_audio_file(&songs_dir, &song_id) {
            Some(path) => path,
            None => {
                log::error!("Audio file not found for song_id: {}", song_id);
                ctx.text("{\"error\":\"audio file not found\"}");
                ctx.stop();
                return;
            }
        };

        log::info!("Starting WebSocket audio stream for: {:?}", audio_path);

        // Spawn async task to read and stream the file
        let addr = ctx.address();
        actix::spawn(async move {
            match stream_file_chunks(audio_path).await {
                Ok(chunks) => {
                    for chunk in chunks {
                        addr.do_send(AudioChunk(chunk));
                    }
                    addr.do_send(StreamComplete);
                }
                Err(e) => {
                    log::error!("Error streaming audio file: {}", e);
                    addr.do_send(StreamError(format!("{}", e)));
                }
            }
        });
    }
}

impl Actor for AudioStreamWs {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        log::info!("WebSocket connection started for song: {}", self.song_id);
        self.heartbeat(ctx);
    }

    fn stopped(&mut self, _ctx: &mut Self::Context) {
        log::info!("WebSocket connection closed for song: {}", self.song_id);
    }
}

/// Handle incoming WebSocket messages
impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for AudioStreamWs {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        match msg {
            Ok(ws::Message::Ping(msg)) => {
                self.hb = std::time::Instant::now();
                ctx.pong(&msg);
            }
            Ok(ws::Message::Pong(_)) => {
                self.hb = std::time::Instant::now();
            }
            Ok(ws::Message::Text(text)) => {
                let text = text.trim();
                
                // Client sends "start" to begin streaming
                if text == "start" && !self.started {
                    self.started = true;
                    log::info!("Client requested audio stream start");
                    ctx.text("{\"status\":\"streaming\"}");
                    self.stream_audio(ctx);
                } else if text == "stop" {
                    log::info!("Client requested stream stop");
                    ctx.stop();
                } else {
                    log::debug!("Received text message: {}", text);
                }
            }
            Ok(ws::Message::Binary(_)) => {
                log::warn!("Received unexpected binary message from client");
            }
            Ok(ws::Message::Close(reason)) => {
                log::info!("Client requested close: {:?}", reason);
                ctx.stop();
            }
            _ => (),
        }
    }
}

// Messages for actor communication
#[derive(actix::Message)]
#[rtype(result = "()")]
struct AudioChunk(Bytes);

#[derive(actix::Message)]
#[rtype(result = "()")]
struct StreamComplete;

#[derive(actix::Message)]
#[rtype(result = "()")]
struct StreamError(String);

impl actix::Handler<AudioChunk> for AudioStreamWs {
    type Result = ();

    fn handle(&mut self, msg: AudioChunk, ctx: &mut Self::Context) {
        ctx.binary(msg.0);
    }
}

impl actix::Handler<StreamComplete> for AudioStreamWs {
    type Result = ();

    fn handle(&mut self, _msg: StreamComplete, ctx: &mut Self::Context) {
        log::info!("Audio stream complete");
        ctx.text("{\"status\":\"complete\"}");
        ctx.stop();
    }
}

impl actix::Handler<StreamError> for AudioStreamWs {
    type Result = ();

    fn handle(&mut self, msg: StreamError, ctx: &mut Self::Context) {
        log::error!("Stream error: {}", msg.0);
        ctx.text(format!("{{\"error\":\"{}\"}}", msg.0));
        ctx.stop();
    }
}

/// Find audio file in directory
fn find_audio_file(songs_dir: &str, song_id: &str) -> Option<PathBuf> {
    let dir = std::fs::read_dir(songs_dir).ok()?;
    
    for entry in dir.filter_map(|e| e.ok()) {
        if let Some(name) = entry.file_name().to_str() {
            if name.starts_with(song_id) {
                let ext = name.rsplit('.').next().unwrap_or("");
                if matches!(ext, "mp3" | "wav" | "ogg" | "flac") {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

/// Read file and split into chunks
async fn stream_file_chunks(path: PathBuf) -> std::io::Result<Vec<Bytes>> {
    let mut file = File::open(&path).await?;
    let metadata = file.metadata().await?;
    let file_size = metadata.len() as usize;
    
    log::info!("Streaming file: {:?}, size: {} bytes", path, file_size);
    
    let mut chunks = Vec::new();
    let mut buffer = vec![0u8; CHUNK_SIZE];
    
    loop {
        let n = file.read(&mut buffer).await?;
        if n == 0 {
            break;
        }
        chunks.push(Bytes::copy_from_slice(&buffer[..n]));
    }
    
    log::info!("File split into {} chunks", chunks.len());
    Ok(chunks)
}

/// WebSocket handler endpoint
pub async fn ws_audio_stream(
    req: HttpRequest,
    stream: web::Payload,
    path: web::Path<String>,
) -> Result<HttpResponse, Error> {
    let song_id = path.into_inner();
    
    log::info!("WebSocket connection request for song: {}", song_id);
    
    let ws = AudioStreamWs::new(song_id);
    let resp = ws::start(ws, &req, stream)?;
    
    Ok(resp)
}
