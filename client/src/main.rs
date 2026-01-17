use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

mod audio;
mod websocket;
mod game;
mod input;

slint::include_modules!();

use game::{GameState, ChartNote};
use input::{InputHandler, KeyBindings};

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
    let chart_data: Arc<Mutex<Option<game::Chart>>> = Arc::new(Mutex::new(None));
    let game_state = Arc::new(Mutex::new(GameState::new()));
    let input_handler = Arc::new(Mutex::new(InputHandler::with_default_bindings()));
    let game_running = Arc::new(Mutex::new(false));
    let game_start_time = Arc::new(Mutex::new(None::<Instant>));
    let game_timer = Arc::new(Mutex::new(None::<slint::Timer>));
    
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
        let game_state = game_state.clone();
        let input_handler = input_handler.clone();
        let game_running = game_running.clone();
        let game_start_time = game_start_time.clone();
        let game_timer = game_timer.clone();
        
        move |song_id, difficulty, instrument| {
            info!("Playing song: {} ({} - {})", song_id, difficulty, instrument);
            
            // Reset game state
            *game_state.lock().unwrap() = GameState::new();
            *game_running.lock().unwrap() = true;
            *game_start_time.lock().unwrap() = Some(Instant::now());
            info!("Game started, timer running");
            
            // Create a Slint Timer for the game loop (runs on UI thread)
            let ui_clone = ui_weak.clone();
            let game_state_timer = game_state.clone();
            let game_running_timer = game_running.clone();
            let game_start_time_timer = game_start_time.clone();
            let chart_data_timer = chart_data.clone();
            
            let timer = slint::Timer::default();
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(16), move || {
                let is_running = *game_running_timer.lock().unwrap();
                if !is_running {
                    return;
                }
                
                // Calculate current time from audio start
                let mut game = game_state_timer.lock().unwrap();
                
                if !game.is_paused {
                    if let Some(start_time) = *game_start_time_timer.lock().unwrap() {
                        let elapsed = start_time.elapsed().as_secs_f32();
                        game.current_time = elapsed;
                    }
                }
                
                // Capture values before releasing lock
                let current_time = game.current_time;
                let score_data = ScoreData {
                    score: game.score as i32,
                    combo: game.combo as i32,
                    max_combo: game.max_combo as i32,
                    health: game.health,
                    accuracy_perfect: game.accuracy_count.perfect as i32,
                    accuracy_great: game.accuracy_count.great as i32,
                    accuracy_good: game.accuracy_count.good as i32,
                    accuracy_ok: game.accuracy_count.ok as i32,
                    accuracy_miss: game.accuracy_count.miss as i32,
                };
                drop(game);
                
                // Update UI (safe because timer runs on UI thread)
                if let Some(ui) = ui_clone.upgrade() {
                    ui.set_current_playback_time(current_time);
                    ui.set_current_score(score_data);
                }
                
                // Check for missed notes
                if let Some(chart) = chart_data_timer.lock().unwrap().as_ref() {
                    let mut game = game_state_timer.lock().unwrap();
                    for note in &chart.notes {
                        let time_passed = current_time - note.time;
                        if time_passed > 0.2 && !game.notes_hit.iter().any(|h| h.note_time == note.time && h.note_lane == note.col) {
                            game.record_hit(note, current_time);
                        }
                    }
                }
            });
            
            // Store timer to keep it alive
            *game_timer.lock().unwrap() = Some(timer);
            
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
                    if chart.notes.len() > 0 {
                        info!("  First note: time={:.2}s, lane={}", chart.notes[0].time, chart.notes[0].col);
                    }
                    *chart_data.lock().unwrap() = Some(chart.clone());
                    
                    // Update UI with chart notes
                    if let Some(ui) = ui_weak.upgrade() {
                        let notes: Vec<NoteData> = chart.notes.iter().map(|n| NoteData {
                            time: n.time,
                            fret: n.col as i32,
                            duration: n.duration,
                        }).collect();
                        info!("Setting {} notes on UI", notes.len());
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
            let game_start_time_clone = game_start_time.clone();
            
            thread::spawn(move || {
                let encoded_song_id = urlencoding::encode(&song_id_clone);
                match stream_audio_from_websocket(&ws_url_clone, &encoded_song_id) {
                    Ok(audio_data) => {
                        info!("Audio stream received: {} bytes", audio_data.len());
                        
                        if let Some(ctx) = audio_context_clone.lock().unwrap().as_ref() {
                            *game_start_time_clone.lock().unwrap() = Some(Instant::now());
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
    
    // Pause song callback
    ui.on_pause_song({
        let game_state = game_state.clone();
        let audio_context = audio_context.clone();
        let game_timer = game_timer.clone();
        
        move || {
            info!("Pausing song");
            game_state.lock().unwrap().pause();
            if let Some(ctx) = audio_context.lock().unwrap().as_ref() {
                ctx.pause();
            }
            if let Some(timer) = game_timer.lock().unwrap().as_ref() {
                timer.stop();
            }
        }
    });
    
    // Resume song callback
    ui.on_resume_song({
        let game_state = game_state.clone();
        let audio_context = audio_context.clone();
        let game_timer = game_timer.clone();
        
        move || {
            info!("Resuming song");
            game_state.lock().unwrap().resume();
            if let Some(ctx) = audio_context.lock().unwrap().as_ref() {
                ctx.resume();
            }
            if let Some(timer) = game_timer.lock().unwrap().as_ref() {
                timer.restart();
            }
        }
    });
    
    // Key press callback
    ui.on_handle_key({
        let input_handler = input_handler.clone();
        let game_state = game_state.clone();
        let chart_data = chart_data.clone();
        let ui_weak = ui_weak.clone();
        
        move |key_str| {
            if let Some(key_char) = key_str.to_uppercase().chars().next() {
                let mut input = input_handler.lock().unwrap();
                let mut game = game_state.lock().unwrap();
                
                if let Some(event) = input.handle_key_press(key_char, game.current_time) {
                    // Check for nearby notes in the chart
                    if let Some(chart) = chart_data.lock().unwrap().as_ref() {
                        let hit_window = 0.2; // 200ms hit window
                        
                        for note in &chart.notes {
                            if note.col == event.lane && (note.time - game.current_time).abs() <= hit_window {
                                if !game.notes_hit.iter().any(|h| h.note_time == note.time && h.note_lane == event.lane) {
                                    let accuracy = game.record_hit(note, event.timestamp);
                                    
                                    info!("Hit {:?} on lane {} at time {}", accuracy, event.lane, game.current_time);
                                    
                                    if let Some(ui) = ui_weak.upgrade() {
                                        ui.set_current_score(ScoreData {
                                            score: game.score as i32,
                                            combo: game.combo as i32,
                                            max_combo: game.max_combo as i32,
                                            health: game.health,
                                            accuracy_perfect: game.accuracy_count.perfect as i32,
                                            accuracy_great: game.accuracy_count.great as i32,
                                            accuracy_good: game.accuracy_count.good as i32,
                                            accuracy_ok: game.accuracy_count.ok as i32,
                                            accuracy_miss: game.accuracy_count.miss as i32,
                                        });
                                    }
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    });
    
    // Load songs on startup
    ui.invoke_load_songs();
    
    info!("UI initialized successfully");
    
    // Run the UI (this blocks until the window is closed)
    ui.run()?;
    
    Ok(())
}
