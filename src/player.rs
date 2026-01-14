// Audio player module

use rodio::{OutputStream, Sink};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

pub enum PlayerCommand {
    Stop,
}

pub struct PlayerStatus {
    pub position_samples: usize,
    pub total_samples: usize,
    pub sample_rate: u32,
    pub is_playing: bool,
}

pub struct Player {
    command_tx: Sender<PlayerCommand>,
    status: Arc<Mutex<PlayerStatus>>,
    thread_handle: Option<thread::JoinHandle<()>>,
    _stream: OutputStream, // Keep stream alive
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
        
        // Create the audio stream - keep it in the Player struct so it stays alive
        let (stream, stream_handle) = OutputStream::try_default()?;
        
        let thread_handle = thread::spawn(move || {
            if let Err(e) = Self::playback_thread(samples, sample_rate, channels, command_rx, status_clone, stream_handle) {
                eprintln!("Playback error: {}", e);
            }
        });
        
        Ok(Player {
            command_tx,
            status,
            thread_handle: Some(thread_handle),
            _stream: stream,
        })
    }
    
    fn playback_thread(
        samples: Vec<f32>,
        sample_rate: u32,
        channels: usize,
        command_rx: Receiver<PlayerCommand>,
        status: Arc<Mutex<PlayerStatus>>,
        stream_handle: rodio::OutputStreamHandle,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let sink = Sink::try_new(&stream_handle)?;
        
        // Create a rodio source from our samples
        let source = rodio::buffer::SamplesBuffer::new(channels as u16, sample_rate, samples.clone());
        sink.append(source);
        
        // Monitor playback progress
        let update_interval = Duration::from_millis(100);
        let samples_per_update = (sample_rate as f64 * update_interval.as_secs_f64()) as usize * channels;
        let mut position = 0;
        
        loop {
            // Check for commands
            if let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    PlayerCommand::Stop => {
                        sink.stop();
                        break;
                    }
                }
            }
            
            // Check if playback is finished
            if sink.empty() {
                if let Ok(mut status) = status.lock() {
                    status.is_playing = false;
                    status.position_samples = samples.len();
                }
                break;
            }
            
            // Update position
            position += samples_per_update;
            if position > samples.len() {
                position = samples.len();
            }
            
            if let Ok(mut status) = status.lock() {
                status.position_samples = position;
            }
            
            thread::sleep(update_interval);
        }
        
        Ok(())
    }
    
    pub fn get_status(&self) -> PlayerStatus {
        self.status.lock().unwrap().clone()
    }
    
    pub fn stop(&self) {
        let _ = self.command_tx.send(PlayerCommand::Stop);
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
