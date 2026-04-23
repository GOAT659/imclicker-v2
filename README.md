# IMCLICKER V2 (Rust)

Desktop autoclicker concept for Windows, written in Rust with an emphasis on:

- minimal UI-thread work
- separate high-priority worker thread for click timing
- hybrid wait strategy (high-resolution waitable timer + short spin phase)
- stable delivery of target CPS without bursty catch-up behavior
- release-profile optimizations enabled in `Cargo.toml`

## Tech

- Rust
- `eframe/egui` for the desktop UI
- `windows-sys` for native Windows input and timing APIs

## Build

Install Rust on Windows, then in the project folder run:

```bash
cargo build --release
```

Binary output:

```text
target/release/imclicker_v2.exe
```

## Notes on performance and timing

This project is optimized around standard Windows user-mode APIs:

- the UI and the click engine are split into different threads
- the click engine raises only its own worker-thread priority
- the engine uses a coarse high-resolution sleep first, then a very short spin-wait near the deadline
- when the app falls behind schedule, it resynchronizes instead of sending burst spikes

That keeps CPU use lower than a full busy-loop while still improving timing consistency.

## Important limitation

No user-mode autoclicker can guarantee that every game will register the exact selected CPS value.
Observed in-game CPS can differ because of:

- game-side input rate caps
- frame pacing and focus behavior
- input buffering rules
- anti-cheat / protection behavior
- OS scheduling noise on a loaded system

What this build does is minimize local timing drift and reduce UI-related overhead.

## Config

The app saves config to a JSON file in the user config directory through the `directories` crate.
