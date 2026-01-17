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

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitAccuracy {
    Perfect,   // ±50ms - 300 points
    Great,     // ±100ms - 200 points
    Good,      // ±150ms - 100 points
    Ok,        // ±200ms - 50 points
    Miss,      // Outside window - 0 points
}

impl HitAccuracy {
    pub fn points(&self) -> u32 {
        match self {
            Self::Perfect => 300,
            Self::Great => 200,
            Self::Good => 100,
            Self::Ok => 50,
            Self::Miss => 0,
        }
    }

    pub fn combo_multiplier(&self) -> f32 {
        match self {
            Self::Perfect => 2.0,
            Self::Great => 1.5,
            Self::Good => 1.0,
            Self::Ok => 0.5,
            Self::Miss => 0.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct HitEvent {
    pub note_time: f32,
    pub hit_time: f32,
    pub accuracy: HitAccuracy,
    pub note_lane: u32,
}

#[derive(Debug, Clone)]
pub struct GameState {
    pub score: u32,
    pub combo: u32,
    pub max_combo: u32,
    pub accuracy_count: AccuracyCounter,
    pub health: f32,
    pub current_time: f32,
    pub notes_hit: Vec<HitEvent>,
    pub is_playing: bool,
    pub is_paused: bool,
}

#[derive(Debug, Clone, Default)]
pub struct AccuracyCounter {
    pub perfect: u32,
    pub great: u32,
    pub good: u32,
    pub ok: u32,
    pub miss: u32,
}

impl GameState {
    pub fn new() -> Self {
        Self {
            score: 0,
            combo: 0,
            max_combo: 0,
            accuracy_count: AccuracyCounter::default(),
            health: 100.0,
            current_time: 0.0,
            notes_hit: Vec::new(),
            is_playing: false,
            is_paused: false,
        }
    }

    pub fn record_hit(&mut self, note: &ChartNote, hit_time: f32) -> HitAccuracy {
        let time_diff = (hit_time - note.time).abs();
        
        let accuracy = if time_diff <= 0.05 {
            HitAccuracy::Perfect
        } else if time_diff <= 0.1 {
            HitAccuracy::Great
        } else if time_diff <= 0.15 {
            HitAccuracy::Good
        } else if time_diff <= 0.2 {
            HitAccuracy::Ok
        } else {
            HitAccuracy::Miss
        };

        // Update accuracy counts
        match accuracy {
            HitAccuracy::Perfect => self.accuracy_count.perfect += 1,
            HitAccuracy::Great => self.accuracy_count.great += 1,
            HitAccuracy::Good => self.accuracy_count.good += 1,
            HitAccuracy::Ok => self.accuracy_count.ok += 1,
            HitAccuracy::Miss => {
                self.accuracy_count.miss += 1;
                self.combo = 0;
                self.health = (self.health - 5.0).max(0.0);
            }
        }

        // Award points
        let base_points = accuracy.points();
        let combo_bonus = (self.combo as f32 * 0.1).min(100.0); // Max 100 bonus points
        let total_points = (base_points as f32 + combo_bonus) as u32;

        self.score += total_points;

        // Update combo
        if accuracy != HitAccuracy::Miss {
            self.combo += 1;
            self.max_combo = self.max_combo.max(self.combo);
        }

        // Health recovery on good hits
        if accuracy != HitAccuracy::Miss {
            self.health = (self.health + 2.0).min(100.0);
        }

        self.notes_hit.push(HitEvent {
            note_time: note.time,
            hit_time,
            accuracy,
            note_lane: note.col,
        });

        accuracy
    }

    pub fn update(&mut self, delta_time: f32) {
        self.current_time += delta_time;
    }

    pub fn pause(&mut self) {
        self.is_paused = true;
    }

    pub fn resume(&mut self) {
        self.is_paused = false;
    }
}

impl Default for GameState {
    fn default() -> Self {
        Self::new()
    }
}
