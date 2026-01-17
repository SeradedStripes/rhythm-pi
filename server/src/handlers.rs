use actix_files::NamedFile;
use actix_web::{web, HttpResponse, Result, HttpRequest};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::{PathBuf, Path};

use crate::db;

#[derive(Serialize)]
struct SongInfo {
    id: String,
    filename: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    artists: Option<Vec<String>>,
}

fn read_song_meta(songs_dir: &str, id: &str) -> (Option<String>, Option<Vec<String>>) {
    let mut jp = PathBuf::from(songs_dir);
    jp.push(format!("{}.json", id));
    if jp.exists() {
        if let Ok(s) = std::fs::read_to_string(&jp) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                let title = v.get("SongTitle").and_then(|t| t.as_str()).map(|s| s.to_string());
                let artists = v.get("Artists").and_then(|a| a.as_array()).map(|arr| {
                    arr.iter().filter_map(|it| it.as_str().map(|s| s.to_string())).collect()
                });
                return (title, artists);
            }
        }
    }
    (None, None)
}

pub async fn list_songs(pool: web::Data<SqlitePool>) -> Result<HttpResponse> {
    // prefer DB-registered songs
    let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
    match db::list_songs_db(&pool).await {
        Ok(rows) => {
            let mut songs = Vec::new();
            for (id, filename) in rows {
                let (title, artists) = read_song_meta(&songs_dir, &id);
                songs.push(SongInfo { id, filename, title, artists });
            }
            if songs.is_empty() {
                // fallback to directory listing for compatibility
                let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
                let mut fallback = Vec::new();
                if let Ok(entries) = std::fs::read_dir(&songs_dir) {
                    use std::collections::HashSet;
                    let mut seen = HashSet::new();
                    for e in entries.filter_map(|r| r.ok()) {
                        if let Some(name) = e.file_name().to_str() {
                            // skip non-audio and metadata files
                            if name.ends_with(".json") { continue; }
                            if !(name.ends_with(".mp3") || name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".flac")) { continue; }
                            let id = name.split('.').next().unwrap_or(name).to_string();
                            if seen.contains(&id) { continue; }
                            seen.insert(id.clone());
                            let (title, artists) = read_song_meta(&songs_dir, &id);
                            fallback.push(SongInfo { id, filename: name.to_string(), title, artists });
                        }
                    }
                }
                return Ok(HttpResponse::Ok().json(fallback));
            }
            Ok(HttpResponse::Ok().json(songs))
        }
        Err(_) => {
            // fallback to directory listing
            let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
            let mut songs = Vec::new();
            if let Ok(entries) = std::fs::read_dir(&songs_dir) {
                    use std::collections::HashSet;
                    let mut seen = HashSet::new();
                    for e in entries.filter_map(|r| r.ok()) {
                        if let Some(name) = e.file_name().to_str() {
                            if name.ends_with(".json") { continue; }
                            if !(name.ends_with(".mp3") || name.ends_with(".wav") || name.ends_with(".ogg") || name.ends_with(".flac")) { continue; }
                            let id = name.split('.').next().unwrap_or(name).to_string();
                            if seen.contains(&id) { continue; }
                            seen.insert(id.clone());
                            let (title, artists) = read_song_meta(&songs_dir, &id);
                            songs.push(SongInfo { id, filename: name.to_string(), title, artists });
                        }
                    }
                }
            Ok(HttpResponse::Ok().json(songs))
        }
    }
}

pub async fn stream_song(req: HttpRequest, path: web::Path<String>) -> Result<HttpResponse> {
    let id = path.into_inner();
    let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
    let mut fpath = PathBuf::from(&songs_dir);
    
    // try to find file that starts with id
    if let Ok(entries) = std::fs::read_dir(&songs_dir) {
        for e in entries.filter_map(|r| r.ok()) {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with(&id) {
                    fpath.push(name);
                    let named = NamedFile::open(fpath.clone())?;
                    
                    // Log the file being served
                    log::info!("Streaming audio file: {:?}", fpath);
                    
                    // Build response with proper headers
                    let mut response = named.into_response(&req);
                    
                    // Ensure proper content-type based on file extension
                    if let Some(ext) = fpath.extension() {
                        let ext_str = ext.to_string_lossy().to_lowercase();
                        let content_type = match ext_str.as_str() {
                            "mp3" => "audio/mpeg",
                            "wav" => "audio/wav",
                            "ogg" => "audio/ogg",
                            "flac" => "audio/flac",
                            _ => "application/octet-stream",
                        };
                        response.headers_mut().insert(
                            actix_web::http::header::CONTENT_TYPE,
                            actix_web::http::header::HeaderValue::from_static(content_type),
                        );
                    }
                    
                    // Add CORS headers for browser audio
                    response.headers_mut().insert(
                        actix_web::http::header::ACCESS_CONTROL_ALLOW_ORIGIN,
                        actix_web::http::header::HeaderValue::from_static("*"),
                    );
                    
                    return Ok(response);
                }
            }
        }
    }
    // not found
    Err(actix_web::error::ErrorNotFound("song not found"))
}

pub async fn get_chart(path: web::Path<String>, query: web::Query<std::collections::HashMap<String, String>>) -> Result<NamedFile> {
    let id = path.into_inner();
    let charts_dir = std::env::var("CHARTS_DIR").unwrap_or_else(|_| "server/assets/charts".to_string());

    // try instrument/difficulty exact name first: {id}_{instrument}_{difficulty}.chart.json
    if let Some(inst) = query.get("instrument") {
        if let Some(diff) = query.get("difficulty") {
            let fname = format!("{}_{}_{}.chart.json", id, inst, diff);
            let cand = PathBuf::from(&charts_dir).join(&fname);
            if cand.exists() {
                return Ok(NamedFile::open(cand)?);
            }
        }
    }

    // fallback to first file that starts with id
    let mut fpath = PathBuf::from(&charts_dir);
    if let Ok(entries) = std::fs::read_dir(&charts_dir) {
        for e in entries.filter_map(|r| r.ok()) {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with(&id) {
                    fpath.push(name);
                    return Ok(NamedFile::open(fpath)?);
                }
            }
        }
    }
    Err(actix_web::error::ErrorNotFound("chart not found"))
}

#[derive(Deserialize)]
pub struct ScoreSubmission {
    pub song_id: String,
    pub player: String,
    pub score: i64,
    pub timestamp: Option<i64>,
    pub online: bool,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

pub async fn register_user(
    pool: web::Data<SqlitePool>,
    payload: web::Json<RegisterRequest>,
) -> Result<HttpResponse> {
    // hash password
    let hash = crate::auth::hash_password(&payload.password).map_err(|e| {
        log::error!("hash error: {}", e);
        actix_web::error::ErrorInternalServerError("hash error")
    })?;

    // create user
    match db::create_user(&pool, &payload.username, &hash).await {
        Ok(_) => Ok(HttpResponse::Created().json(serde_json::json!({"status":"ok"}))),
        Err(e) => {
            log::error!("create user error: {}", e);
            // likely uniqueness constraint
            Err(actix_web::error::ErrorConflict("username taken"))
        }
    }
}

pub async fn login_user(
    pool: web::Data<SqlitePool>,
    payload: web::Json<LoginRequest>,
) -> Result<HttpResponse> {
    let maybe_hash = db::get_password_hash(&pool, &payload.username).await.map_err(|e| {
        log::error!("db error: {}", e);
        actix_web::error::ErrorInternalServerError("db error")
    })?;

    if let Some(hash) = maybe_hash {
        let ok = crate::auth::verify_password(&hash, &payload.password).map_err(|e| {
            log::error!("verify error: {}", e);
            actix_web::error::ErrorInternalServerError("verify error")
        })?;

        if ok {
            let token = crate::auth::create_token(&payload.username, 60 * 60 * 24 * 30).map_err(|e| {
                log::error!("token error: {}", e);
                actix_web::error::ErrorInternalServerError("token error")
            })?;

            return Ok(HttpResponse::Ok().json(serde_json::json!({"token": token})));
        }
    }

    Err(actix_web::error::ErrorUnauthorized("invalid credentials"))
}

pub async fn post_score(
    pool: web::Data<SqlitePool>,
    payload: web::Json<ScoreSubmission>,
    req: HttpRequest,
) -> Result<HttpResponse> {
    let ts = payload.timestamp.unwrap_or_else(|| chrono::Utc::now().timestamp());

    // if online submission, require auth
    if payload.online {
        if let Some(token) = crate::auth::extract_bearer(req.headers()) {
            match crate::auth::decode_token(&token) {
                Ok(username) => {
                    // replace player with username from token for trust
                    db::insert_score(
                        &pool,
                        &payload.song_id,
                        &username,
                        payload.score,
                        ts,
                        true,
                    )
                    .await
                    .map_err(|e| {
                        log::error!("db insert error: {}", e);
                        actix_web::error::ErrorInternalServerError("db error")
                    })?;

                    return Ok(HttpResponse::Ok()
                        .json(serde_json::json!({"status":"ok","message":"score recorded and eligible for leaderboard"})));
                }
                Err(e) => {
                    log::warn!("token decode error: {}", e);
                    return Err(actix_web::error::ErrorUnauthorized("invalid token"));
                }
            }
        } else {
            return Err(actix_web::error::ErrorUnauthorized("missing token"));
        }
    } else {
        // offline: accept any player name and store
        db::insert_score(
            &pool,
            &payload.song_id,
            &payload.player,
            payload.score,
            ts,
            false,
        )
        .await
        .map_err(|e| {
            log::error!("db insert error: {}", e);
            actix_web::error::ErrorInternalServerError("db error")
        })?;

        return Ok(HttpResponse::Accepted().json(serde_json::json!({"status":"accepted","message":"score recorded offline; not shown on leaderboard"})));
    }
}

pub async fn get_leaderboard(
    pool: web::Data<SqlitePool>,
    path: web::Path<String>,
) -> Result<HttpResponse> {
    let song_id = path.into_inner();
    let rows = db::top_scores(&pool, &song_id, 10).await.map_err(|e| {
        log::error!("db query error: {}", e);
        actix_web::error::ErrorInternalServerError("db error")
    })?;

    let mut resp = Vec::new();
    for (player, score, ts) in rows {
        resp.push(serde_json::json!({"player": player, "score": score, "timestamp": ts}));
    }

    Ok(HttpResponse::Ok().json(resp))
}

pub async fn admin_scan(pool: web::Data<SqlitePool>) -> Result<HttpResponse> {
    match crate::song_watcher::scan_once(&pool).await {
        Ok(_) => Ok(HttpResponse::Ok().json(serde_json::json!({"status":"ok","message":"scan completed"}))),
        Err(e) => {
            log::error!("manual scan failed: {}", e);
            Err(actix_web::error::ErrorInternalServerError("scan failed"))
        }
    }
}

pub async fn admin_generate_hq(
    _pool: web::Data<SqlitePool>,
    path: web::Path<String>,
    query: web::Query<std::collections::HashMap<String, String>>,
) -> Result<HttpResponse> {
    let song_id = path.into_inner();
    let force = query.get("force").map(|v| v == "1" || v.to_lowercase() == "true").unwrap_or(false);

    let songs_dir = std::env::var("SONGS_DIR").unwrap_or_else(|_| "server/assets/songs".to_string());
    let charts_dir = std::env::var("CHARTS_DIR").unwrap_or_else(|_| "server/assets/charts".to_string());
    let song_wav = PathBuf::from(&songs_dir).join(format!("{}.wav", song_id));

    if !song_wav.exists() {
        return Err(actix_web::error::ErrorNotFound("song file not found (wav required)"));
    }

    // call Rust HQ generator synchronously via spawn_blocking
    match tokio::task::spawn_blocking({
        let sid = song_id.clone();
        let charts_dir = charts_dir.clone();
        let wav = song_wav.clone();
        move || crate::hq_rust::generate_hq_charts_rust(&sid, &wav, Path::new(&charts_dir), force)
    }).await {
        Ok(Ok(written)) => {
            let generated: Vec<String> = written.into_iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<String>>();
            Ok(HttpResponse::Ok().json(serde_json::json!({"status":"ok","generated": generated })))
        }
        Ok(Err(e)) => {
            log::error!("HQ generation failed: {}", e);
            Err(actix_web::error::ErrorInternalServerError("hq generation failed"))
        }
        Err(e) => {
            log::error!("HQ task join failed: {}", e);
            Err(actix_web::error::ErrorInternalServerError("hq generation failed"))
        }
    }
}
