use crate::beat_detection::Note;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum LaneAssignmentStrategy {
    /// Assign to lanes based on frequency bands
    FrequencyBased { low_hz: f32, mid_hz: f32, high_hz: f32 },
    /// Cycle through lanes sequentially
    Sequential,
    /// Random assignment (for testing)
    Random,
}

pub struct LaneAssigner {
    pub strategy: LaneAssignmentStrategy,
    pub num_lanes: u8, // 4 for Easy/Normal/Hard, 5 for Expert
}

impl LaneAssigner {
    pub fn new(strategy: LaneAssignmentStrategy, num_lanes: u8) -> Self {
        LaneAssigner {
            strategy,
            num_lanes,
        }
    }

    /// Assign lanes to detected notes based on the strategy
    pub fn assign_lanes(&self, notes: Vec<Note>, frequency_data: Option<&HashMap<u32, Vec<f32>>>) -> Vec<Note> {
        match &self.strategy {
            LaneAssignmentStrategy::FrequencyBased { low_hz, mid_hz, high_hz } => {
                self.assign_by_frequency(notes, *low_hz, *mid_hz, *high_hz, frequency_data)
            }
            LaneAssignmentStrategy::Sequential => {
                self.assign_sequential(notes)
            }
            LaneAssignmentStrategy::Random => {
                self.assign_random(notes)
            }
        }
    }

    /// Assign lanes based on frequency content at note time
    fn assign_by_frequency(
        &self,
        mut notes: Vec<Note>,
        low_hz: f32,
        mid_hz: f32,
        high_hz: f32,
        frequency_data: Option<&HashMap<u32, Vec<f32>>>,
    ) -> Vec<Note> {
        // If no frequency data, fall back to sequential
        let freq_data = match frequency_data {
            Some(data) => data,
            None => return self.assign_sequential(notes),
        };

        for note in &mut notes {
            // Find the closest time in frequency data
            let note_time_ms = (note.time * 1000.0) as u32;
            let mut closest_time = None;
            let mut closest_distance = u32::MAX;

            for &time in freq_data.keys() {
                let distance = if time > note_time_ms {
                    time - note_time_ms
                } else {
                    note_time_ms - time
                };
                if distance < closest_distance {
                    closest_distance = distance;
                    closest_time = Some(time);
                }
            }

            if let Some(time) = closest_time {
                if let Some(spectrum) = freq_data.get(&time) {
                    note.col = self.frequency_to_lane(spectrum, low_hz, mid_hz, high_hz);
                }
            }
        }

        notes
    }

    /// Convert frequency spectrum to lane assignment
    fn frequency_to_lane(&self, spectrum: &[f32], low_hz: f32, mid_hz: f32, high_hz: f32) -> u8 {
        let low_energy: f32 = spectrum.iter().take((low_hz / 100.0) as usize).sum();
        let mid_energy: f32 = spectrum
            .iter()
            .skip((low_hz / 100.0) as usize)
            .take(((mid_hz - low_hz) / 100.0) as usize)
            .sum();
        let high_energy: f32 = spectrum
            .iter()
            .skip((mid_hz / 100.0) as usize)
            .take(((high_hz - mid_hz) / 100.0) as usize)
            .sum();

        match self.num_lanes {
            4 => {
                // 4 lanes: assign based on energy
                let energies = [low_energy, mid_energy, high_energy];
                if energies.is_empty() {
                    0
                } else {
                    let max_idx = energies
                        .iter()
                        .enumerate()
                        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                        .map(|(idx, _)| idx)
                        .unwrap_or(0);
                    (max_idx as u8) % 4
                }
            }
            5 => {
                // 5 lanes: use more granular assignment
                let energies = vec![low_energy / 2.0, low_energy, mid_energy, high_energy, high_energy / 2.0];
                energies
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(idx, _)| (idx as u8) % 5)
                    .unwrap_or(0)
            }
            _ => 0,
        }
    }

    /// Assign lanes sequentially, cycling through available lanes
    fn assign_sequential(&self, mut notes: Vec<Note>) -> Vec<Note> {
        for (i, note) in notes.iter_mut().enumerate() {
            note.col = (i as u8) % self.num_lanes;
        }
        notes
    }

    /// Assign lanes randomly (for testing)
    fn assign_random(&self, mut notes: Vec<Note>) -> Vec<Note> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos() as u64;

        let mut rng = SimpleLcg::new(seed);

        for note in &mut notes {
            note.col = (rng.next() % self.num_lanes as u64) as u8;
        }

        notes
    }
}

/// Simple pseudo-random number generator
struct SimpleLcg {
    state: u64,
}

impl SimpleLcg {
    fn new(seed: u64) -> Self {
        SimpleLcg { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345);
        (self.state / 65536) % 32768
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sequential_assignment() {
        let assigner = LaneAssigner::new(LaneAssignmentStrategy::Sequential, 4);
        let notes = vec![
            Note { time: 0.0, col: 0, duration: 0.0 },
            Note { time: 1.0, col: 0, duration: 0.0 },
            Note { time: 2.0, col: 0, duration: 0.0 },
        ];

        let assigned = assigner.assign_lanes(notes, None);
        assert_eq!(assigned[0].col, 0);
        assert_eq!(assigned[1].col, 1);
        assert_eq!(assigned[2].col, 2);
    }

    #[test]
    fn test_lane_wrapping() {
        let assigner = LaneAssigner::new(LaneAssignmentStrategy::Sequential, 4);
        let notes = (0..8)
            .map(|i| Note {
                time: i as f32,
                col: 0,
                duration: 0.0,
            })
            .collect();

        let assigned = assigner.assign_lanes(notes, None);
        assert_eq!(assigned[4].col, 0); // Wraps back to 0
        assert_eq!(assigned[7].col, 3);
    }
}
