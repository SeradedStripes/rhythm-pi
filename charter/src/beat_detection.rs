use anyhow::Result;
use rustfft::{FftPlanner, num_complex::Complex};

#[derive(Clone, Debug)]
pub struct Note {
    pub time: f32,      // time in seconds
    pub col: u8,        // column (0-4)
    pub duration: f32,  // duration in seconds (0 for tap, >0 for hold)
}

#[derive(Clone, Debug)]
pub struct BeatDetection {
    pub peaks: Vec<f32>,        // beat times in seconds
    pub bpm: f32,               // estimated BPM
    pub onset_strengths: Vec<f32>, // energy values at each frame
}

impl BeatDetection {
    /// Detect beats using energy peaks in the audio signal
    pub fn detect(samples: &[f32], sample_rate: u32) -> Result<Self> {
        // Parameters
        let frame_size = 2048;
        let hop_size = 512;
        let fft_size = 2048;
        
        // Compute energy envelope using short-time Fourier transform approach
        let mut onset_strengths = Vec::new();
        let num_frames = (samples.len() as i32 - frame_size as i32) / hop_size as i32;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        for i in 0..num_frames {
            let start = (i * hop_size as i32) as usize;
            let end = (start + frame_size).min(samples.len());
            
            // Apply Hann window
            let windowed: Vec<Complex<f32>> = samples[start..end]
                .iter()
                .enumerate()
                .map(|(idx, &sample)| {
                    let window = 0.5 * (1.0 - ((2.0 * std::f32::consts::PI * idx as f32) / (frame_size as f32 - 1.0)).cos());
                    Complex::new(sample * window, 0.0)
                })
                .collect();

            // Pad to FFT size
            let mut fft_input: Vec<Complex<f32>> = windowed.clone();
            fft_input.resize(fft_size, Complex::new(0.0, 0.0));

            // Compute FFT
            fft.process(&mut fft_input);

            // Calculate energy (sum of magnitude squared in frequency domain)
            let energy: f32 = fft_input.iter().map(|c| c.norm_sqr()).sum();
            onset_strengths.push(energy.sqrt());
        }

        // Smooth the energy curve
        let smoothed = Self::smooth_curve(&onset_strengths, 3);
        
        // Detect peaks in the smoothed energy curve
        let peaks_indices = Self::find_peaks(&smoothed, 0.5);
        
        // Convert frame indices to time in seconds
        let peaks: Vec<f32> = peaks_indices
            .iter()
            .map(|&idx| (idx as f32 * hop_size as f32) / sample_rate as f32)
            .collect();

        // Estimate BPM from peak spacing
        let bpm = Self::estimate_bpm(&peaks);

        Ok(BeatDetection {
            peaks,
            bpm,
            onset_strengths: smoothed,
        })
    }

    /// Smooth curve using moving average
    fn smooth_curve(data: &[f32], window_size: usize) -> Vec<f32> {
        if data.is_empty() {
            return Vec::new();
        }

        let half_window = window_size / 2;
        let mut smoothed = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            let start = if i >= half_window { i - half_window } else { 0 };
            let end = (i + half_window + 1).min(data.len());
            
            let avg = data[start..end].iter().sum::<f32>() / (end - start) as f32;
            smoothed.push(avg);
        }

        smoothed
    }

    /// Find local peaks in a curve
    fn find_peaks(data: &[f32], threshold_ratio: f32) -> Vec<usize> {
        if data.len() < 3 {
            return Vec::new();
        }

        let max_val = data.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let threshold = max_val * threshold_ratio;

        let mut peaks = Vec::new();

        for i in 1..data.len() - 1 {
            if data[i] > threshold && data[i] > data[i - 1] && data[i] > data[i + 1] {
                peaks.push(i);
            }
        }

        peaks
    }

    /// Estimate BPM from beat peak spacing
    fn estimate_bpm(peaks: &[f32]) -> f32 {
        if peaks.len() < 2 {
            return 120.0; // default BPM
        }

        // Calculate inter-beat intervals
        let mut intervals = Vec::new();
        for i in 1..peaks.len().min(20) {
            intervals.push(peaks[i] - peaks[i - 1]);
        }

        if intervals.is_empty() {
            return 120.0;
        }

        // Find the most common interval (rough approach)
        let avg_interval = intervals.iter().sum::<f32>() / intervals.len() as f32;
        
        // BPM = 60 / interval_in_seconds
        let bpm = 60.0 / avg_interval;
        
        // Clamp to reasonable range
        bpm.clamp(60.0, 240.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smooth_curve() {
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let smoothed = BeatDetection::smooth_curve(&data, 3);
        assert_eq!(smoothed.len(), data.len());
        assert!(smoothed[0] <= smoothed[1]); // boundary behavior
    }

    #[test]
    fn test_find_peaks() {
        let data = vec![0.0, 1.0, 0.5, 2.0, 0.5, 1.5, 0.0];
        let peaks = BeatDetection::find_peaks(&data, 0.3);
        assert!(peaks.contains(&1)); // peak at index 1
        assert!(peaks.contains(&3)); // peak at index 3
    }

    #[test]
    fn test_estimate_bpm() {
        // Peaks spaced 0.5 seconds apart -> 120 BPM
        let peaks = vec![0.0, 0.5, 1.0, 1.5];
        let bpm = BeatDetection::estimate_bpm(&peaks);
        assert!((bpm - 120.0).abs() < 1.0);
    }
}
