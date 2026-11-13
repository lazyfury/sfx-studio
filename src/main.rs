//! SFX LAB — compact standalone game-audio synthesizer.

mod app;
mod audio;
mod cli;
mod export;
mod icons;
mod model;
mod project_io;
mod synth;
mod theme;
mod tres;
mod ui;

fn main() {
    cli::run();
}
