use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use chrono::Utc;

#[derive(Deserialize, Serialize, Debug)]
pub struct Chart {
    pub song_id: String,
    pub instrument: String,
    pub difficulty: String,
    pub columns: u8,
    pub generated_at: i64,
    pub notes: Vec<serde_json::Value>,
}

pub fn generate_charts_for_song(song_id: &str, _songs_dir: &Path, charts_dir: &Path) -> Result<Vec<PathBuf>> {
    // Ensure output dir exists
    fs::create_dir_all(charts_dir)?;

    let instruments = ["vocal", "bass", "lead", "drums"];
    let difficulties = ["Easy", "Normal", "Hard"];

    let mut written = Vec::new();

    for instr in instruments.iter() {
        for diff in difficulties.iter() {
            let columns = if *diff == "Hard" { 5 } else { 4 };
            let chart = Chart {
                song_id: song_id.to_string(),
                instrument: instr.to_string(),
                difficulty: diff.to_string(),
                columns,
                generated_at: Utc::now().timestamp(),
                notes: vec![],
            };

            let fname = format!("{}_{instr}_{diff}.chart.json", song_id);
            let dest = charts_dir.join(fname);
            if dest.exists() {
                // don't overwrite existing charts
                written.push(dest);
                continue;
            }
            let s = serde_json::to_string_pretty(&chart)?;
            fs::write(&dest, s)?;
            written.push(dest);
        }
    }

    Ok(written)
}
