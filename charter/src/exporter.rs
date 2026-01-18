use crate::beat_detection::Note;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChartExport {
    pub song_id: String,
    pub instrument: String,
    pub difficulty: String,
    pub columns: u8,
    pub bpm: f32,
    pub generated_at: i64,
    pub notes: Vec<NoteExport>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct NoteExport {
    pub time: f32,
    pub col: u8,
    #[serde(skip_serializing_if = "is_zero")]
    pub duration: f32,
}

fn is_zero(n: &f32) -> bool {
    *n == 0.0 || *n < 0.001
}

impl ChartExport {
    pub fn new(
        song_id: String,
        instrument: String,
        difficulty: String,
        columns: u8,
        bpm: f32,
        notes: Vec<Note>,
    ) -> Self {
        let generated_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);

        let notes = notes
            .into_iter()
            .map(|n| NoteExport {
                time: n.time,
                col: n.col,
                duration: n.duration,
            })
            .collect();

        ChartExport {
            song_id,
            instrument,
            difficulty,
            columns,
            bpm,
            generated_at,
            notes,
        }
    }

    /// Export to JSON format
    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string_pretty(&self)?)
    }

    /// Export to custom .chart format (text-based)
    pub fn to_chart(&self) -> String {
        let mut output = String::new();
        
        output.push_str(&format!("[SONG]\n"));
        output.push_str(&format!("  Title = \"{}\"\n", self.song_id));
        output.push_str(&format!("  Artist = \"\"\n"));
        output.push_str(&format!("  BPM = {}\n", self.bpm));
        output.push_str(&format!("  Gap = 0\n\n"));

        output.push_str(&format!("[NOTES]\n"));
        output.push_str(&format!("  Instrument = {}\n", self.instrument));
        output.push_str(&format!("  Difficulty = {}\n", self.difficulty));
        output.push_str(&format!("  Columns = {}\n", self.columns));
        output.push_str(&format!("  Notes = {}\n", self.notes.len()));
        output.push_str(":\n");

        for note in &self.notes {
            let note_type = if note.duration > 0.001 {
                '2' // Hold note
            } else {
                '1' // Tap note
            };

            output.push_str(&format!(
                "  {}|{:.3}|{:.3}\n",
                note_type, note.col, note.time
            ));

            if note.duration > 0.001 {
                output.push_str(&format!("  2|{}|{:.3}\n", note.col, note.duration));
            }
        }

        output.push_str(";\n");
        output
    }

    /// Save chart to file
    pub fn save(&self, path: &Path, format: ChartFormat) -> Result<()> {
        let content = match format {
            ChartFormat::Json => self.to_json()?,
            ChartFormat::Chart => self.to_chart(),
        };

        std::fs::write(path, content)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug)]
pub enum ChartFormat {
    Json,
    Chart,
}

impl ChartFormat {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "json" => Some(ChartFormat::Json),
            "chart" => Some(ChartFormat::Chart),
            _ => None,
        }
    }

    pub fn extension(&self) -> &'static str {
        match self {
            ChartFormat::Json => "json",
            ChartFormat::Chart => "chart",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_export_json() {
        let notes = vec![Note {
            time: 0.5,
            col: 2,
            duration: 0.0,
        }];

        let chart = ChartExport::new(
            "test_song".to_string(),
            "vocals".to_string(),
            "Easy".to_string(),
            4,
            120.0,
            notes,
        );

        let json = chart.to_json().unwrap();
        assert!(json.contains("\"time\": 0.5"));
        assert!(json.contains("\"col\": 2"));
    }

    #[test]
    fn test_chart_export_chart_format() {
        let notes = vec![Note {
            time: 0.5,
            col: 2,
            duration: 0.25,
        }];

        let chart = ChartExport::new(
            "test_song".to_string(),
            "vocals".to_string(),
            "Easy".to_string(),
            4,
            120.0,
            notes,
        );

        let chart_text = chart.to_chart();
        assert!(chart_text.contains("BPM = 120"));
        assert!(chart_text.contains("Difficulty = Easy"));
        assert!(chart_text.contains("Columns = 4"));
    }

    #[test]
    fn test_chart_format_detection() {
        assert_eq!(ChartFormat::from_str("json").unwrap().extension(), "json");
        assert_eq!(ChartFormat::from_str("chart").unwrap().extension(), "chart");
        assert!(ChartFormat::from_str("invalid").is_none());
    }
}
