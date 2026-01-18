use anyhow::Result;
use log::info;
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use rdev::{listen, Event, EventType, Key};

mod audio;
mod websocket;
mod game;
mod input;

slint::include_modules!();

use game::GameState;
use input::InputHandler;

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
        let game_running = game_running.clone();
        let game_start_time = game_start_time.clone();
        let game_timer = game_timer.clone();
        
        move |song_id, difficulty, instrument| {
            info!("Playing song: {} ({} - {})", song_id, difficulty, instrument);
            
            // Request focus on the key input
            if let Some(ui) = ui_weak.upgrade() {
                ui.invoke_focus_key_input();
            }
            
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
            let frame_count = Arc::new(Mutex::new(0));
            let last_log = Arc::new(Mutex::new(Instant::now()));
            let frame_count_timer = frame_count.clone();
            let last_log_timer = last_log.clone();
            
            timer.start(slint::TimerMode::Repeated, Duration::from_millis(12), move || {
                let is_running = *game_running_timer.lock().unwrap();
                if !is_running {
                    return;
                }
                
                let mut fc = frame_count_timer.lock().unwrap();
                *fc += 1;
                let frame = *fc;
                drop(fc);
                
                let should_log = frame % 60 == 0;
                let should_update_ui = frame % 1 == 0;
                let should_check_misses = frame % 1 == 0;
                
                if should_log {
                    let mut ll = last_log_timer.lock().unwrap();
                    let elapsed = ll.elapsed().as_secs_f32();
                    let fps = 120.0 / elapsed;
                    eprintln!("Timer FPS: {:.1}", fps);
                    *ll = Instant::now();
                }
                
                // Calculate current time from audio start
                let current_time = if let Some(start_time) = *game_start_time_timer.lock().unwrap() {
                    start_time.elapsed().as_secs_f32()
                } else {
                    0.0
                };
                
                // Update game state time (fast operation)
                {
                    let mut game = game_state_timer.lock().unwrap();
                    if !game.is_paused {
                        game.current_time = current_time;
                    }
                }
                
                // Update UI less frequently (this is expensive)
                if should_update_ui {
                    if let Some(ui) = ui_clone.upgrade() {
                        ui.set_current_playback_time(current_time);
                    }
                }
                
                // Check for missed notes (least frequently)
                if should_check_misses {
                    if let Some(chart) = chart_data_timer.lock().unwrap().as_ref() {
                        let mut game = game_state_timer.lock().unwrap();
                        
                        // Only check notes within a reasonable time window
                        for (i, note) in chart.notes.iter().enumerate() {
                            let time_diff = current_time - note.time;
                            
                            // Only check notes that are slightly past their hit time
                            if time_diff > 0.2 && time_diff < 0.5 {
                                if !game.notes_hit.iter().any(|h| h.note_index == i) {
                                    game.record_hit(i, note, current_time);
                                }
                            }
                        }
                        
                        // Filter out hit notes from the UI
                        if let Some(ui) = ui_clone.upgrade() {
                            let notes: Vec<NoteData> = chart.notes.iter().enumerate().filter_map(|(i, n)| {
                                // Skip notes that have been hit
                                if game.notes_hit.iter().any(|h| h.note_index == i) {
                                    return None;
                                }
                                
                                // Clamp to 4 lanes for current UI
                                let lane = if chart.columns > 4 && n.col >= 4 {
                                    3
                                } else {
                                    n.col.min(3)
                                } as i32;

                                Some(NoteData { time: n.time, fret: lane, duration: n.duration })
                            }).collect();
                            let model = std::rc::Rc::new(slint::VecModel::from(notes));
                            ui.set_chart_notes(model.into());
                        }
                        
                        // Update score UI
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
                        
                        if let Some(ui) = ui_clone.upgrade() {
                            ui.set_current_score(score_data);
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
                        let notes: Vec<NoteData> = chart.notes.iter().map(|n| {
                            // Clamp to 4 lanes for current UI; map any extra lanes to the last lane
                            let lane = if chart.columns > 4 && n.col >= 4 {
                                3 // map 4th+ columns to lane 3 to avoid UI issues
                            } else {
                                n.col.min(3)
                            } as i32;

                            NoteData {
                                time: n.time,
                                fret: lane,
                                duration: n.duration,
                            }
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
            //eprintln!("=== KEY CALLBACK ===");
            //eprintln!("Received key_str: {:?}", key_str);
            //eprintln!("Length: {}", key_str.len());
            //eprintln!("Bytes: {:?}", key_str.as_bytes());
            
            info!("Key pressed: {}", key_str);
            if let Some(key_char) = key_str.to_uppercase().chars().next() {
                eprintln!("Extracted char: {:?}", key_char);
                let mut input = input_handler.lock().unwrap();
                let mut game = game_state.lock().unwrap();
                
                info!("Handling key '{}' at game time {:.3}s", key_char, game.current_time);
                
                if let Some(event) = input.handle_key_press(key_char, game.current_time) {
                    // Log input pressed state for the lane
                    info!("UI Input state lane {} pressed?: {}", event.lane, input.is_lane_pressed(event.lane));
                    // Highlight the key button
                    if let Some(ui) = ui_weak.upgrade() {
                        match event.lane {
                            0 => ui.invoke_set_lane_pressed(0),
                            1 => ui.invoke_set_lane_pressed(1),
                            2 => ui.invoke_set_lane_pressed(2),
                            3 => ui.invoke_set_lane_pressed(3),
                            _ => {}
                        }
                    }
                    
                    // Check for nearby notes in the chart
                    if let Some(chart) = chart_data.lock().unwrap().as_ref() {
                        let hit_window = 0.3; // 300ms hit window

                        // Gather candidate notes in the hit window for this lane and log them
                        let candidates: Vec<(usize, &game::ChartNote)> = chart.notes.iter().enumerate()
                            .filter(|(_i, n)| n.col == event.lane && (n.time - game.current_time).abs() <= hit_window)
                            .collect();

                        info!("Key press on lane {} at {:.3}s; {} candidate(s): {:?}; already hit count={}",
                            event.lane,
                            game.current_time,
                            candidates.len(),
                            candidates.iter().map(|(i, n)| (i, n.time)).collect::<Vec<_>>(),
                            game.notes_hit.len()
                        );

                        let mut did_hit = false;
                        for (i_ref, note_ref) in &candidates {
                            let i = *i_ref;
                            let note = *note_ref;
                            if !game.notes_hit.iter().any(|h| h.note_index == i) {
                                let accuracy = game.record_hit(i, note, event.timestamp);

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

                                    // Update UI note model immediately to remove hit notes
                                    let notes: Vec<NoteData> = chart.notes.iter().enumerate().filter_map(|(i, n)| {
                                        // Skip notes that have been hit
                                        if game.notes_hit.iter().any(|h| h.note_index == i) {
                                            return None;
                                        }

                                        // Clamp to 4 lanes for current UI
                                        let lane = if chart.columns > 4 && n.col >= 4 {
                                            3
                                        } else {
                                            n.col.min(3)
                                        } as i32;

                                        Some(NoteData { time: n.time, fret: lane, duration: n.duration })
                                    }).collect();
                                    let model = std::rc::Rc::new(slint::VecModel::from(notes));
                                    ui.set_chart_notes(model.into());
                                }
                                did_hit = true;
                                break;
                            } else {
                                info!("Skipping already-hit note idx={} lane={}", i, note.col);
                            }
                        }

                        if !did_hit && candidates.is_empty() {
                            // When there are no candidates, log nearest note on this lane to understand why
                            let nearest = chart.notes.iter().enumerate()
                                .filter(|(_i, n)| n.col == event.lane)
                                .min_by(|a, b| ((a.1.time - game.current_time).abs()).partial_cmp(&((b.1.time - game.current_time).abs())).unwrap());

                            if let Some((ni, nn)) = nearest {
                                info!("No candidate notes for lane {} at {:.3}s; nearest idx={} time={:.3} dt={:.3}s", event.lane, game.current_time, ni, nn.time, nn.time - game.current_time);
                            } else {
                                info!("No candidate notes for lane {} at {:.3}s; no notes in chart for this lane", event.lane, game.current_time);
                            }
                        }
                    }
                }
            }
        }
    });
    
    // Set lane pressed callback (also release input state after visual unpress)
    ui.on_set_lane_pressed({
        let ui_weak = ui_weak.clone();
        let input_handler = input_handler.clone();
        let game_state_for_ui = game_state.clone();
        let chart_data_for_ui = chart_data.clone();
        move |lane| {
            if let Some(ui) = ui_weak.upgrade() {
                match lane {
                    0 => ui.set_lane_1_pressed(true),
                    1 => ui.set_lane_2_pressed(true),
                    2 => ui.set_lane_3_pressed(true),
                    3 => ui.set_lane_4_pressed(true),
                    _ => {}
                }

                // Schedule a timer to unpress after 100ms and release input state
                let ui_clone = ui_weak.clone();
                let input_clone = input_handler.clone();
                let game_state_clone = game_state_for_ui.clone();
                let chart_clone = chart_data_for_ui.clone();
                let timer = slint::Timer::default();
                timer.start(slint::TimerMode::SingleShot, Duration::from_millis(100), move || {
                    if let Some(ui) = ui_clone.upgrade() {
                        match lane {
                            0 => ui.set_lane_1_pressed(false),
                            1 => ui.set_lane_2_pressed(false),
                            2 => ui.set_lane_3_pressed(false),
                            3 => ui.set_lane_4_pressed(false),
                            _ => {}
                        }
                    }

                    // Also clear the input's pressed state for this lane's key and process a release-based tap
                    let key_to_release = match lane {
                        0 => input_clone.lock().unwrap().get_bindings().lane_1,
                        1 => input_clone.lock().unwrap().get_bindings().lane_2,
                        2 => input_clone.lock().unwrap().get_bindings().lane_3,
                        3 => input_clone.lock().unwrap().get_bindings().lane_4,
                        _ => '\0',
                    };

                    if key_to_release != '\0' {
                        let current_time = game_state_clone.lock().unwrap().current_time;
                        if let Some(event) = input_clone.lock().unwrap().handle_key_release(key_to_release, Some(current_time)) {
                            // Process the release event similarly to a press
                            if let Some(chart) = chart_clone.lock().unwrap().as_ref() {
                                let mut game = game_state_clone.lock().unwrap();
                                let hit_window = 0.3; // 300ms hit window

                                // Gather candidate notes in the hit window for this lane
                                let candidates: Vec<(usize, &game::ChartNote)> = chart.notes.iter().enumerate()
                                    .filter(|(_i, n)| n.col == event.lane && (n.time - game.current_time).abs() <= hit_window)
                                    .collect();

                                info!("Key release on lane {} at {:.3}s; {} candidate(s): {:?}; already hit count={}",
                                    event.lane,
                                    game.current_time,
                                    candidates.len(),
                                    candidates.iter().map(|(i, n)| (i, n.time)).collect::<Vec<_>>(),
                                    game.notes_hit.len()
                                );

                                let mut did_hit = false;
                                for (i_ref, note_ref) in &candidates {
                                    let i = *i_ref;
                                    let note = *note_ref;
                                    if !game.notes_hit.iter().any(|h| h.note_index == i) {
                                        let accuracy = game.record_hit(i, note, event.timestamp);

                                        info!("Hit {:?} on lane {} at time {} (release)", accuracy, event.lane, game.current_time);

                                        if let Some(ui) = ui_clone.upgrade() {
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

                                            // Update UI note model immediately to remove hit notes
                                            let notes: Vec<NoteData> = chart.notes.iter().enumerate().filter_map(|(i, n)| {
                                                // Skip notes that have been hit
                                                if game.notes_hit.iter().any(|h| h.note_index == i) {
                                                    return None;
                                                }

                                                // Clamp to 4 lanes for current UI
                                                let lane = if chart.columns > 4 && n.col >= 4 {
                                                    3
                                                } else {
                                                    n.col.min(3)
                                                } as i32;

                                                Some(NoteData { time: n.time, fret: lane, duration: n.duration })
                                            }).collect();
                                            let model = std::rc::Rc::new(slint::VecModel::from(notes));
                                            ui.set_chart_notes(model.into());
                                        }
                                        did_hit = true;
                                        break;
                                    } else {
                                        info!("Skipping already-hit note idx={} lane={}", i, note.col);
                                    }
                                }

                                if !did_hit && candidates.is_empty() {
                                    // When there are no candidates, log nearest note on this lane to understand why
                                    let nearest = chart.notes.iter().enumerate()
                                        .filter(|(_i, n)| n.col == event.lane)
                                        .min_by(|a, b| ((a.1.time - game.current_time).abs()).partial_cmp(&((b.1.time - game.current_time).abs())).unwrap());

                                    if let Some((ni, nn)) = nearest {
                                        info!("No candidate notes for lane {} at {:.3}s; nearest idx={} time={:.3} dt={:.3}s", event.lane, game.current_time, ni, nn.time, nn.time - game.current_time);
                                    } else {
                                        info!("No candidate notes for lane {} at {:.3}s; no notes in chart for this lane", event.lane, game.current_time);
                                    }
                                }
                            }
                        }
                    }
                });
            }
        }
    });
    
    // Focus key input callback
    ui.on_focus_key_input({
        move || {
            eprintln!("Attempting to focus key input");
        }
    });
    
    // Load songs on startup
    ui.invoke_load_songs();
    
    info!("UI initialized successfully");
    
    // Start keyboard listener in a separate thread
    let ui_weak_kb = ui_weak.clone();
    let input_handler_kb = input_handler.clone();
    let game_state_kb = game_state.clone();
    let chart_data_kb = chart_data.clone();
    let game_running_kb = game_running.clone();
    
    thread::spawn(move || {
        if let Err(e) = listen(move |event: Event| {
            match event.event_type {
                EventType::KeyPress(key) => {
                    let key_char = match key {
                        Key::KeyD => Some('D'),
                        Key::KeyF => Some('F'),
                        Key::KeyJ => Some('J'),
                        Key::KeyK => Some('K'),
                        _ => None,
                    };

                    if let Some(ch) = key_char {
                        // Only process if game is running
                        if *game_running_kb.lock().unwrap() {
                            //eprintln!("=== KEY PRESSED: {} ===", ch);

                            let mut input = input_handler_kb.lock().unwrap();
                            let mut game = game_state_kb.lock().unwrap();

                            if let Some(event) = input.handle_key_press(ch, game.current_time) {
                                //eprintln!("Key mapped to lane {}", event.lane);

                                // Show whether input state believes the lane is pressed
                                info!("KB Input state lane {} pressed?: {}", event.lane, input.is_lane_pressed(event.lane));

                                // Highlight the key button
                                if let Some(ui) = ui_weak_kb.upgrade() {
                                    match event.lane {
                                        0 => ui.invoke_set_lane_pressed(0),
                                        1 => ui.invoke_set_lane_pressed(1),
                                        2 => ui.invoke_set_lane_pressed(2),
                                        3 => ui.invoke_set_lane_pressed(3),
                                        _ => {}
                                    }
                                }

                                // Check for nearby notes in the chart
                                if let Some(chart) = chart_data_kb.lock().unwrap().as_ref() {
                                    let hit_window = 0.3;

                                    // Gather candidate notes in the hit window for this lane and log them
                                    let candidates_kb: Vec<(usize, &game::ChartNote)> = chart.notes.iter().enumerate()
                                        .filter(|(_i, n)| n.col == event.lane && (n.time - game.current_time).abs() <= hit_window)
                                        .collect();

                                    info!("KB key press on lane {} at {:.3}s; {} candidate(s): {:?}; already hit count={}",
                                        event.lane,
                                        game.current_time,
                                        candidates_kb.len(),
                                        candidates_kb.iter().map(|(i, n)| (i, n.time)).collect::<Vec<_>>(),
                                        game.notes_hit.len()
                                    );

                                    let mut did_hit = false;
                                    for (i_ref, note_ref) in &candidates_kb {
                                        let i = *i_ref;
                                        let note = *note_ref;
                                        if !game.notes_hit.iter().any(|h| h.note_index == i) {
                                            let _accuracy = game.record_hit(i, note, event.timestamp);
                                            //eprintln!("Hit {:?} on lane {} at time {}", _accuracy, event.lane, game.current_time);
                                            if let Some(ui) = ui_weak_kb.upgrade() {
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

                                                // Update UI note model immediately to remove hit notes
                                                let notes: Vec<NoteData> = chart.notes.iter().enumerate().filter_map(|(i, n)| {
                                                    // Skip notes that have been hit
                                                    if game.notes_hit.iter().any(|h| h.note_index == i) {
                                                        return None;
                                                    }

                                                    // Clamp to 4 lanes for current UI
                                                    let lane = if chart.columns > 4 && n.col >= 4 {
                                                        3
                                                    } else {
                                                        n.col.min(3)
                                                    } as i32;

                                                    Some(NoteData { time: n.time, fret: lane, duration: n.duration })
                                                }).collect();
                                                let model = std::rc::Rc::new(slint::VecModel::from(notes));
                                                ui.set_chart_notes(model.into());
                                            }
                                            did_hit = true;
                                            break;
                                        } else {
                                            info!("Skipping already-hit note idx={} lane={}", i, note.col);
                                        }
                                    }

                                    if !did_hit && candidates_kb.is_empty() {
                                        // Log nearest note on this lane to understand why
                                        let nearest = chart.notes.iter().enumerate()
                                            .filter(|(_i, n)| n.col == event.lane)
                                            .min_by(|a, b| ((a.1.time - game.current_time).abs()).partial_cmp(&((b.1.time - game.current_time).abs())).unwrap());

                                        if let Some((ni, nn)) = nearest {
                                            info!("No candidate notes for lane {} at {:.3}s; nearest idx={} time={:.3} dt={:.3}s", event.lane, game.current_time, ni, nn.time, nn.time - game.current_time);
                                        } else {
                                            info!("No candidate notes for lane {} at {:.3}s; no notes in chart for this lane", event.lane, game.current_time);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                EventType::KeyRelease(key) => {
                    let key_char = match key {
                        Key::KeyD => Some('D'),
                        Key::KeyF => Some('F'),
                        Key::KeyJ => Some('J'),
                        Key::KeyK => Some('K'),
                        _ => None,
                    };

                    if let Some(ch) = key_char {
                        // Clear pressed state so subsequent presses are processed
                        if *game_running_kb.lock().unwrap() {
                            info!("KB key release received for '{}'", ch);
                            let current_time = game_state_kb.lock().unwrap().current_time;
                            if let Some(event) = input_handler_kb.lock().unwrap().handle_key_release(ch, Some(current_time)) {
                                // Process the release event similarly to a press
                                if let Some(chart) = chart_data_kb.lock().unwrap().as_ref() {
                                    let mut game = game_state_kb.lock().unwrap();
                                    let hit_window = 0.3; // 300ms hit window

                                    // Gather candidate notes in the hit window for this lane
                                    let candidates: Vec<(usize, &game::ChartNote)> = chart.notes.iter().enumerate()
                                        .filter(|(_i, n)| n.col == event.lane && (n.time - game.current_time).abs() <= hit_window)
                                        .collect();

                                    info!("Key release on lane {} at {:.3}s; {} candidate(s): {:?}; already hit count={}",
                                        event.lane,
                                        game.current_time,
                                        candidates.len(),
                                        candidates.iter().map(|(i, n)| (i, n.time)).collect::<Vec<_>>(),
                                        game.notes_hit.len()
                                    );

                                    let mut did_hit = false;
                                    for (i_ref, note_ref) in &candidates {
                                        let i = *i_ref;
                                        let note = *note_ref;
                                        if !game.notes_hit.iter().any(|h| h.note_index == i) {
                                            let accuracy = game.record_hit(i, note, event.timestamp);

                                            info!("Hit {:?} on lane {} at time {} (release)", accuracy, event.lane, game.current_time);

                                            if let Some(ui) = ui_weak_kb.upgrade() {
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

                                                // Update UI note model immediately to remove hit notes
                                                let notes: Vec<NoteData> = chart.notes.iter().enumerate().filter_map(|(i, n)| {
                                                    // Skip notes that have been hit
                                                    if game.notes_hit.iter().any(|h| h.note_index == i) {
                                                        return None;
                                                    }

                                                    // Clamp to 4 lanes for current UI
                                                    let lane = if chart.columns > 4 && n.col >= 4 {
                                                        3
                                                    } else {
                                                        n.col.min(3)
                                                    } as i32;

                                                    Some(NoteData { time: n.time, fret: lane, duration: n.duration })
                                                }).collect();
                                                let model = std::rc::Rc::new(slint::VecModel::from(notes));
                                                ui.set_chart_notes(model.into());
                                            }
                                            did_hit = true;
                                            break;
                                        } else {
                                            info!("Skipping already-hit note idx={} lane={}", i, note.col);
                                        }
                                    }

                                    if !did_hit && candidates.is_empty() {
                                        // Log nearest note on this lane to understand why
                                        let nearest = chart.notes.iter().enumerate()
                                            .filter(|(_i, n)| n.col == event.lane)
                                            .min_by(|a, b| ((a.1.time - game.current_time).abs()).partial_cmp(&((b.1.time - game.current_time).abs())).unwrap());

                                        if let Some((ni, nn)) = nearest {
                                            info!("No candidate notes for lane {} at {:.3}s; nearest idx={} time={:.3} dt={:.3}s", event.lane, game.current_time, ni, nn.time, nn.time - game.current_time);
                                        } else {
                                            info!("No candidate notes for lane {} at {:.3}s; no notes in chart for this lane", event.lane, game.current_time);
                                        }
                                    }
                                }
                            }

                            // Log resulting lane state
                            let lane = input_handler_kb.lock().unwrap().get_bindings().key_to_lane(ch).unwrap_or(99);
                            info!("KB Input state lane {} pressed?: {}", lane, input_handler_kb.lock().unwrap().is_lane_pressed(lane));
                        }
                    }
                }
                _ => {}
            }
        }) {
            eprintln!("Error listening to keyboard: {:?}", e);
        }
    });
    
    // Run the UI (this blocks until the window is closed)
    ui.run()?;
    
    Ok(())
}
