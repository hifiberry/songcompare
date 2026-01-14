mod processor;
mod player;

use std::env;
use std::fs::File;
use std::thread;
use std::time::Duration;
use std::io::Write;
use glob::glob;
use crossterm::{
    event::{self, Event, KeyCode},
    terminal::{enable_raw_mode, disable_raw_mode},
};
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

struct AudioData {
    filename: String,
    samples: Vec<f32>,
    sample_rate: u32,
    channels: usize,
}

fn read_audio_file(path: &str) -> Result<AudioData, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    
    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path).extension() {
        hint.with_extension(ext.to_str().unwrap_or(""));
    }
    
    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();
    
    let probed = symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;
    let mut format = probed.format;
    
    let track = format.default_track().ok_or("No default track found")?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate.ok_or("No sample rate")?;
    let channels = track.codec_params.channels.ok_or("No channels")?.count();
    
    let decoder_opts = DecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs().make(&track.codec_params, &decoder_opts)?;
    
    let mut samples = Vec::new();
    
    while let Ok(packet) = format.next_packet() {
        if packet.track_id() != track_id {
            continue;
        }
        
        let decoded = decoder.decode(&packet)?;
        
        let mut sample_buf = symphonia::core::audio::SampleBuffer::<f32>::new(
            decoded.capacity() as u64,
            *decoded.spec()
        );
        sample_buf.copy_interleaved_ref(decoded);
        samples.extend_from_slice(sample_buf.samples());
    }
    
    Ok(AudioData {
        filename: path.to_string(),
        samples,
        sample_rate,
        channels,
    })
}

fn expand_wildcards(patterns: &[String]) -> Vec<String> {
    let mut file_paths = Vec::new();
    
    for pattern in patterns {
        // On Windows, convert backslashes to forward slashes for glob
        let normalized_pattern = pattern.replace('\\', "/");
        
        match glob(&normalized_pattern) {
            Ok(paths) => {
                let mut found_any = false;
                for entry in paths {
                    match entry {
                        Ok(path) => {
                            if let Some(path_str) = path.to_str() {
                                file_paths.push(path_str.to_string());
                                found_any = true;
                            }
                        }
                        Err(e) => eprintln!("Error reading path: {}", e),
                    }
                }
                // If glob didn't find anything, try the original pattern as a literal path
                if !found_any {
                    file_paths.push(pattern.clone());
                }
            }
            Err(e) => {
                eprintln!("Invalid glob pattern '{}': {}", pattern, e);
                // Treat as literal path
                file_paths.push(pattern.clone());
            }
        }
    }
    
    file_paths
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    
    if args.is_empty() {
        eprintln!("Usage: songcompare [--normalize_db=VALUE] <audio_file1> <audio_file2> ...");
        std::process::exit(1);
    }
    
    // Parse normalize_db option and filter out file patterns
    let mut normalize_db = -25.0;
    let mut file_patterns = Vec::new();
    
    for arg in &args {
        if let Some(value_str) = arg.strip_prefix("--normalize_db=") {
            match value_str.parse::<f32>() {
                Ok(value) => normalize_db = value,
                Err(_) => {
                    eprintln!("Invalid value for --normalize_db: {}", value_str);
                    std::process::exit(1);
                }
            }
        } else {
            file_patterns.push(arg.clone());
        }
    }
    
    let file_paths = expand_wildcards(&file_patterns);
    
    if file_paths.is_empty() {
        eprintln!("No audio files were successfully loaded");
        std::process::exit(1);
    }
    
    let mut audio_files = Vec::new();
    let processor = processor::Processor::new();
    
    println!("Normalizing audio files to {} dB RMS\n", normalize_db);
    
    for path in &file_paths {
        match read_audio_file(path) {
            Ok(mut audio) => {
                let max_level = processor.calculate_max_level(&audio.samples);
                let rms_level = processor.calculate_rms_level(&audio.samples);
                let max_db = processor::Processor::level_to_db(max_level);
                let rms_db = processor::Processor::level_to_db(rms_level);
                
                println!("Loaded: {} ({} samples, {} Hz, {} channels)", 
                    audio.filename, audio.samples.len(), audio.sample_rate, audio.channels);
                println!("  Before: Max level: {:.6} ({:.2} dB), RMS level: {:.6} ({:.2} dB)", 
                    max_level, max_db, rms_level, rms_db);
                
                // Normalize the audio
                audio.samples = processor.normalize_to_rms_db(&audio.samples, normalize_db);
                
                let max_level_after = processor.calculate_max_level(&audio.samples);
                let rms_level_after = processor.calculate_rms_level(&audio.samples);
                let max_db_after = processor::Processor::level_to_db(max_level_after);
                let rms_db_after = processor::Processor::level_to_db(rms_level_after);
                
                println!("  After:  Max level: {:.6} ({:.2} dB), RMS level: {:.6} ({:.2} dB)", 
                    max_level_after, max_db_after, rms_level_after, rms_db_after);
                
                audio_files.push(audio);
            }
            Err(e) => {
                eprintln!("Error loading {}: {}", path, e);
            }
        }
    }
    
    println!("\nLoaded {} audio file(s) into memory", audio_files.len());
    
    if audio_files.is_empty() {
        eprintln!("No audio files to play");
        std::process::exit(1);
    }
    
    // Start playback of the first audio file
    let first_audio = &audio_files[0];
    println!("\nStarting playback of: {}", first_audio.filename);
    
    let player = match player::Player::new(
        first_audio.samples.clone(),
        first_audio.sample_rate,
        first_audio.channels,
    ) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to create player: {}", e);
            std::process::exit(1);
        }
    };
    
    // Enable raw mode for keyboard input
    enable_raw_mode().unwrap();
    
    // Display progress indicator and handle keyboard input
    print!("\r");
    let mut should_exit = false;
    
    loop {
        let status = player.get_status();
        
        if !status.is_playing {
            println!("\r\nPlayback finished");
            should_exit = true;
        }
        
        // Calculate time in seconds
        let position_seconds = status.position_samples as f32 / (status.sample_rate * first_audio.channels as u32) as f32;
        let total_seconds = status.total_samples as f32 / (status.sample_rate * first_audio.channels as u32) as f32;
        
        let pos_min = (position_seconds / 60.0) as u32;
        let pos_sec = (position_seconds % 60.0) as u32;
        let total_min = (total_seconds / 60.0) as u32;
        let total_sec = (total_seconds % 60.0) as u32;
        
        print!("\rProgress: {:02}:{:02}/{:02}:{:02} [ESC: Exit, Left/Right: Skip ±5s]  ", pos_min, pos_sec, total_min, total_sec);
        std::io::stdout().flush().unwrap();
        
        // Check for keyboard events (non-blocking)
        if event::poll(Duration::from_millis(100)).unwrap() {
            if let Event::Key(key_event) = event::read().unwrap() {
                match key_event.code {
                    KeyCode::Esc => {
                        println!("\r\nExiting...");
                        player.stop();
                        should_exit = true;
                    }
                    KeyCode::Left => {
                        // Skip backward 5 seconds
                        let skip_samples = (5.0 * status.sample_rate as f32 * first_audio.channels as f32) as i64;
                        player.seek(-skip_samples);
                    }
                    KeyCode::Right => {
                        // Skip forward 5 seconds
                        let skip_samples = (5.0 * status.sample_rate as f32 * first_audio.channels as f32) as i64;
                        player.seek(skip_samples);
                    }
                    _ => {}
                }
            }
        }
        
        if should_exit {
            break;
        }
        
        thread::sleep(Duration::from_millis(400));
    }
    
    // Disable raw mode before exiting
    disable_raw_mode().unwrap();
}
