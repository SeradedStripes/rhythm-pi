use anyhow::Result;
use sqlx::SqlitePool;
use std::path::Path;
use std::time::Duration;
use tokio::time::sleep;
use crate::chart_gen;
use crate::db;

pub async fn start_watcher(pool: SqlitePool) {
    // run a quick scan at startup then schedule periodic scans
    if let Err(e) = scan_once(&pool).await {
        log::warn!("initial scan failed: {}", e);
    }

    loop {
        sleep(Duration::from_secs(60 * 5)).await; // 5 minutes
        if let Err(e) = scan_once(&pool).await {
            log::warn!("periodic scan failed: {}", e);
        }
    }
}

pub async fn scan_once(pool: &SqlitePool) -> Result<()> {
    let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
    let charts_dir = std::env::var("CHARTS_DIR").unwrap_or_else(|_| "server/assets/charts".to_string());

    let mut entries = std::fs::read_dir(&songs_dir)?.filter_map(|r| r.ok()).collect::<Vec<_>>();
    // process json metadata files first
    entries.sort_by_key(|e| e.file_name());

    for e in entries {
        let fname = e.file_name();
        let name = fname.to_string_lossy().to_string();
        if !name.ends_with(".json") {
            continue;
        }
        let json_path = e.path();
        let song_id = json_path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();
        if song_id.is_empty() { continue; }

        // get mtime
        let _ = std::fs::metadata(&json_path)?;
        // store simple timestamp for now
        let mtime_ts = chrono::Utc::now().timestamp();

        // read metadata
        let content = std::fs::read_to_string(&json_path)?;
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap_or(serde_json::json!({}));
        let title = parsed.get("title").and_then(|v| v.as_str());
        let artist = parsed.get("artist").and_then(|v| v.as_str());

        // upsert to db
        db::upsert_song(&pool, &song_id, &name, title, artist, mtime_ts).await?;

        // ensure charts exist; generate if none or if existing charts look empty/too small
        let mut have_chart = false;
        let mut need_regen = false;
        if let Ok(ch_entries) = std::fs::read_dir(&charts_dir) {
            for ch in ch_entries.filter_map(|r| r.ok()) {
                if let Some(fname) = ch.file_name().to_str() {
                    if fname.starts_with(&song_id) {
                        have_chart = true;
                        // inspect for empty or tiny note arrays
                        if let Ok(txt) = std::fs::read_to_string(ch.path()) {
                            if let Ok(jsonv) = serde_json::from_str::<serde_json::Value>(&txt) {
                                if let Some(notes) = jsonv.get("notes") {
                                    if notes.is_array() {
                                        let n = notes.as_array().unwrap().len();
                                        if n == 0 || n < 8 {
                                            log::info!("chart {} has {} notes; will regenerate", fname, n);
                                            need_regen = true;
                                            break;
                                        }
                                    } else {
                                        need_regen = true;
                                        break;
                                    }
                                } else {
                                    need_regen = true;
                                    break;
                                }
                            } else {
                                need_regen = true;
                                break;
                            }
                        } else {
                            need_regen = true;
                            break;
                        }
                    }
                }
            }
        }

        if !have_chart || need_regen {
            if need_regen {
                log::info!("Detected empty/insufficient charts for {}; regenerating (force)", song_id);
            }
            // prefer Rust HQ generator when RUST_HQ env var is true (default true)
            let rust_hq = std::env::var("RUST_HQ").map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(true);
            if rust_hq {
                match tokio::task::spawn_blocking({
                    let sid = song_id.clone();
                    let charts_dir = charts_dir.clone();
                    let wav = json_path.with_extension("wav");
                    move || {
                        crate::hq_rust::generate_hq_charts_rust(&sid, &wav, std::path::Path::new(&charts_dir), true)
                    }
                }).await {
                    Ok(Ok(written)) => {
                        let cnt = written.len();
                        log::info!("Rust HQ generated {} charts for {}", cnt, song_id);
                    }
                    Ok(Err(e)) => {
                        log::warn!("Rust HQ generation failed: {}, falling back to simple generator", e);
                        let written = chart_gen::generate_charts_for_song(&song_id, Path::new(&songs_dir), Path::new(&charts_dir))?;
                        log::info!("generated {} charts for {}", written.len(), song_id);
                    }
                    Err(e) => {
                        log::warn!("Rust HQ task join failed: {}, falling back", e);
                        let written = chart_gen::generate_charts_for_song(&song_id, Path::new(&songs_dir), Path::new(&charts_dir))?;
                        log::info!("generated {} charts for {}", written.len(), song_id);
                    }
                }
            } else {
                let written = chart_gen::generate_charts_for_song(&song_id, Path::new(&songs_dir), Path::new(&charts_dir))?;
                log::info!("generated {} charts for {}", written.len(), song_id);
            }
        }
    }

    Ok(())
}
