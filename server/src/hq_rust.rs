use anyhow::{Result, Context};
use hound::WavReader;
use rustfft::{FftPlanner, num_complex::Complex};
use ndarray::prelude::*;
use std::path::{Path, PathBuf};
use std::fs;

fn hann_window(n: usize) -> Vec<f32> {
    (0..n).map(|i| {
        let x = (i as f32) / (n as f32);
        0.5 - 0.5 * (2.0 * std::f32::consts::PI * x).cos()
    }).collect()
}

fn stft(samples: &[f32], n_fft: usize, hop: usize) -> Array2<f32> {
    let w = hann_window(n_fft);
    let mut planner = FftPlanner::<f32>::new();
    let fft = planner.plan_fft_forward(n_fft);
    let frames = (samples.len().saturating_sub(n_fft) / hop) + 1;
    let mut spec = Array2::<f32>::zeros((n_fft/2+1, frames));

    let mut buffer: Vec<Complex<f32>> = vec![Complex{ re: 0.0, im: 0.0 }; n_fft];
    for i in 0..frames {
        let start = i * hop;
        for j in 0..n_fft {
            let s = if start + j < samples.len() { samples[start+j] } else { 0.0 };
            buffer[j].re = s * w[j];
            buffer[j].im = 0.0;
        }
        fft.process(&mut buffer);
        for k in 0..(n_fft/2+1) {
            spec[[k, i]] = buffer[k].norm();
        }
    }
    spec
}

fn spectral_flux(spec: &Array2<f32>) -> Vec<f32> {
    let cols = spec.shape()[1];
    let mut env = vec![0.0f32; cols];
    for t in 1..cols {
        let mut sum = 0.0f32;
        for f in 0..spec.shape()[0] {
            let diff = spec[[f, t]] - spec[[f, t-1]];
            if diff > 0.0 { sum += diff; }
        }
        env[t] = sum;
    }
    env
}

fn peak_pick(env: &[f32], pre: usize, post: usize, delta: f32) -> Vec<usize> {
    let mut peaks = Vec::new();
    let n = env.len();
    for i in 0..n {
        let left = env[i.saturating_sub(pre)..i].iter().cloned().fold(0./0., f32::max);
        let right = if i+1 <= n-1 { env[i+1..(i+1+post).min(n)].iter().cloned().fold(0./0., f32::max) } else { f32::NEG_INFINITY };
        if env[i] > left && env[i] > right && env[i] > delta { peaks.push(i); }
    }
    peaks
}

fn estimate_bpm_from_env(env: &[f32], sr: u32, hop: usize) -> f32 {
    // compute autocorrelation of onset envelope (downsampled)
    let n = env.len();
    if n < 4 { return 120.0; }
    let mut ac = vec![0.0f32; n];
    for lag in 1..n/2 {
        let mut sum = 0.0f32;
        for i in 0..(n-lag) { sum += env[i] * env[i+lag]; }
        ac[lag] = sum;
    }
    // find lag corresponding to highest autocorr in tempo range 40-200 BPM
    let sr_hop = sr as f32 / hop as f32; // frames per second
    let mut best_lag = 0usize;
    let mut best_val = 0.0f32;
    for lag in 1..(n/2) {
        let bpm = 60.0 * sr_hop / (lag as f32);
        if bpm < 40.0 || bpm > 200.0 { continue; }
        if ac[lag] > best_val { best_val = ac[lag]; best_lag = lag; }
    }
    if best_lag == 0 { return 120.0; }
    60.0 * sr_hop / (best_lag as f32)
}

fn frames_to_times(frames: &[usize], hop: usize, sr: u32) -> Vec<f32> {
    frames.iter().map(|&f| (f * hop) as f32 / sr as f32).collect()
}

pub fn generate_hq_charts_rust(song_id: &str, song_file: &Path, charts_dir: &Path, force: bool) -> Result<Vec<PathBuf>> {
    // read wav
    let mut reader = WavReader::open(song_file).context("open wav")?;
    let spec = reader.spec();
    let sr = spec.sample_rate;
    let mut samples: Vec<f32> = Vec::new();
    if spec.channels == 1 {
        for s in reader.samples::<i16>() { samples.push(s? as f32 / 32768.0); }
    } else {
        // mixdown to mono
        let mut it = reader.samples::<i16>();
        loop {
            let mut sum = 0.0f32;
            let mut count = 0;
            for _c in 0..spec.channels {
                if let Some(Ok(v)) = it.next() { sum += v as f32 / 32768.0; count += 1; } else { break; }
            }
            if count == 0 { break; }
            samples.push(sum / count as f32);
        }
    }

    let n_fft = 2048usize;
    let hop = 512usize;
    let spec_mag = stft(&samples, n_fft, hop);
    let onset_env = spectral_flux(&spec_mag);
    let bpm = estimate_bpm_from_env(&onset_env, sr, hop);

    // compute per-band onset envelopes
    let freqs_per_bin = sr as f32 / n_fft as f32;
    let bass_max = 200.0;
    let vocal_min = 200.0;
    let vocal_max = 3000.0;
    let lead_min = 500.0;

    let mut bands = vec![vec![0.0f32; onset_env.len()]; 4]; // bass,vocal,lead,drums
    for t in 0..spec_mag.shape()[1] {
        let mut bass_sum = 0.0f32;
        let mut vocal_sum = 0.0f32;
        let mut lead_sum = 0.0f32;
        let mut _total = 0.0f32;
        for k in 0..spec_mag.shape()[0] {
            let f = k as f32 * freqs_per_bin;
            let mag = spec_mag[[k, t]];
            _total += mag;
            if f <= bass_max { bass_sum += mag; }
            if f >= vocal_min && f <= vocal_max { vocal_sum += mag; }
            if f >= lead_min && f <= 5000.0 { lead_sum += mag; }
        }
        bands[0][t] = bass_sum;
        bands[1][t] = vocal_sum;
        bands[2][t] = lead_sum;
        // drums: spectral flux is good proxy
        bands[3][t] = onset_env[t];
    }

    // normalize band envelopes
    for b in 0..bands.len() { let m = bands[b].iter().cloned().fold(0./0., f32::max); if m > 0.0 { for v in bands[b].iter_mut() { *v /= m; } } }

    // detect peaks per band
    let mut per_instr_onsets: Vec<Vec<f32>> = Vec::new();
    for b in 0..bands.len() {
        let env = &bands[b];
        let peaks = peak_pick(env, 3, 3, 0.15);
        let times = frames_to_times(&peaks, hop, sr);
        per_instr_onsets.push(times);
    }

    // quantize to beat grid
    let beat_interval = 60.0 / bpm.max(30.0);
    let duration_sec = samples.len() as f32 / sr as f32;
    let mut beat_times = Vec::new();
    let mut t = 0.0f32;
    while t < duration_sec { beat_times.push(t); t += beat_interval; }

    // ensure charts dir
    fs::create_dir_all(charts_dir)?;

    let instruments = ["bass","vocals","lead","drums"];
    let difficulties = [("Easy",4,4usize), ("Normal",8,4usize), ("Hard",16,5usize)];
    let mut written = Vec::new();

    for (bi, instr) in instruments.iter().enumerate() {
        // skip if instrument not available per song metadata checked earlier by caller
        let onsets = &per_instr_onsets[bi];
        for (diff_name, quant, columns) in difficulties.iter() {
            let mut notes = Vec::new();
            for &ot in onsets {
                // find nearest beat index
                let mut best_beat = 0usize;
                let mut best_d = f32::MAX;
                for (i, &bt) in beat_times.iter().enumerate() {
                    let interval = if i+1 < beat_times.len() { beat_times[i+1] - bt } else { beat_interval };
                    for q in 0..*quant {
                        let subdiv = bt + (q as f32 / *quant as f32) * interval;
                        let d = (subdiv - ot).abs();
                        if d < best_d { best_d = d; best_beat = i * (*quant as usize) + q as usize; }
                    }
                }
                // derive column
                let column = best_beat % *columns;
                // store event time as nearest subdivision time
                let beat_idx = best_beat / (*quant as usize);
                let qidx = best_beat % (*quant as usize);
                let bstart = beat_times.get(beat_idx).cloned().unwrap_or(0.0);
                let interval = if beat_idx+1 < beat_times.len() { beat_times[beat_idx+1] - bstart } else { beat_interval };
                let subdiv_time = bstart + (qidx as f32 / *quant as f32) * interval;
                notes.push(serde_json::json!({"time": subdiv_time, "col": column}));
            }

            // if no notes, fallback to beat aligned notes for playability
            if notes.is_empty() {
                for (i, &bt) in beat_times.iter().enumerate() {
                    notes.push(serde_json::json!({"time": bt, "col": i % *columns}));
                }
            }

            let chart = serde_json::json!({
                "song_id": song_id,
                "instrument": instr,
                "difficulty": diff_name,
                "columns": *columns,
                "generated_at": chrono::Utc::now().timestamp(),
                "tempo": bpm,
                "notes": notes
            });
            let fname = format!("{}_{}_{}.chart.json", song_id, instr, diff_name);
            let path = charts_dir.join(fname);
            if path.exists() && !force { written.push(path); continue; }
            fs::write(&path, serde_json::to_string_pretty(&chart)?)?;
            written.push(path);
        }
    }

    Ok(written)
}
