use anyhow::{Result, anyhow};
use std::path::Path;

#[derive(Clone, Debug)]
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl AudioData {
    /// Load audio from WAV or OGG file
    pub fn load(path: &Path) -> Result<Self> {
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_lowercase();

        match extension.as_str() {
            "wav" => Self::load_wav(path),
            "ogg" => Self::load_ogg(path),
            ext => Err(anyhow!("Unsupported audio format: {}", ext)),
        }
    }

    fn load_wav(path: &Path) -> Result<Self> {
        let reader = hound::WavReader::open(path)
            .map_err(|e| anyhow!("Failed to open WAV file: {}", e))?;

        let spec = reader.spec();
        
        // Convert samples to f32
        let samples: Result<Vec<f32>> = reader
            .into_samples::<i32>()
            .map(|s| {
                s.map(|sample| {
                    // Normalize 32-bit integer to float (-1.0 to 1.0)
                    sample as f32 / (i32::MAX as f32)
                })
                .map_err(|e| anyhow!("Failed to read WAV sample: {}", e))
            })
            .collect();

        Ok(AudioData {
            samples: samples?,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        })
    }

    fn load_ogg(_path: &Path) -> Result<Self> {
        // OGG support can be added later if needed
        Err(anyhow!("OGG support not yet implemented"))
    }

    /// Convert multi-channel audio to mono by averaging channels
    pub fn to_mono(&self) -> Result<Vec<f32>> {
        if self.channels == 1 {
            return Ok(self.samples.clone());
        }

        let channels = self.channels as usize;
        let num_samples = self.samples.len() / channels;
        let mut mono = vec![0.0f32; num_samples];

        for (i, chunk) in self.samples.chunks(channels).enumerate() {
            let sum: f32 = chunk.iter().sum();
            mono[i] = sum / channels as f32;
        }

        Ok(mono)
    }

    /// Get audio duration in seconds
    pub fn duration(&self) -> f32 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.samples.len() as f32 / (self.sample_rate as f32 * self.channels as f32)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audio_duration_calculation() {
        let audio = AudioData {
            samples: vec![0.0; 44100],
            sample_rate: 44100,
            channels: 2,
        };
        assert_eq!(audio.duration(), 0.5);
    }
}
