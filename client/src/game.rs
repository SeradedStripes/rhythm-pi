use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub bpm: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartNote {
    pub time: f32,
    #[serde(alias = "fret")]
    pub col: u32,
    #[serde(default)]
    pub duration: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chart {
    pub notes: Vec<ChartNote>,
    pub bpm: Option<f32>,
    pub offset: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub time: f32,
    pub lane: u32,
    pub duration: f32,
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub score: u32,
    pub combo: u32,
    pub health: f32,
    pub current_time: f32,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            score: 0,
            combo: 0,
            health: 100.0,
            current_time: 0.0,
        }
    }
}
