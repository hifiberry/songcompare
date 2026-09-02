# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.1] - 2026-09-02

### Added

- Debian packaging (`debian/`), producing a `hifiberry-songcompare` package
  that installs `/usr/bin/songcompare`. The build runs the unit tests, so a
  broken tree fails where the `.deb` is made and not only in CI.

### Fixed

- The crate did not build on the Rust 1.85 that `Cargo.toml` declares as its
  minimum. A `let` chain in the key-event loop is stable only from 1.88, so
  every toolchain between the declared minimum and 1.88 failed with
  `error[E0658]: 'let' expressions in this position are unstable` — Debian
  trixie's 1.85.0 among them, which is what the package is built with. CI
  tests only against current stable and so never saw it.

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

[1.0.1]: https://github.com/hifiberry/songcompare/releases/tag/v1.0.1
[1.0.0]: https://github.com/hifiberry/songcompare/releases/tag/v1.0.0
