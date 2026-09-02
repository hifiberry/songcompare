# SongCompare

[![CI](https://github.com/hifiberry/songcompare/actions/workflows/ci.yml/badge.svg)](https://github.com/hifiberry/songcompare/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)

> **This is released as-is.** It is a tool we wrote for our own listening tests
> and published because it might be useful to someone else. There is no support,
> no warranty and no roadmap, and nobody is obliged to answer questions, fix
> bugs or review changes. Issues and pull requests may sit untouched.
>
> It is MIT licensed, so do what you like with it: use it, change it, fork it,
> ship it in something else. Just don't expect anything from us if it breaks.

A Rust-based audio comparison tool designed for comparing different versions of the same song. Perfect for evaluating different sources (CD vs streaming), mixing/mastering variations, encoding formats (lossy vs lossless), audio filters, or any other audio processing differences.

## Use Cases

- **Format Comparison**: Compare lossy encodings (MP3, AAC, OGG) against lossless sources (FLAC, WAV)
- **Mastering Versions**: Evaluate different masters of the same album (original vs remaster)
- **Source Comparison**: Compare different releases (CD vs vinyl rip vs streaming)
- **Audio Processing**: Test the impact of filters, EQ, or other audio processing
- **Encoding Settings**: Compare different encoder settings or bitrates
- **Blind Testing**: Use anonymous mode to eliminate bias when comparing versions

## Features

- **Multi-format Support**: Load audio files in various formats (MP3, AAC, ALAC, FLAC, WAV, OGG, M4A, MKV) using Symphonia
- **Audio Normalization**: Normalize audio levels to a target RMS dB level
- **Cross-correlation Alignment**: Automatically align audio files using cross-correlation
- **Seamless Crossfading**: Switch between audio sources with configurable crossfade periods
- **Interactive Playback Controls**: 
  - Play/Pause
  - Seek forward/backward
  - Switch between tracks
  - Random track selection
- **Anonymous Mode**: Compare audio files without knowing which is which until the end
- **Sample Rate Handling**: Automatic resampling when device doesn't support source sample rate
- **Windows Wildcard Support**: Built-in glob pattern expansion

## Installing

Requires Rust 1.85 or newer (edition 2024).

```bash
cargo install --git https://github.com/hifiberry/songcompare
```

### Debian package

The repository carries its own `debian/` directory, so a `.deb` can be built
from a checkout on a Debian system:

```bash
sudo apt-get install build-essential debhelper cargo libasound2-dev pkg-config
dpkg-buildpackage -us -uc -b
```

The binary package is named `hifiberry-songcompare` and installs
`/usr/bin/songcompare`. HiFiBerry OS builds it from this repository and
publishes it to the HiFiBerry apt repository.

## Building from source

```bash
git clone https://github.com/hifiberry/songcompare
cd songcompare
cargo build --release
```

The binary is written to `target/release/songcompare`.

On Linux, `cpal` needs the ALSA development headers:

```bash
sudo apt-get install libasound2-dev
```

## Testing

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

## Usage

```bash
songcompare [OPTIONS] <audio_file1> <audio_file2> ...
```

### Command Line Options

- `--normalize_db=VALUE` - Target RMS normalization level in dB (default: -25.0)
- `--no-normalisation` - Skip the normalization process entirely
- `--allow-resample` - Allow automatic resampling if sample rate is not supported
- `--maxshift=VALUE` - Enable audio alignment with maximum shift in samples (default: 0, disabled)
- `--correlator=TYPE` - Correlation algorithm for alignment: `simple`, `gccphat`, or `none` (default: gccphat)
- `--min-freq=VALUE` - Minimum frequency in Hz to use for GCC-PHAT correlation (default: 500)
- `--max-freq=VALUE` - Maximum frequency in Hz to use for GCC-PHAT correlation (default: 2000)
- `--fade-samples=VALUE` - Duration of crossfade in samples (default: 128)
- `--anonymize` - Hide filenames during playback and randomize track order
- `--debug` - Show detailed correlation information for each shift
- `-h`, `--help` - Print usage information and exit
- `-V`, `--version` - Print the version and exit

Unrecognised options are rejected rather than being treated as filenames. To pass
a file whose name begins with `-`, prefix it with a path (`./-song.wav`).

### Keyboard Controls

During playback:
- `ESC` - Exit the program
- `SPACE` - Toggle play/pause
- `←` - Skip backward 5 seconds
- `→` - Skip forward 5 seconds
- `↑` - Next track
- `↓` - Previous track
- `ENTER` - Switch to random track

## Examples

### Basic comparison with normalization
```bash
songcompare song1.wav song2.mp3 song3.flac
```

### Blind listening test
```bash
songcompare --anonymize "path/to/files/*.wav"
```

### Compare with alignment
```bash
songcompare --maxshift=1000 file1.wav file2.wav
```

### Use simple correlator for alignment
```bash
songcompare --maxshift=1000 --correlator=simple file1.wav file2.wav
```

### Custom normalization and crossfade
```bash
songcompare --normalize_db=-18 --fade-samples=2048 song1.wav song2.wav
```

### Without normalization (preserve original levels)
```bash
songcompare --no-normalisation *.wav
```

### With resampling enabled
```bash
songcompare --allow-resample --normalize_db=-20 song1.mp3 song2.mp3
```

## How It Works

### Normalization
The tool analyzes the RMS (Root Mean Square) level of each audio file and applies gain to match a target dB level. This ensures fair comparison between files with different mastering levels.

### Audio Alignment
When `--maxshift` is set to a value greater than 0, the tool:
1. Uses the first file as reference
2. Performs cross-correlation with subsequent files
3. Finds the optimal time shift (within ±maxshift samples)
4. Applies the shift by adding silence or trimming
5. Reports correlation before and after alignment

Two correlation algorithms are available:

#### None (`--correlator=none`)
- **Method**: No correlation or alignment
- **Process**: Skips alignment entirely
- **Best for**: When files are already aligned or alignment is not needed
- **Performance**: No computational overhead

#### Simple Correlator (`--correlator=simple`)
- **Method**: Time-domain cross-correlation
- **Process**: 
  - Mixes down to mono
  - Computes correlation at each possible shift
  - Normalizes by signal energy
- **Best for**: Clean signals with minimal noise, different filters
- **Performance**: Good for short alignment windows

#### GCC-PHAT Correlator (`--correlator=gccphat`) - Default
- **Method**: Generalized Cross-Correlation with Phase Transform
- **Process**:
  - Mixes down to mono
  - Performs FFT on both signals
  - Filters frequencies outside the range `--min-freq` to `--max-freq` (default: 500-2000 Hz)
  - Computes cross-power spectrum
  - Applies PHAT weighting (normalizes by magnitude, uses phase only)
  - Inverse FFT to get correlation function
- **Best for**: Noisy signals, reverberant environments, robust alignment
- **Performance**: More computationally intensive but more accurate
- **Note**: Mid-range frequencies (500-2000 Hz) are generally most stable for alignment, avoiding both low-frequency rumble and high-frequency noise/artifacts. Adjust the frequency range based on your content.

The GCC-PHAT algorithm is more robust to noise and reverberation, making it the default choice for general use.

Audio alignment is useful for comparing:
- Different encodings of the same source
- Recordings with slightly different start times
- Masters with different pre-roll

### Crossfading
Crossfading happens in two phases:
1. **Pre-fade period**: 128 samples of the old track continue playing
2. **Fade period**: Configurable linear crossfade from old to new track

### Anonymous Mode
When enabled, tracks are:
- Assigned random track numbers
- Displayed without filenames during playback
- Revealed with their mapping at program exit

This is ideal for blind listening tests where you want to avoid bias.

## Audio Processing Details

- **Ring Buffer**: 1 second of audio buffered for smooth playback
- **Sample Rate**: Automatically detects and uses highest supported device rate
- **Resampling**: High-quality sinc interpolation using rubato
- **Crossfade**: Linear interpolation between sources

## Dependencies

- `symphonia` - Audio decoding
- `cpal` - Cross-platform audio I/O
- `rubato` - Audio resampling
- `rustfft` - FFT library for frequency-domain correlation
- `crossterm` - Terminal input handling
- `glob` - Wildcard pattern matching
- `rand` - Random number generation

## Limitations

- Sample rate conversion requires `--allow-resample` flag
- Alignment correlates only the first 10000 frames of each file
- Windows-specific wildcard expansion (Unix shells expand wildcards automatically)

## License

Released under the [MIT License](LICENSE).

Copyright (c) 2026 HiFiBerry
