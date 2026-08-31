// Audio processor module

use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
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
        samples
            .iter()
            .map(|s| s.abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0)
    }

    /// Calculate the RMS (Root Mean Square) level of the audio samples
    pub fn calculate_rms_level(&self, samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }

        let sum_of_squares: f32 = samples.iter().map(|s| s * s).sum();

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

        samples.iter().map(|s| s * gain).collect()
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
    pub fn resample(
        &self,
        samples: &[f32],
        from_rate: u32,
        to_rate: u32,
        channels: usize,
    ) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
        if from_rate == to_rate {
            return Ok(samples.to_vec());
        }

        println!(
            "Resampling from {} Hz to {} Hz ({} channels)",
            from_rate, to_rate, channels
        );

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
            for channel in resampled_channels.iter() {
                output.push(channel[frame]);
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative tolerance used when comparing floating point audio levels.
    const EPS: f32 = 1e-5;

    #[test]
    fn max_level_of_empty_input_is_zero() {
        let p = Processor::new();
        assert_eq!(p.calculate_max_level(&[]), 0.0);
    }

    #[test]
    fn max_level_uses_absolute_value() {
        let p = Processor::new();
        assert!((p.calculate_max_level(&[0.1, -0.8, 0.3]) - 0.8).abs() < EPS);
    }

    #[test]
    fn rms_of_empty_input_is_zero() {
        let p = Processor::new();
        assert_eq!(p.calculate_rms_level(&[]), 0.0);
    }

    #[test]
    fn rms_of_constant_signal_is_that_constant() {
        let p = Processor::new();
        assert!((p.calculate_rms_level(&[0.5; 128]) - 0.5).abs() < EPS);
    }

    #[test]
    fn rms_of_square_wave_is_amplitude() {
        let p = Processor::new();
        let samples: Vec<f32> = (0..128)
            .map(|i| if i % 2 == 0 { 0.25 } else { -0.25 })
            .collect();
        assert!((p.calculate_rms_level(&samples) - 0.25).abs() < EPS);
    }

    #[test]
    fn level_to_db_maps_unity_to_zero() {
        assert!(Processor::level_to_db(1.0).abs() < EPS);
    }

    #[test]
    fn level_to_db_halving_is_about_minus_six() {
        assert!((Processor::level_to_db(0.5) + 6.0206).abs() < 1e-3);
    }

    #[test]
    fn level_to_db_clamps_silence() {
        assert_eq!(Processor::level_to_db(0.0), -100.0);
        assert_eq!(Processor::level_to_db(-1.0), -100.0);
    }

    #[test]
    fn normalize_scales_to_target_rms() {
        let p = Processor::new();
        let out = p.normalize_to_rms(&[0.1; 64], 0.5);
        assert!((p.calculate_rms_level(&out) - 0.5).abs() < EPS);
    }

    #[test]
    fn normalize_leaves_silence_untouched() {
        let p = Processor::new();
        let out = p.normalize_to_rms(&[0.0; 32], 0.5);
        assert!(out.iter().all(|&s| s == 0.0));
    }

    #[test]
    fn normalize_db_hits_the_requested_level() {
        let p = Processor::new();
        let samples: Vec<f32> = (0..1024).map(|i| (i as f32 * 0.01).sin()).collect();
        let out = p.normalize_to_rms_db(&samples, -20.0);
        let db = Processor::level_to_db(p.calculate_rms_level(&out));
        assert!((db + 20.0).abs() < 1e-3, "got {} dB", db);
    }

    #[test]
    fn normalize_preserves_sample_count_and_shape() {
        let p = Processor::new();
        let samples = vec![0.2, -0.4, 0.6, -0.8];
        let out = p.normalize_to_rms_db(&samples, -10.0);
        assert_eq!(out.len(), samples.len());
        // Gain is uniform, so the ratio between any two samples is unchanged.
        assert!((out[1] / out[0] - samples[1] / samples[0]).abs() < EPS);
    }

    #[test]
    fn resample_is_a_no_op_when_rates_match() {
        let p = Processor::new();
        let samples = vec![0.1, 0.2, 0.3, 0.4];
        let out = p.resample(&samples, 48_000, 48_000, 2).unwrap();
        assert_eq!(out, samples);
    }
}
