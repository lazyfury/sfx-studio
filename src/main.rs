//! SFX LAB — compact standalone game-audio synthesizer.

mod app;
mod audio;
mod builtin;
mod cli;
mod demo;
mod export;
mod fx;
mod icons;
mod library;
mod model;
mod music_bank;
mod parity;
mod presets;
mod project_io;
mod synth;
mod theme;
mod tres;
mod ui;
mod wav;

fn main() {
    cli::run();
}
