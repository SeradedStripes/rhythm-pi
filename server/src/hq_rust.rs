use anyhow::{Result, bail};
use std::path::{Path, PathBuf};

/// HQ chart generation disabled in favor of external tooling (e.g., Moonscraper).
pub fn generate_hq_charts_rust(song_id: &str, song_file: &Path, charts_dir: &Path, force: bool) -> Result<Vec<PathBuf>> {
    let _ = (song_id, song_file, charts_dir, force);
    bail!("HQ chart generation disabled (use external tool)")
}
