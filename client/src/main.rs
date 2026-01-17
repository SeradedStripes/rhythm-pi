use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;

mod audio;
mod websocket;
mod game;

slint::include_modules!();

#[derive(Debug, Deserialize, Serialize)]
struct ServerSong {
    id: String,
    filename: String,
    title: Option<String>,
    artists: Option<Vec<String>>,
}

fn fetch_songs_from_server(server_url: &str) -> Result<Vec<SongData>> {
    let url = format!("{}/api/songs", server_url);
    
    info!("Fetching songs from {}", url);
    
    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()?;
    
    let songs: Vec<ServerSong> = response.into_json()?;
    
    let song_data: Vec<SongData> = songs
        .into_iter()
        .map(|s| SongData {
            id: s.id.into(),
            title: s.title.unwrap_or_else(|| s.filename.clone()).into(),
            artists: s.artists
                .map(|a| a.join(", "))
                .unwrap_or_else(|| "Unknown".to_string())
                .into(),
        })
        .collect();
    
    Ok(song_data)
}

fn fetch_chart_from_server(server_url: &str, song_id: &str, instrument: &str, difficulty: &str) -> Result<game::Chart> {
    let encoded_id = urlencoding::encode(song_id);
    let url = format!("{}/api/songs/{}/chart?instrument={}&difficulty={}", server_url, encoded_id, instrument, difficulty);
    
    info!("Fetching chart from {}", url);
    
    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()?;
    
    let chart: game::Chart = response.into_json()?;
    
    Ok(chart)
}

fn stream_audio_from_websocket(ws_url: &str, song_id: &str) -> Result<Vec<u8>> {
    use tokio_tungstenite::{connect_async, tungstenite::Message};
    use futures::{SinkExt, StreamExt};
    
    let url = format!("{}/ws/audio/{}", ws_url, song_id);
    info!("Connecting to WebSocket: {}", url);
    
    // Create a new runtime for this blocking operation
    let rt = tokio::runtime::Runtime::new()?;
    
    rt.block_on(async {
        let (ws_stream, _) = connect_async(&url).await?;
        let (mut write, mut read) = ws_stream.split();
        
        // Send start command
        write.send(Message::Text("start".to_string())).await?;
        
        let mut audio_data = Vec::new();
        let mut chunk_count = 0;
        
        while let Some(msg) = read.next().await {
            match msg? {
                Message::Binary(data) => {
                    chunk_count += 1;
                    audio_data.extend_from_slice(&data);
                    if chunk_count % 10 == 0 {
                        info!("Received {} audio chunks", chunk_count);
                    }
                }
                Message::Text(text) => {
                    info!("Server message: {}", text);
                    if text.contains("complete") {
                        info!("Audio stream complete: {} total bytes", audio_data.len());
                        break;
                    }
                }
                Message::Close(_) => {
                    info!("Connection closed");
                    break;
                }
                _ => {}
            }
        }
        
        Ok::<Vec<u8>, anyhow::Error>(audio_data)
    })
}

fn main() -> Result<()> {
    env_logger::init();
    
    info!("Starting Rhythm Pi Client");
    
    let ui = MainWindow::new()?;
    let ui_weak = ui.as_weak();
    
    // Server configuration
    let server_url = std::env::var("SERVER_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let ws_url = std::env::var("WS_URL").unwrap_or_else(|_| "ws://localhost:8080".to_string());
    
    // Shared state
    let audio_context = Arc::new(Mutex::new(None));
    let chart_data = Arc::new(Mutex::new(None));
    
    // Load songs callback
    ui.on_load_songs({
        let ui_weak = ui_weak.clone();
        let server_url = server_url.clone();
        move || {
            match fetch_songs_from_server(&server_url) {
                Ok(songs) => {
                    info!("Loaded {} songs from server", songs.len());
                    if let Some(ui) = ui_weak.upgrade() {
                        let model = std::rc::Rc::new(slint::VecModel::from(songs));
                        ui.set_songs(model.into());
                    }
                }
                Err(e) => {
                    log::error!("Failed to load songs: {}", e);
                }
            }
        }
    });
    
    // Play song callback
    ui.on_play_song({
        let ui_weak = ui_weak.clone();
        let server_url = server_url.clone();
        let ws_url = ws_url.clone();
        let audio_context = audio_context.clone();
        let chart_data = chart_data.clone();
        
        move |song_id, difficulty, instrument| {
            info!("Playing song: {} ({} - {})", song_id, difficulty, instrument);
            
            // Initialize audio context
            match audio::AudioContext::new() {
                Ok(ctx) => {
                    *audio_context.lock().unwrap() = Some(ctx);
                }
                Err(e) => {
                    log::error!("Failed to initialize audio: {}", e);
                    return;
                }
            }
            
            // Fetch chart
            match fetch_chart_from_server(&server_url, &song_id, &instrument, &difficulty) {
                Ok(chart) => {
                    info!("Loaded chart with {} notes", chart.notes.len());
                    *chart_data.lock().unwrap() = Some(chart.clone());
                    
                    // Update UI with chart notes
                    if let Some(ui) = ui_weak.upgrade() {
                        let notes: Vec<NoteData> = chart.notes.iter().map(|n| NoteData {
                            time: n.time,
                            fret: n.col as i32,
                            duration: n.duration,
                        }).collect();
                        let model = std::rc::Rc::new(slint::VecModel::from(notes));
                        ui.set_chart_notes(model.into());
                    }
                }
                Err(e) => {
                    log::error!("Failed to fetch chart: {}", e);
                }
            }
            
            // Stream audio in a separate thread
            let song_id_clone = song_id.to_string();
            let ws_url_clone = ws_url.clone();
            let audio_context_clone = audio_context.clone();
            
            thread::spawn(move || {
                // URL-encode the song_id to handle special characters and spaces
                let encoded_song_id = urlencoding::encode(&song_id_clone);
                match stream_audio_from_websocket(&ws_url_clone, &encoded_song_id) {
                    Ok(audio_data) => {
                        info!("Audio stream received: {} bytes", audio_data.len());
                        
                        if let Some(ctx) = audio_context_clone.lock().unwrap().as_ref() {
                            if let Err(e) = ctx.play_bytes(audio_data) {
                                log::error!("Failed to play audio: {}", e);
                            } else {
                                info!("Audio playback started");
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to stream audio: {}", e);
                    }
                }
            });
        }
    });
    
    // Load songs on startup
    ui.invoke_load_songs();
    
    info!("UI initialized successfully");
    
    // Run the UI (this blocks until the window is closed)
    ui.run()?;
    
    Ok(())
}
