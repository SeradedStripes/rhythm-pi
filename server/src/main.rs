use actix_web::{App, HttpServer, middleware::Logger};
use actix_cors::Cors;

use rhythm_pi_server::{db, handlers, song_watcher, websocket};
use sqlx::SqlitePool;
use env_logger::Env;
use std::path::Path;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    // init logging
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    // load .env if present (so env overrides work)
    let _ = dotenvy::dotenv();

    // database URL (supports sqlite:<path> or sqlite::memory:)
    let db_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite:server/data/rhythm.db".to_string());

    // if using sqlite file, ensure parent dir exists and create an empty db file if necessary
    if db_url.starts_with("sqlite:") {
        if let Some(path) = db_url.strip_prefix("sqlite:") {
            // don't touch :memory:
            if !path.is_empty() && path != ":memory:" {
                let p = Path::new(path);
                if let Some(parent) = p.parent() {
                    if !parent.exists() {
                        std::fs::create_dir_all(parent).expect("failed to create db parent dir");
                    }
                }

                // ensure file exists (touch). Do not truncate if exists.
                if !p.exists() {
                    std::fs::OpenOptions::new()
                        .create(true)
                        .write(true)
                        .open(p)
                        .expect("failed to create sqlite db file");
                    log::info!("created sqlite db file at {}", path);
                }
            }
        }
    }

    let pool = SqlitePool::connect(&db_url).await.expect("failed to connect to sqlite DB");
    db::init_db(&pool).await.expect("failed to init db");

    // spawn background watcher task to detect new/changed songs every 5 minutes
    let pool_clone = pool.clone();
    tokio::spawn(async move {
        crate::song_watcher::start_watcher(pool_clone).await;
    });

    let addr = ("0.0.0.0", 8080);
    log::info!("Starting server on {}:{}", addr.0, addr.1);

    HttpServer::new(move || {
        App::new()
            // allow requests from local clients (Tauri / file:// / tauri://) during development
            .wrap(Cors::permissive())
            .wrap(Logger::default())
            .app_data(actix_web::web::Data::new(pool.clone()))
            .service(
                actix_web::web::scope("/api")
                    .route("/songs", actix_web::web::get().to(handlers::list_songs))
                    .route("/songs/{id}/stream", actix_web::web::get().to(handlers::stream_song))
                    .route("/songs/{id}/chart", actix_web::web::get().to(handlers::get_chart))
                    .route("/leaderboard/{song_id}", actix_web::web::get().to(handlers::get_leaderboard))
                    .route("/scores", actix_web::web::post().to(handlers::post_score))
                    .route("/register", actix_web::web::post().to(handlers::register_user))
                    .route("/login", actix_web::web::post().to(handlers::login_user))
                    // admin: trigger immediate scan (helpful for dev/testing)
                    .route("/admin/scan", actix_web::web::post().to(handlers::admin_scan))
                    .route("/admin/generate_hq/{song_id}", actix_web::web::post().to(handlers::admin_generate_hq))
            )
            // WebSocket endpoint for audio streaming
            .route("/ws/audio/{song_id}", actix_web::web::get().to(websocket::ws_audio_stream))
            // expose raw static files for downloads
            .service(actix_files::Files::new("/files/songs", "server/assets/songs").show_files_listing())
            .service(actix_files::Files::new("/files/charts", "server/assets/charts").show_files_listing())
    })
    .bind(addr)?
    .run()
    .await
}
