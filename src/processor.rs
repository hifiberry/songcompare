// Audio processor module

use rubato::{
    Resampler, SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction,
};

pub struct Processor {
    // Add fields as needed
}

impl Processor {
    pub fn new() -> Self {
        Processor {}
    }
    
    /// Calculate the maximum absolute level in the audio samples
    pub fn calculate_max_level(&self, samples: &[f32]) -> f32 {
        samples.iter()
            .map(|s| s.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }
    
    /// Calculate the RMS (Root Mean Square) level of the audio samples
    pub fn calculate_rms_level(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        
        let sum_of_squares: f32 = samples.iter()
            .map(|s| s * s)
            .sum();
        
        (sum_of_squares / samples.len() as f32).sqrt()
    }
    
    /// Convert linear level to dB
    pub fn level_to_db(level: f32) -> f32 {
        if level <= 0.0 {
            -100.0 // Return a very low dB value for silence
        } else {
            20.0 * level.log10()
        }
    }
    
    /// Normalize audio samples to a target RMS level
    /// Returns a new vector with normalized samples
    pub fn normalize_to_rms(&self, samples: &[f32], target_rms: f32) -> Vec<f32> {
        let current_rms = self.calculate_rms_level(samples);
        
        if current_rms == 0.0 {
            // Silence remains silence
            return samples.to_vec();
        }
        
        let gain = target_rms / current_rms;
        
        samples.iter()
            .map(|s| s * gain)
            .collect()
    }
    
    /// Normalize audio samples to a target RMS level in dB
    /// Returns a new vector with normalized samples
    pub fn normalize_to_rms_db(&self, samples: &[f32], target_rms_db: f32) -> Vec<f32> {
        // Convert dB to linear
        let target_rms = 10.0_f32.powf(target_rms_db / 20.0);
        self.normalize_to_rms(samples, target_rms)
    }
    
    /// Resample audio to a different sample rate
    /// samples: interleaved audio samples
    /// from_rate: source sample rate
    /// to_rate: target sample rate
    /// channels: number of audio channels
    pub fn resample(&self, samples: &[f32], from_rate: u32, to_rate: u32, channels: usize) 
        -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        
        if from_rate == to_rate {
            return Ok(samples.to_vec());
        }
        
        println!("Resampling from {} Hz to {} Hz ({} channels)", from_rate, to_rate, channels);
        
        // Deinterleave samples into per-channel vectors
        let num_frames = samples.len() / channels;
        let mut channel_data: Vec<Vec<f32>> = vec![Vec::with_capacity(num_frames); channels];
        
        for (i, &sample) in samples.iter().enumerate() {
            let channel = i % channels;
            channel_data[channel].push(sample);
        }
        
        // Create resampler
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        
        let mut resampler = SincFixedIn::<f32>::new(
            to_rate as f64 / from_rate as f64,
            2.0,
            params,
            num_frames,
            channels,
        )?;
        
        // Resample each channel
        let resampled_channels = resampler.process(&channel_data, None)?;
        
        // Interleave the resampled channels
        let output_frames = resampled_channels[0].len();
        let mut output = Vec::with_capacity(output_frames * channels);
        
        for frame in 0..output_frames {
            for channel in 0..channels {
                output.push(resampled_channels[channel][frame]);
            }
        }
        
        Ok(output)
    }
}
