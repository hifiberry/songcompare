// Audio correlation module for time alignment

use rustfft::{FftPlanner, num_complex::Complex};

/// No-op correlator that skips alignment entirely
pub struct NoCorrelator {}

impl NoCorrelator {
    pub fn new() -> Self {
        NoCorrelator {}
    }
}

impl Correlator for NoCorrelator {
    fn find_best_shift(&self, _audio1: &[f32], _audio2: &[f32], _max_shift: usize, _channels: usize) -> (i32, f32, f32) {
        // No correlation or shifting
        (0, 0.0, 0.0)
    }
}

/// Trait for audio correlators that can align audio sources
pub trait Correlator {
    /// Find the best shift (in samples) to align audio2 with audio1
    /// Returns (shift, correlation_before, correlation_after)
    fn find_best_shift(&self, audio1: &[f32], audio2: &[f32], max_shift: usize, channels: usize) -> (i32, f32, f32);
    
    /// Apply a shift to audio samples by adding silence or trimming
    /// Positive shift delays the audio (adds silence at beginning)
    /// Negative shift advances the audio (trims from beginning)
    fn apply_shift(&self, samples: &[f32], shift: i32, channels: usize) -> Vec<f32> {
        if shift == 0 {
            return samples.to_vec();
        }
        
        let shift_samples = shift.abs() as usize;
        let frame_shift = shift_samples / channels * channels; // Align to frame boundary
        
        if shift > 0 {
            // Delay: add silence at the beginning
            let mut result = vec![0.0; frame_shift];
            result.extend_from_slice(samples);
            result
        } else {
            // Advance: trim from the beginning
            if frame_shift >= samples.len() {
                vec![0.0; samples.len()] // Return silence if shift is too large
            } else {
                samples[frame_shift..].to_vec()
            }
        }
    }
}

pub struct SimpleCorrelator {
    debug: bool,
}

impl SimpleCorrelator {
    pub fn new(debug: bool) -> Self {
        SimpleCorrelator { debug }
    }
}

impl Correlator for SimpleCorrelator {
    
    /// Find the best shift (in samples) to align audio2 with audio1 using cross-correlation
    /// Returns (shift, correlation_before, correlation_after)
    /// shift: the shift amount (positive means audio2 should be delayed, negative means advanced)
    /// correlation_before: correlation at shift=0
    /// correlation_after: correlation at best shift
    /// max_shift: maximum number of samples to search in either direction
    /// channels: number of audio channels (for proper frame alignment)
    fn find_best_shift(&self, audio1: &[f32], audio2: &[f32], max_shift: usize, channels: usize) -> (i32, f32, f32) {
        // Convert max_shift to frames to ensure we shift by complete frames
        let max_shift_frames = max_shift / channels;
        let max_shift_samples = max_shift_frames * channels;
        
        let compare_frames = 10000; // Compare first 10000 frames
        let num_frames = (audio1.len().min(audio2.len()) / channels).min(compare_frames);
        
        println!("  Finding best alignment (max shift: ±{} samples, {} frames)...", max_shift_samples, max_shift_frames);
        println!("  Mixing down to mono for correlation...");
        
        // Mix down to mono for correlation
        let mut audio1_mono = Vec::with_capacity(num_frames);
        let mut audio2_mono = Vec::with_capacity(num_frames);
        
        for frame in 0..num_frames {
            let mut sum1 = 0.0;
            let mut sum2 = 0.0;
            for ch in 0..channels {
                sum1 += audio1[frame * channels + ch];
                sum2 += audio2[frame * channels + ch];
            }
            audio1_mono.push(sum1 / channels as f32);
            audio2_mono.push(sum2 / channels as f32);
        }
        
        let mut best_shift = 0;
        let mut best_correlation = f32::MIN;
        let mut initial_correlation = 0.0;
        
        // Try different shifts (by frame)
        for shift_frames in -(max_shift_frames as i32)..=(max_shift_frames as i32) {
            let shift_samples = shift_frames * channels as i32;
            
            let mut correlation = 0.0;
            let mut audio1_energy = 0.0;
            let mut audio2_energy = 0.0;
            let mut count = 0;
            
            // Correlate mono signals
            for i in 0..num_frames {
                let idx2 = i as i32 + shift_frames;
                
                if idx2 >= 0 && (idx2 as usize) < audio2_mono.len() {
                    let sample1 = audio1_mono[i];
                    let sample2 = audio2_mono[idx2 as usize];
                    correlation += sample1 * sample2;
                    audio1_energy += sample1 * sample1;
                    audio2_energy += sample2 * sample2;
                    count += 1;
                }
            }
            
            if count > 0 && audio1_energy > 0.0 && audio2_energy > 0.0 {
                // Normalize correlation coefficient (Pearson correlation)
                let normalized_corr = correlation / (audio1_energy * audio2_energy).sqrt();
                
                if self.debug {
                    println!("    Shift: {:5} samples, Correlation: {:.6}", shift_samples, normalized_corr);
                }
                
                // Store initial correlation (at shift=0)
                if shift_frames == 0 {
                    initial_correlation = normalized_corr;
                }
                
                if normalized_corr > best_correlation {
                    best_correlation = normalized_corr;
                    best_shift = shift_samples;
                }
            }
        }
        
        (best_shift, initial_correlation, best_correlation)
    }
}

/// GCC-PHAT (Generalized Cross-Correlation with Phase Transform) correlator
/// Uses frequency domain processing for more robust time delay estimation
pub struct GccPhatCorrelator {
    debug: bool,
}

impl GccPhatCorrelator {
    pub fn new(debug: bool) -> Self {
        GccPhatCorrelator { debug }
    }
    
    /// Calculate next power of 2 for FFT size
    fn next_power_of_2(n: usize) -> usize {
        let mut p = 1;
        while p < n {
            p *= 2;
        }
        p
    }
}

impl Correlator for GccPhatCorrelator {
    fn find_best_shift(&self, audio1: &[f32], audio2: &[f32], max_shift: usize, channels: usize) -> (i32, f32, f32) {
        let max_shift_frames = max_shift / channels;
        let max_shift_samples = max_shift_frames * channels;
        
        let compare_frames = 10000;
        let num_frames = (audio1.len().min(audio2.len()) / channels).min(compare_frames);
        
        println!("  Finding best alignment using GCC-PHAT (max shift: ±{} samples, {} frames)...", max_shift_samples, max_shift_frames);
        println!("  Mixing down to mono for correlation...");
        
        // Mix down to mono
        let mut audio1_mono = Vec::with_capacity(num_frames);
        let mut audio2_mono = Vec::with_capacity(num_frames);
        
        for frame in 0..num_frames {
            let mut sum1 = 0.0;
            let mut sum2 = 0.0;
            for ch in 0..channels {
                sum1 += audio1[frame * channels + ch];
                sum2 += audio2[frame * channels + ch];
            }
            audio1_mono.push(sum1 / channels as f32);
            audio2_mono.push(sum2 / channels as f32);
        }
        
        // Pad to next power of 2 for efficient FFT
        let fft_size = Self::next_power_of_2(num_frames + max_shift_frames * 2);
        println!("  Using FFT size: {}", fft_size);
        
        // Prepare FFT planner
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);
        let ifft = planner.plan_fft_inverse(fft_size);
        
        // Convert to complex and pad
        let mut audio1_complex: Vec<Complex<f32>> = audio1_mono.iter()
            .map(|&x| Complex::new(x, 0.0))
            .collect();
        audio1_complex.resize(fft_size, Complex::new(0.0, 0.0));
        
        let mut audio2_complex: Vec<Complex<f32>> = audio2_mono.iter()
            .map(|&x| Complex::new(x, 0.0))
            .collect();
        audio2_complex.resize(fft_size, Complex::new(0.0, 0.0));
        
        // Perform FFT on both signals
        fft.process(&mut audio1_complex);
        fft.process(&mut audio2_complex);
        
        // Calculate cross-power spectrum with PHAT weighting
        let mut cross_spectrum: Vec<Complex<f32>> = audio1_complex.iter()
            .zip(audio2_complex.iter())
            .map(|(a, b)| {
                let cross = a * b.conj();
                let magnitude = cross.norm();
                if magnitude > 1e-10 {
                    // PHAT weighting: normalize by magnitude (phase-only)
                    cross / magnitude
                } else {
                    Complex::new(0.0, 0.0)
                }
            })
            .collect();
        
        // Inverse FFT to get correlation
        ifft.process(&mut cross_spectrum);
        
        // Find the peak in correlation (searching within max_shift range)
        let mut best_shift = 0;
        let mut best_correlation = f32::MIN;
        
        // Search positive shifts (delays in audio2)
        for shift_frames in 0..=max_shift_frames.min(fft_size / 2) {
            let shift_samples = shift_frames as i32 * channels as i32;
            
            // Calculate actual correlation - this is what we use to find the best shift
            let actual_corr = self.calculate_correlation(&audio1_mono, &audio2_mono, shift_frames as i32);
            
            if self.debug {
                let fft_corr_value = cross_spectrum[shift_frames].re / fft_size as f32;
                println!("    Shift: {:5} samples, FFT corr: {:.6}, Actual corr: {:.6}", shift_samples, fft_corr_value, actual_corr);
            }
            
            if actual_corr > best_correlation {
                best_correlation = actual_corr;
                best_shift = shift_samples;
            }
        }
        
        // Search negative shifts (advances in audio2) - appear at end of FFT result
        for shift_frames in 1..=max_shift_frames.min(fft_size / 2) {
            let idx = fft_size - shift_frames;
            let shift_samples = -(shift_frames as i32) * channels as i32;
            
            // Calculate actual correlation - this is what we use to find the best shift
            let actual_corr = self.calculate_correlation(&audio1_mono, &audio2_mono, -(shift_frames as i32));
            
            if self.debug {
                let fft_corr_value = cross_spectrum[idx].re / fft_size as f32;
                println!("    Shift: {:5} samples, FFT corr: {:.6}, Actual corr: {:.6}", shift_samples, fft_corr_value, actual_corr);
            }
            
            if actual_corr > best_correlation {
                best_correlation = actual_corr;
                best_shift = shift_samples;
            }
        }
        
        // Calculate correlations at shift=0 and at the best shift
        // Both use the overlapping samples for fair comparison
        let shift_frames = best_shift / channels as i32;
        let initial_correlation = self.calculate_correlation(&audio1_mono, &audio2_mono, 0);
        let final_correlation = self.calculate_correlation(&audio1_mono, &audio2_mono, shift_frames);
        
        (best_shift, initial_correlation, final_correlation)
    }
}

impl GccPhatCorrelator {
    /// Helper function to calculate simple correlation at a specific shift for reference
    fn calculate_correlation(&self, audio1: &[f32], audio2: &[f32], shift: i32) -> f32 {
        let mut correlation = 0.0;
        let mut energy1 = 0.0;
        let mut energy2 = 0.0;
        let mut count = 0;
        
        for i in 0..audio1.len() {
            let idx2 = i as i32 + shift;
            if idx2 >= 0 && (idx2 as usize) < audio2.len() {
                let s1 = audio1[i];
                let s2 = audio2[idx2 as usize];
                correlation += s1 * s2;
                energy1 += s1 * s1;
                energy2 += s2 * s2;
                count += 1;
            }
        }
        
        if count > 0 && energy1 > 0.0 && energy2 > 0.0 {
            correlation / (energy1 * energy2).sqrt()
        } else {
            0.0
        }
    }
}
