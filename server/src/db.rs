use anyhow::Result;
use sqlx::{SqlitePool, Row};

pub async fn init_db(pool: &SqlitePool) -> Result<()> {
    // simple scores table. Only scores with online = 1 are considered for leaderboards
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS scores (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            song_id TEXT NOT NULL,
            player TEXT NOT NULL,
            score INTEGER NOT NULL,
            timestamp INTEGER NOT NULL,
            online INTEGER NOT NULL
        );"#,
    )
    .execute(pool)
    .await?;

    // users table for simple auth
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL
        );"#,
    )
    .execute(pool)
    .await?;

    // songs table: store registered songs and file mtimes so we can detect changes
    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS songs (
            id TEXT PRIMARY KEY,
            filename TEXT NOT NULL,
            title TEXT,
            artist TEXT,
            mtime INTEGER,
            registered_at INTEGER NOT NULL
        );"#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn upsert_song(
    pool: &SqlitePool,
    id: &str,
    filename: &str,
    title: Option<&str>,
    artist: Option<&str>,
    mtime: i64,
) -> Result<()> {
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO songs (id, filename, title, artist, mtime, registered_at) VALUES (?, ?, ?, ?, ?, ?) ON CONFLICT(id) DO UPDATE SET filename = excluded.filename, title = excluded.title, artist = excluded.artist, mtime = excluded.mtime"
    )
    .bind(id)
    .bind(filename)
    .bind(title)
    .bind(artist)
    .bind(mtime)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_songs_db(pool: &SqlitePool) -> Result<Vec<(String, String)>> {
    let rows = sqlx::query("SELECT id, filename FROM songs ORDER BY registered_at DESC")
        .fetch_all(pool)
        .await?;
    let mut v = Vec::new();
    for r in rows {
        let id: String = r.try_get("id")?;
        let filename: String = r.try_get("filename")?;
        v.push((id, filename));
    }
    Ok(v)
}

pub async fn insert_score(
    pool: &SqlitePool,
    song_id: &str,
    player: &str,
    score: i64,
    timestamp: i64,
    online: bool,
) -> Result<()> {
    sqlx::query("INSERT INTO scores (song_id, player, score, timestamp, online) VALUES (?, ?, ?, ?, ?)")
        .bind(song_id)
        .bind(player)
        .bind(score)
        .bind(timestamp)
        .bind(if online { 1 } else { 0 })
        .execute(pool)
        .await?;

    Ok(())
}

pub async fn top_scores(pool: &SqlitePool, song_id: &str, limit: i64) -> Result<Vec<(String, i64, i64)>> {
    let rows = sqlx::query("SELECT player, score, timestamp FROM scores WHERE song_id = ? AND online = 1 ORDER BY score DESC LIMIT ?")
        .bind(song_id)
        .bind(limit)
        .fetch_all(pool)
        .await?;

    let mut v = Vec::new();
    for r in rows {
        let player: String = r.try_get("player")?;
        let score: i64 = r.try_get("score")?;
        let timestamp: i64 = r.try_get("timestamp")?;
        v.push((player, score, timestamp));
    }

    Ok(v)
}

pub async fn create_user(pool: &SqlitePool, username: &str, password_hash: &str) -> Result<()> {
    sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
        .bind(username)
        .bind(password_hash)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn get_password_hash(pool: &SqlitePool, username: &str) -> Result<Option<String>> {
    let row = sqlx::query("SELECT password_hash FROM users WHERE username = ?")
        .bind(username)
        .fetch_optional(pool)
        .await?;

    if let Some(r) = row {
        let hash: String = r.try_get("password_hash")?;
        Ok(Some(hash))
    } else {
        Ok(None)
    }
}
