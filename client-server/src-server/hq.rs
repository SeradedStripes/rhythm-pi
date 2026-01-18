use anyhow::{Result, Context};
use std::path::{Path, PathBuf};
use std::process::Command;

pub async fn generate_hq_charts(song_id: &str, song_file: &Path, charts_dir: &Path, force: bool) -> Result<Vec<PathBuf>> {
    // Locate python env: prefer env var HQ_PYTHON, otherwise system python
    let python = std::env::var("HQ_PYTHON").unwrap_or_else(|_| "python3".to_string());

    let script = Path::new("server/scripts/generate_hq.py");
    if !script.exists() {
        anyhow::bail!("HQ generator script not found: {}", script.display());
    }

    let mut cmd = Command::new(python);
    cmd.arg(script).arg("--input").arg(song_file).arg("--out").arg(charts_dir).arg("--song-id").arg(song_id);
    if force { cmd.arg("--force"); }

    log::info!("running HQ generator: {:?}", cmd);
    let out = cmd.output().context("failed to execute HQ python script")?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!("HQ generator failed: {}", stderr);
    }

    let stdout = String::from_utf8_lossy(&out.stdout);
    log::info!("HQ generator output: {}", stdout);

    // collect generated chart files for this song
    let mut written = Vec::new();
    if let Ok(entries) = std::fs::read_dir(charts_dir) {
        for e in entries.filter_map(|r| r.ok()) {
            if let Some(name) = e.file_name().to_str() {
                if name.starts_with(song_id) && name.ends_with(".chart.json") {
                    written.push(e.path());
                }
            }
        }
    }

    Ok(written)
}
