use crate::beat_detection::Note;

#[derive(Clone, Debug)]
pub struct Quantizer {
    pub bpm: f32,
    pub sample_rate: u32,
    pub grid_division: u8, // 4 = sixteenth notes, 8 = thirty-second notes, etc.
}

impl Quantizer {
    pub fn new(bpm: f32, sample_rate: u32, grid_division: u8) -> Self {
        Quantizer {
            bpm,
            sample_rate,
            grid_division,
        }
    }

    /// Get the time of a grid position (beat + subdivision)
    pub fn grid_time(&self, beat: f32, subdivision: u8) -> f32 {
        let beat_duration = 60.0 / self.bpm;
        let subdivision_duration = beat_duration / self.grid_division as f32;
        beat * beat_duration + subdivision as f32 * subdivision_duration
    }

    /// Quantize a time to the nearest grid point
    pub fn quantize(&self, time: f32) -> (f32, u8) {
        let beat_duration = 60.0 / self.bpm;
        let subdivision_duration = beat_duration / self.grid_division as f32;

        let total_grid_points = (time / subdivision_duration).round();
        let beat = (total_grid_points / self.grid_division as f32).floor();
        let subdivision = (total_grid_points % self.grid_division as f32) as u8;

        let quantized_time = self.grid_time(beat, subdivision);
        (quantized_time, subdivision)
    }

    /// Quantize multiple notes and remove duplicates
    pub fn quantize_notes(&self, mut notes: Vec<Note>) -> Vec<Note> {
        for note in &mut notes {
            note.time = self.quantize(note.time).0;
        }

        // Sort by time
        notes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

        // Remove near-duplicate notes (within 10ms)
        let mut deduped = Vec::new();
        let mut last_time = -1.0f32;
        
        for note in notes {
            if (note.time - last_time).abs() > 0.01 {
                last_time = note.time;
                deduped.push(note);
            }
        }

        deduped
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_grid_time_calculation() {
        let quantizer = Quantizer::new(120.0, 44100, 4);
        // Beat duration at 120 BPM = 0.5 seconds
        // Subdivision duration = 0.125 seconds
        let time = quantizer.grid_time(0.0, 0);
        assert!((time - 0.0).abs() < 0.001);
        
        let time = quantizer.grid_time(1.0, 0);
        assert!((time - 0.5).abs() < 0.001);
        
        let time = quantizer.grid_time(0.0, 1);
        assert!((time - 0.125).abs() < 0.001);
    }

    #[test]
    fn test_quantize_time() {
        let quantizer = Quantizer::new(120.0, 44100, 4);
        
        // Time slightly off from grid should snap to nearest grid point
        let (quantized, _) = quantizer.quantize(0.06);
        assert!((quantized - 0.0625).abs() < 0.001);
    }
}
