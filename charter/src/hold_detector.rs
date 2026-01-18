use crate::beat_detection::Note;

#[derive(Clone, Debug)]
pub struct HoldDetector {
    pub sustain_threshold: f32, // Energy threshold for detecting sustained notes
    pub min_hold_duration: f32, // Minimum hold duration in seconds (e.g., 0.25)
}

impl HoldDetector {
    pub fn new(sustain_threshold: f32, min_hold_duration: f32) -> Self {
        HoldDetector {
            sustain_threshold,
            min_hold_duration,
        }
    }

    /// Detect holds by analyzing sustained energy in specific frequency bands
    pub fn detect_holds(
        &self,
        notes: Vec<Note>,
        frequency_data: &[(f32, Vec<f32>)],
        lane_to_freq_range: &[(u8, f32, f32)],
    ) -> Vec<Note> {
        let mut notes_with_holds = notes;

        // Build a map of time -> frequency spectrum
        let mut spectrum_map: std::collections::HashMap<u32, Vec<f32>> =
            std::collections::HashMap::new();

        for (time, spectrum) in frequency_data {
            let time_ms = (time * 1000.0) as u32;
            spectrum_map.insert(time_ms, spectrum.clone());
        }

        // For each note, check if there's sustained energy after it
        for note in &mut notes_with_holds {
            let note_time_ms = (note.time * 1000.0) as u32;
            
            // Find sustained energy in the frequency range for this note's lane
            if let Some((_, freq_low, freq_high)) = lane_to_freq_range.iter()
                .find(|(lane, _, _)| *lane == note.col)
            {
                let hold_duration = self.find_sustained_energy(
                    &spectrum_map,
                    note_time_ms,
                    *freq_low,
                    *freq_high,
                );

                if hold_duration >= self.min_hold_duration {
                    note.duration = hold_duration;
                }
            }
        }

        notes_with_holds
    }

    /// Find how long energy is sustained in a frequency range after a start time
    fn find_sustained_energy(
        &self,
        spectrum_map: &std::collections::HashMap<u32, Vec<f32>>,
        start_time_ms: u32,
        freq_low: f32,
        freq_high: f32,
    ) -> f32 {
        let mut times: Vec<u32> = spectrum_map.keys().cloned().collect();
        times.sort();

        let start_idx = times.binary_search(&start_time_ms).unwrap_or_default();
        let mut hold_duration = 0.0;

        // Check for sustained energy in subsequent frames
        for &time_ms in &times[start_idx..] {
            if let Some(spectrum) = spectrum_map.get(&time_ms) {
                let energy = self.get_band_energy(spectrum, freq_low, freq_high);
                
                if energy >= self.sustain_threshold {
                    hold_duration = (time_ms as f32 - start_time_ms as f32) / 1000.0;
                } else {
                    // Energy dropped below threshold, hold ends
                    break;
                }
            }
        }

        hold_duration
    }

    /// Calculate energy in a frequency band
    fn get_band_energy(&self, spectrum: &[f32], freq_low: f32, freq_high: f32) -> f32 {
        let low_bin = (freq_low / 100.0) as usize;
        let high_bin = (freq_high / 100.0) as usize;

        spectrum
            .get(low_bin..=high_bin.min(spectrum.len() - 1))
            .map(|band| band.iter().sum())
            .unwrap_or(0.0)
    }

    /// Merge notes that are too close and should be treated as holds
    pub fn merge_nearby_notes(&self, mut notes: Vec<Note>, gap_threshold: f32) -> Vec<Note> {
        if notes.len() < 2 {
            return notes;
        }

        notes.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));

        let mut merged = Vec::new();
        let mut current = notes[0].clone();

        for note in &notes[1..] {
            let gap = note.time - (current.time + current.duration);
            
            if gap <= gap_threshold && note.col == current.col {
                // Merge: extend current note's duration
                current.duration = note.time + note.duration - current.time;
            } else {
                // Gap too large, add current and start new
                merged.push(current.clone());
                current = note.clone();
            }
        }

        merged.push(current);
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_band_energy() {
        let detector = HoldDetector::new(0.5, 0.25);
        let spectrum = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        
        let energy = detector.get_band_energy(&spectrum, 0.0, 200.0);
        assert!(energy > 0.0);
    }

    #[test]
    fn test_merge_nearby_notes() {
        let detector = HoldDetector::new(0.5, 0.25);
        let notes = vec![
            Note { time: 0.0, col: 0, duration: 0.1 },
            Note { time: 0.15, col: 0, duration: 0.1 },
            Note { time: 1.0, col: 0, duration: 0.1 },
        ];

        let merged = detector.merge_nearby_notes(notes, 0.2);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].time, 0.0);
        assert!((merged[0].duration - 0.25).abs() < 0.001);
    }
}
