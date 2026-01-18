use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use serde_json;
use std::path::{Path, PathBuf};

#[derive(Deserialize, Serialize, Debug)]
pub struct Chart {
    pub song_id: String,
    pub instrument: String,
    pub difficulty: String,
    pub columns: u8,
    pub generated_at: i64,
    pub notes: Vec<serde_json::Value>,
}

pub fn generate_charts_for_song(song_id: &str, songs_dir: &Path, charts_dir: &Path) -> Result<Vec<PathBuf>> {
    let _ = (song_id, songs_dir, charts_dir);
    bail!("chart generation disabled (use external tool)")
}
