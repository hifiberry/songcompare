# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-08-31

First public release.

### Added

- `-h` / `--help` and `-V` / `--version` flags.
- Unit tests for level/normalization maths and for shift detection and application.
- Continuous integration on Linux, macOS and Windows (clippy, tests, release build).
- MIT license.

### Fixed

- Alignment shifted files in the wrong direction. `find_best_shift` reported the
  lag it measured, while `apply_shift` expects the correction to undo that lag,
  so `--maxshift` roughly doubled the misalignment instead of removing it. The
  reported "correlation after" was computed in the search's own index convention
  and therefore still looked correct, which hid the problem.
- MP3 and AAC files could not be decoded. The advertised formats needed
  Symphonia's `mpa`, `aac`, `alac` and `isomp4` features, which were not enabled.
- Unrecognised command line options were silently treated as filenames, so a
  mistyped flag such as `--anonymise` failed later as a missing file.
- A panic during playback left the terminal in raw mode and the shell unusable.
  Raw mode is now restored from a panic hook.

[1.0.0]: https://github.com/hifiberry/songcompare/releases/tag/v1.0.0
