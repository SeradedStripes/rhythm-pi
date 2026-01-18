pub mod audio;
pub mod beat_detection;
pub mod quantizer;
pub mod lane_assigner;
pub mod hold_detector;
pub mod exporter;
pub mod frequency_filter;

use anyhow::Result;
use audio::AudioData;
use beat_detection::BeatDetection;
use quantizer::Quantizer;
use lane_assigner::{LaneAssigner, LaneAssignmentStrategy};
use hold_detector::HoldDetector;
use exporter::ChartExport;
use frequency_filter::{FrequencyBand, bandpass_filter};
use std::path::Path;

/// Main charter configuration
#[derive(Clone, Debug)]
pub struct CharterConfig {
    pub bpm: Option<f32>,              // If None, will be auto-detected
    pub grid_division: u8,             // 4, 8, 16, etc.
    pub sustain_threshold: f32,        // Energy threshold for holds
    pub min_hold_duration: f32,        // Minimum hold duration in seconds
    pub lane_strategy: LaneAssignmentStrategy,
}

impl Default for CharterConfig {
    fn default() -> Self {
        CharterConfig {
            bpm: None,
            grid_division: 4,
            sustain_threshold: 0.5,
            min_hold_duration: 0.25,
            lane_strategy: LaneAssignmentStrategy::Sequential,
        }
    }
}

/// Main charter that orchestrates the entire process
pub struct Charter {
    config: CharterConfig,
}

impl Charter {
    pub fn new(config: CharterConfig) -> Self {
        Charter { config }
    }

    /// Generate charts for all difficulties of an instrument
    pub fn generate_all_difficulties(
        &self,
        audio_path: &Path,
        song_id: &str,
        instrument: &str,
    ) -> Result<Vec<ChartExport>> {
        let audio = AudioData::load(audio_path)?;
        let mono = audio.to_mono()?;

        // Get frequency band for this instrument
        let freq_band = FrequencyBand::for_instrument(instrument);
        
        // Filter audio to instrument's frequency band
        log::info!("Filtering audio to {} frequency band ({}-{} Hz)", 
                   instrument, freq_band.low_hz, freq_band.high_hz);
        let filtered = bandpass_filter(&mono, audio.sample_rate, &freq_band);

        // Detect beats in the filtered signal (instrument-specific)
        let beat_detection = BeatDetection::detect(&filtered, audio.sample_rate)?;
        let bpm = self.config.bpm.unwrap_or(beat_detection.bpm);

        // Create charts for each difficulty
        let mut charts = Vec::new();

        // Easy - 4 columns
        charts.push(self.generate_chart(
            audio.sample_rate,
            &beat_detection,
            bpm,
            song_id,
            instrument,
            "Easy",
            4,
        )?);

        // Normal - 4 columns
        charts.push(self.generate_chart(
            audio.sample_rate,
            &beat_detection,
            bpm,
            song_id,
            instrument,
            "Normal",
            4,
        )?);

        // Hard - 4 columns
        charts.push(self.generate_chart(
            audio.sample_rate,
            &beat_detection,
            bpm,
            song_id,
            instrument,
            "Hard",
            4,
        )?);

        // Expert - 5 columns
        charts.push(self.generate_chart(
            audio.sample_rate,
            &beat_detection,
            bpm,
            song_id,
            instrument,
            "Expert",
            5,
        )?);

        Ok(charts)
    }

    /// Generate a single difficulty chart
    fn generate_chart(
        &self,
        sample_rate: u32,
        beat_detection: &BeatDetection,
        bpm: f32,
        song_id: &str,
        instrument: &str,
        difficulty: &str,
        num_lanes: u8,
    ) -> Result<ChartExport> {
        // Apply difficulty-specific filtering to note density
        let filtered_peaks = match difficulty {
            "Easy" => {
                // Easy: keep ~60% of notes (every other peak + smoothing)
                self.reduce_notes(&beat_detection.peaks, 0.6)
            }
            "Normal" => {
                // Normal: keep ~80% of notes
                self.reduce_notes(&beat_detection.peaks, 0.8)
            }
            "Hard" => {
                // Hard: keep all notes
                beat_detection.peaks.clone()
            }
            "Expert" => {
                // Expert: keep all notes and add some intermediate peaks
                self.enhance_notes(&beat_detection.peaks, &beat_detection.onset_strengths)
            }
            _ => beat_detection.peaks.clone(),
        };

        // Create notes from detected peaks
        let mut notes: Vec<beat_detection::Note> = filtered_peaks
            .iter()
            .map(|&time| beat_detection::Note {
                time,
                col: 0,
                duration: 0.0,
            })
            .collect();

        // Quantize notes to the beat grid
        let quantizer = Quantizer::new(bpm, sample_rate, self.config.grid_division);
        notes = quantizer.quantize_notes(notes);

        // Assign lanes with frequency-based strategy for variety
        let lane_assigner = LaneAssigner::new(LaneAssignmentStrategy::Sequential, num_lanes);
        notes = lane_assigner.assign_lanes(notes, None);

        // Detect holds (basic implementation)
        let hold_detector = HoldDetector::new(self.config.sustain_threshold, self.config.min_hold_duration);
        let lane_freq_ranges = vec![
            (0, 50.0, 150.0),
            (1, 150.0, 300.0),
            (2, 300.0, 600.0),
            (3, 600.0, 2000.0),
            (4, 2000.0, 8000.0),
        ];
        notes = hold_detector.detect_holds(notes, &[], &lane_freq_ranges);

        // Create chart export
        let chart = ChartExport::new(
            song_id.to_string(),
            instrument.to_string(),
            difficulty.to_string(),
            num_lanes,
            bpm,
            notes,
        );

        Ok(chart)
    }

    /// Reduce note count by filtering out weaker peaks
    fn reduce_notes(&self, peaks: &[f32], keep_ratio: f32) -> Vec<f32> {
        if peaks.is_empty() {
            return Vec::new();
        }

        let keep_count = (peaks.len() as f32 * keep_ratio).ceil() as usize;
        let target_spacing = peaks.len() as f32 / keep_count as f32;
        let mut result = Vec::new();
        let mut next_index = 0.0;

        for (i, &peak) in peaks.iter().enumerate() {
            if (i as f32) >= next_index {
                result.push(peak);
                next_index += target_spacing;
            }
        }

        result
    }

    /// Enhance notes for Expert difficulty by finding intermediate peaks
    fn enhance_notes(&self, peaks: &[f32], _strengths: &[f32]) -> Vec<f32> {
        let mut enhanced = peaks.to_vec();
        
        // Find gaps and add intermediate peaks if energy is sustained
        for i in 0..peaks.len().saturating_sub(1) {
            let gap = peaks[i + 1] - peaks[i];
            
            // If gap is > 0.5 seconds, look for intermediate peaks
            if gap > 0.5 {
                let gap_mid = (peaks[i] + peaks[i + 1]) / 2.0;
                enhanced.push(gap_mid);
            }
        }

        enhanced.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        enhanced.dedup_by(|a, b| (*a - *b).abs() < 0.05); // Remove duplicates within 50ms
        enhanced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_charter_config_default() {
        let config = CharterConfig::default();
        assert_eq!(config.grid_division, 4);
        assert!(config.bpm.is_none());
    }
}
