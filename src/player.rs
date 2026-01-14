// Audio player module

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

const MIX_PERIOD: usize = 128;

pub enum PlayerCommand {
    Stop,
    Seek(i64), // Seek by number of samples (positive = forward, negative = backward)
    SwitchSource(Vec<f32>, u32), // Switch to new audio source (samples, sample_rate)
    Pause,
    Resume,
}

pub struct PlayerStatus {
    pub position_samples: usize,
    pub total_samples: usize,
    pub sample_rate: u32,
    pub is_playing: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum CrossfadeState {
    Normal,           // Normal playback
    WaitingToFade,   // Waiting MIX_PERIOD samples before starting fade
    Fading(usize),   // Currently fading, tracks samples processed in fade
}

struct PlaybackState {
    samples: Vec<f32>,
    position: usize,
    _channels: usize,
    is_playing: bool,
    is_paused: bool,
    ring_buffer: Vec<f32>,
    ring_buffer_read_pos: usize,
    ring_buffer_write_pos: usize,
    ring_buffer_size: usize,
    
    // Crossfade support
    next_samples: Option<Vec<f32>>,
    next_sample_rate: Option<u32>,
    crossfade_state: CrossfadeState,
    crossfade_counter: usize,
}

impl PlaybackState {
    fn new(samples: Vec<f32>, channels: usize, sample_rate: u32) -> Self {
        // Ring buffer size: 1 second of audio
        let ring_buffer_size = sample_rate as usize * channels;
        
        PlaybackState {
            samples,
            position: 0,
            _channels: channels,
            is_playing: true,
            is_paused: false,
            ring_buffer: vec![0.0; ring_buffer_size],
            ring_buffer_read_pos: 0,
            ring_buffer_write_pos: 0,
            ring_buffer_size,
            next_samples: None,
            next_sample_rate: None,
            crossfade_state: CrossfadeState::Normal,
            crossfade_counter: 0,
        }
    }
    
    /// Calculate crossfade mix between two samples
    /// fade_position: 0 to MIX_PERIOD-1
    fn mix_samples(current: f32, next: f32, fade_position: usize) -> f32 {
        let fade_ratio = fade_position as f32 / MIX_PERIOD as f32;
        current * (1.0 - fade_ratio) + next * fade_ratio
    }
    
    fn fill_buffer(&mut self) {
        // Fill the ring buffer from source
        while self.ring_buffer_write_pos < self.ring_buffer_size && self.position < self.samples.len() {
            let sample = match self.crossfade_state {
                CrossfadeState::Normal => {
                    // Normal playback from current source
                    self.samples[self.position]
                }
                CrossfadeState::WaitingToFade => {
                    // Still playing from current source, counting down to fade
                    self.crossfade_counter += 1;
                    if self.crossfade_counter >= MIX_PERIOD {
                        // Start fading
                        self.crossfade_state = CrossfadeState::Fading(0);
                        self.crossfade_counter = 0;
                    }
                    self.samples[self.position]
                }
                CrossfadeState::Fading(fade_pos) => {
                    // Crossfading between sources
                    let current_sample = self.samples[self.position];
                    
                    if let Some(ref next_samples) = self.next_samples {
                        let next_sample = if self.position < next_samples.len() {
                            next_samples[self.position]
                        } else {
                            0.0
                        };
                        
                        let mixed = Self::mix_samples(current_sample, next_sample, fade_pos);
                        
                        // Update fade position
                        let new_fade_pos = fade_pos + 1;
                        if new_fade_pos >= MIX_PERIOD {
                            // Fade complete, switch to new source
                            if let Some(next_samples) = self.next_samples.take() {
                                self.samples = next_samples;
                            }
                            self.crossfade_state = CrossfadeState::Normal;
                        } else {
                            self.crossfade_state = CrossfadeState::Fading(new_fade_pos);
                        }
                        
                        mixed
                    } else {
                        // No next source available, just play current
                        self.crossfade_state = CrossfadeState::Normal;
                        current_sample
                    }
                }
            };
            
            self.ring_buffer[self.ring_buffer_write_pos] = sample;
            self.ring_buffer_write_pos += 1;
            self.position += 1;
        }
    }
    
    fn read_sample(&mut self) -> f32 {
        // Return silence when paused
        if self.is_paused {
            return 0.0;
        }
        
        if self.ring_buffer_read_pos >= self.ring_buffer_write_pos {
            // Buffer empty, fill it
            self.ring_buffer_write_pos = 0;
            self.ring_buffer_read_pos = 0;
            self.fill_buffer();
        }
        
        if self.ring_buffer_read_pos < self.ring_buffer_write_pos {
            let sample = self.ring_buffer[self.ring_buffer_read_pos];
            self.ring_buffer_read_pos += 1;
            sample
        } else {
            // No more data
            self.is_playing = false;
            0.0
        }
    }
    
    fn seek(&mut self, offset: i64) {
        let new_pos = (self.position as i64 + offset - (self.ring_buffer_write_pos as i64 - self.ring_buffer_read_pos as i64))
            .max(0)
            .min(self.samples.len() as i64) as usize;
        
        self.position = new_pos;
        self.ring_buffer_read_pos = 0;
        self.ring_buffer_write_pos = 0;
        self.fill_buffer();
    }
    
    fn get_position(&self) -> usize {
        // Actual position is the source position minus buffered data
        self.position.saturating_sub(self.ring_buffer_write_pos - self.ring_buffer_read_pos)
    }
    
    fn switch_source(&mut self, new_samples: Vec<f32>, _new_sample_rate: u32) {
        // Store the new source and start the crossfade sequence
        self.next_samples = Some(new_samples);
        self.next_sample_rate = Some(_new_sample_rate);
        self.crossfade_state = CrossfadeState::WaitingToFade;
        self.crossfade_counter = 0;
        
        println!("Source switch initiated, will fade after {} samples", MIX_PERIOD);
    }
}

pub struct Player {
    command_tx: Sender<PlayerCommand>,
    status: Arc<Mutex<PlayerStatus>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    _stream: cpal::Stream, // Keep stream alive
}

impl Player {
    pub fn new(samples: Vec<f32>, sample_rate: u32, channels: usize) -> Result<Self, Box<dyn std::error::Error>> {
        let (command_tx, command_rx) = mpsc::channel();
        
        let status = Arc::new(Mutex::new(PlayerStatus {
            position_samples: 0,
            total_samples: samples.len(),
            sample_rate,
            is_playing: true,
        }));
        
        let status_clone = Arc::clone(&status);
        
        // Create the audio stream
        println!("\nAvailable audio hosts:");
        for host_id in cpal::available_hosts() {
            println!("  {:?}", host_id);
        }
        
        let host = cpal::default_host();
        println!("Using host: {:?}", host.id());
        
        let device = host.default_output_device().ok_or("No output device available")?;
        println!("Using device: {:?}", device.name());
        
        // Get supported configs
        let supported_configs: Vec<_> = device.supported_output_configs()?.collect();
        
        let desired_sample_rate = cpal::SampleRate(sample_rate);
        let desired_channels = channels as u16;
        
        println!("\nRequested: {} Hz, {} channels", sample_rate, channels);
        println!("Supported configurations:");
        for config in &supported_configs {
            println!("  {} channels: {} Hz - {} Hz (sample format: {:?})", 
                config.channels(),
                config.min_sample_rate().0,
                config.max_sample_rate().0,
                config.sample_format()
            );
        }
        
        // Find a config that supports our sample rate and channels
        let matching_config = supported_configs
            .iter()
            .find(|config| {
                config.min_sample_rate() <= desired_sample_rate 
                    && config.max_sample_rate() >= desired_sample_rate
                    && config.channels() == desired_channels
                    && config.sample_format() == cpal::SampleFormat::F32
            });
        
        let actual_sample_rate;
        let actual_channels;
        
        if let Some(_config) = matching_config {
            // Use the requested config
            actual_sample_rate = sample_rate;
            actual_channels = desired_channels;
            println!("Using exact match: {} Hz, {} channels", actual_sample_rate, actual_channels);
        } else {
            // Fall back to device default
            println!("No exact match found, using device default config");
            let default_config = device.default_output_config()?;
            actual_sample_rate = default_config.sample_rate().0;
            actual_channels = default_config.channels();
            println!("Using: {} Hz, {} channels (NOTE: This may cause pitch/speed issues!)", actual_sample_rate, actual_channels);
        }
        
        // Create config
        let config = cpal::StreamConfig {
            channels: actual_channels,
            sample_rate: cpal::SampleRate(actual_sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };
        
        let playback_state = Arc::new(Mutex::new(PlaybackState::new(samples.clone(), channels, sample_rate)));
        let playback_state_clone = Arc::clone(&playback_state);
        
        let stream = device.build_output_stream(
            &config,
            move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                let mut state = playback_state_clone.lock().unwrap();
                for sample in data.iter_mut() {
                    *sample = state.read_sample();
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        )?;
        
        stream.play()?;
        
        let thread_handle = thread::spawn(move || {
            Self::command_thread(command_rx, playback_state, status_clone);
        });
        
        Ok(Player {
            command_tx,
            status,
            thread_handle: Some(thread_handle),
            _stream: stream,
        })
    }
    
    fn command_thread(
        command_rx: Receiver<PlayerCommand>,
        playback_state: Arc<Mutex<PlaybackState>>,
        status: Arc<Mutex<PlayerStatus>>,
    ) {
        loop {
            // Update status
            {
                let state = playback_state.lock().unwrap();
                let mut status = status.lock().unwrap();
                status.position_samples = state.get_position();
                status.is_playing = state.is_playing;
                
                if !state.is_playing {
                    break;
                }
            }
            
            // Check for commands
            if let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    PlayerCommand::Stop => {
                        let mut state = playback_state.lock().unwrap();
                        state.is_playing = false;
                        let mut status = status.lock().unwrap();
                        status.is_playing = false;
                        break;
                    }
                    PlayerCommand::Seek(sample_offset) => {
                        let mut state = playback_state.lock().unwrap();
                        state.seek(sample_offset);
                    }
                    PlayerCommand::SwitchSource(new_samples, new_sample_rate) => {
                        let mut state = playback_state.lock().unwrap();
                        state.switch_source(new_samples, new_sample_rate);
                    }
                    PlayerCommand::Pause => {
                        let mut state = playback_state.lock().unwrap();
                        state.is_paused = true;
                    }
                    PlayerCommand::Resume => {
                        let mut state = playback_state.lock().unwrap();
                        state.is_paused = false;
                    }
                }
            }
            
            thread::sleep(Duration::from_millis(50));
        }
    }
    
    pub fn get_status(&self) -> PlayerStatus {
        self.status.lock().unwrap().clone()
    }
    
    pub fn stop(&self) {
        let _ = self.command_tx.send(PlayerCommand::Stop);
    }
    
    pub fn pause(&self) {
        let _ = self.command_tx.send(PlayerCommand::Pause);
    }
    
    pub fn resume(&self) {
        let _ = self.command_tx.send(PlayerCommand::Resume);
    }
    
    pub fn seek(&self, sample_offset: i64) {
        let _ = self.command_tx.send(PlayerCommand::Seek(sample_offset));
    }
    
    pub fn switch_source(&self, new_samples: Vec<f32>, new_sample_rate: u32) {
        let _ = self.command_tx.send(PlayerCommand::SwitchSource(new_samples, new_sample_rate));
    }
    
    /// Get supported sample rates for the given number of channels
    pub fn get_supported_sample_rates(channels: usize) -> Result<Vec<u32>, Box<dyn std::error::Error>> {
        let host = cpal::default_host();
        let device = host.default_output_device().ok_or("No output device available")?;
        
        let supported_configs: Vec<_> = device.supported_output_configs()?.collect();
        let desired_channels = channels as u16;
        
        let mut sample_rates = Vec::new();
        
        for config in supported_configs {
            if config.channels() == desired_channels && config.sample_format() == cpal::SampleFormat::F32 {
                // Add common sample rates within the supported range
                let common_rates = [8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000];
                for &rate in &common_rates {
                    let rate_cpal = cpal::SampleRate(rate);
                    if config.min_sample_rate() <= rate_cpal && config.max_sample_rate() >= rate_cpal {
                        if !sample_rates.contains(&rate) {
                            sample_rates.push(rate);
                        }
                    }
                }
            }
        }
        
        sample_rates.sort();
        Ok(sample_rates)
    }
    
    /// Get the highest supported sample rate for the given number of channels
    pub fn get_highest_sample_rate(channels: usize) -> Result<u32, Box<dyn std::error::Error>> {
        let rates = Self::get_supported_sample_rates(channels)?;
        rates.last().copied().ok_or("No supported sample rates found".into())
    }
}

impl Clone for PlayerStatus {
    fn clone(&self) -> Self {
        PlayerStatus {
            position_samples: self.position_samples,
            total_samples: self.total_samples,
            sample_rate: self.sample_rate,
            is_playing: self.is_playing,
        }
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop();
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }
}
