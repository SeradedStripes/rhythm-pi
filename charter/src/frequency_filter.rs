use rustfft::{FftPlanner, num_complex::Complex};

/// Frequency band information for different instruments
#[derive(Clone, Debug)]
pub struct FrequencyBand {
    pub name: &'static str,
    pub low_hz: f32,
    pub high_hz: f32,
}

impl FrequencyBand {
    /// Get frequency bands for each instrument
    pub fn for_instrument(instrument: &str) -> Self {
        match instrument.to_lowercase().as_str() {
            "vocals" => FrequencyBand {
                name: "vocals",
                low_hz: 200.0,
                high_hz: 4000.0,
            },
            "bass" => FrequencyBand {
                name: "bass",
                low_hz: 40.0,
                high_hz: 250.0,
            },
            "drums" => FrequencyBand {
                name: "drums",
                low_hz: 30.0,
                high_hz: 5000.0,
            },
            "lead" => FrequencyBand {
                name: "lead",
                low_hz: 400.0,
                high_hz: 8000.0,
            },
            _ => FrequencyBand {
                name: "default",
                low_hz: 40.0,
                high_hz: 8000.0,
            },
        }
    }
}

/// Filter audio to a specific frequency band
pub fn bandpass_filter(samples: &[f32], sample_rate: u32, band: &FrequencyBand) -> Vec<f32> {
    let fft_size = 2048;
    let hop_size = 512;

    // Compute STFT
    let mut filtered_samples = vec![0.0; samples.len()];
    let mut planner = FftPlanner::new();
    let fft_forward = planner.plan_fft_forward(fft_size);
    let fft_inverse = planner.plan_fft_inverse(fft_size);

    let num_frames = (samples.len() as i32 - fft_size as i32) / hop_size as i32;

    for frame_idx in 0..=num_frames.max(0) as usize {
        let start = frame_idx * hop_size;
        let end = (start + fft_size).min(samples.len());

        if end - start < fft_size {
            break;
        }

        // Apply Hann window and create FFT input
        let mut fft_input: Vec<Complex<f32>> = samples[start..end]
            .iter()
            .enumerate()
            .map(|(idx, &sample)| {
                let window = 0.5 * (1.0 - ((2.0 * std::f32::consts::PI * idx as f32) / (fft_size as f32 - 1.0)).cos());
                Complex::new(sample * window, 0.0)
            })
            .collect();

        // Forward FFT
        fft_forward.process(&mut fft_input);

        // Bandpass filtering in frequency domain
        let freq_resolution = sample_rate as f32 / fft_size as f32;
        for (bin, coeff) in fft_input.iter_mut().enumerate() {
            let freq = bin as f32 * freq_resolution;
            
            // Zero out frequencies outside our band
            if freq < band.low_hz || freq > band.high_hz {
                *coeff = Complex::new(0.0, 0.0);
            }
        }

        // Inverse FFT
        fft_inverse.process(&mut fft_input);

        // Overlap-add synthesis
        for (idx, coeff) in fft_input.iter().enumerate() {
            if start + idx < samples.len() {
                filtered_samples[start + idx] += coeff.re / fft_size as f32;
            }
        }
    }

    // Normalize output
    let max_val = filtered_samples.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    if max_val > 0.0 {
        filtered_samples.iter_mut().for_each(|x| *x /= max_val);
    }

    filtered_samples
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frequency_band_for_instrument() {
        let band = FrequencyBand::for_instrument("vocals");
        assert_eq!(band.name, "vocals");
        assert!(band.low_hz > 0.0);
        assert!(band.high_hz > band.low_hz);
    }

    #[test]
    fn test_all_instruments_have_bands() {
        for instrument in &["vocals", "bass", "drums", "lead"] {
            let band = FrequencyBand::for_instrument(instrument);
            assert_eq!(band.name, *instrument);
        }
    }

    #[test]
    fn test_bandpass_filter_output_length() {
        let samples = vec![0.1; 4410]; // 0.1 second at 44100 Hz
        let band = FrequencyBand::for_instrument("vocals");
        let filtered = bandpass_filter(&samples, 44100, &band);
        assert_eq!(filtered.len(), samples.len());
    }
}
