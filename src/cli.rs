//! CLI entry points, headless selfcheck and app construction.
//!
//! Keeping this out of `app.rs` keeps the interactive state machine separate
//! from process startup and asset-pipeline flags.

use std::path::PathBuf;

use igui_app::{App, AppConfig};
use igui_backend_wgpu::FontConfig;
use igui_headless::{HeadlessPlugin, RecordingHandle};
use igui_winit::{
    ClipboardPlugin, GpuConfig, KeyboardPlugin, PointerPlugin, TextMeasurePlugin, TitlebarMode,
    WgpuPlugin, WindowConfig, WinitPlugin,
};

use crate::app::{SfxLogic, State};
use crate::project_io::{load_folder, open_folder, print_warnings};
use crate::{export, theme};

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "sfx-studio [--project DIR] [--bake DIR] [--export DIR] [--dump DIR] [--selfcheck]\n\
             \n\
             --project DIR  open a project folder\n\
             --bake DIR     write audio/baked + manifest\n\
             --export DIR   write audio/sfx/*.tres + music.tres\n\
             --dump DIR     print a short project summary\n\
             --parity-fixtures DIR  write Rust/Godot parity fixtures\n\
             --parity-check DIR     compare Rust and Godot renders\n\
             --selfcheck    headless layout smoke test"
        );
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--parity-fixtures") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--parity-fixtures needs a folder");
            std::process::exit(2);
        };
        match crate::parity::write_fixtures(&PathBuf::from(path)) {
            Ok(names) => println!("wrote {} parity fixtures", names.len()),
            Err(error) => {
                eprintln!("parity fixtures failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--parity-check") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--parity-check needs a folder");
            std::process::exit(2);
        };
        match crate::parity::compare(&PathBuf::from(path)) {
            Ok(reports) => {
                for report in reports {
                    println!(
                        "  {:<12} peak Δ{:>6.3} dB  RMS Δ{:>6.3} dB  max Δ{:>7.5}  ({} frames)",
                        report.name,
                        report.peak_delta_db,
                        report.rms_delta_db,
                        report.max_abs_delta,
                        report.frames
                    );
                }
                println!("parity ok");
            }
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|arg| arg == "--selfcheck") {
        selfcheck();
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--bake") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--bake needs a project folder");
            std::process::exit(2);
        };
        let folder = PathBuf::from(path);
        let (project, warnings) = load_folder(&folder).unwrap_or_else(|error| {
            eprintln!("bake failed: {error}");
            std::process::exit(1);
        });
        print_warnings(&warnings);
        match export::bake(&project, &folder) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("bake failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--export") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--export needs a project folder");
            std::process::exit(2);
        };
        let folder = PathBuf::from(path);
        let (project, warnings) = load_folder(&folder).unwrap_or_else(|error| {
            eprintln!("export failed: {error}");
            std::process::exit(1);
        });
        print_warnings(&warnings);
        match export::export_tres(&project, &folder) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("export failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--dump") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--dump needs a project folder");
            std::process::exit(2);
        };
        let folder = PathBuf::from(path);
        let (project, warnings) = load_folder(&folder).unwrap_or_else(|error| {
            eprintln!("dump failed: {error}");
            std::process::exit(1);
        });
        print_warnings(&warnings);
        println!(
            "{}: {} patches, music {} bpm / {} bars (schema v{})",
            project.name,
            project.patches.len(),
            project.music.bpm,
            project.music.bars,
            project.version
        );
        for patch in &project.patches {
            let layer = patch.layers.first();
            println!(
                "  {:<14} layers={} wave={} duration={:.3}s gain={:.2} category={}",
                patch.name,
                patch.layers.len(),
                layer.map(|l| l.wave.label()).unwrap_or("—"),
                patch.end(),
                layer.map(|l| l.gain).unwrap_or(0.0),
                patch.category.label()
            );
        }
        return;
    }
    let mut state = State::new();
    if let Some(index) = args.iter().position(|arg| arg == "--project") {
        match args.get(index + 1) {
            Some(path) => {
                if let Err(error) = open_folder(&mut state, PathBuf::from(path)) {
                    eprintln!("project failed: {error}");
                    std::process::exit(1);
                }
            }
            None => {
                eprintln!("--project needs a folder");
                std::process::exit(2);
            }
        }
    }
    build_app(state, false).run();
}

fn build_app(state: State, headless: bool) -> App {
    let builder = App::new(AppConfig {
        title: "SFX LAB".into(),
        size: (theme::VIEW_W as f64, theme::VIEW_H as f64),
        ..Default::default()
    });
    let builder = if headless {
        builder.plugin(HeadlessPlugin::new(theme::VIEW_W, theme::VIEW_H))
    } else {
        builder
            .plugin(WinitPlugin::new(WindowConfig {
                title: "SFX LAB".into(),
                size: (theme::VIEW_W as f64, theme::VIEW_H as f64),
                titlebar: TitlebarMode::Native,
                ime: false,
            }))
            .plugin(WgpuPlugin::new(GpuConfig {
                clear_color: theme::BG,
                font: FontConfig {
                    default_family: Some("Helvetica Neue".into()),
                    device_pixel_rasterization: true,
                    em_size: true,
                    ..Default::default()
                },
                ..Default::default()
            }))
            .plugin(PointerPlugin)
            .plugin(KeyboardPlugin)
            .plugin(TextMeasurePlugin)
            .plugin(ClipboardPlugin)
    };
    builder.logic(SfxLogic::new(state, !headless)).build()
}

fn selfcheck() {
    let mut app = build_app(State::new(), true);
    let recording = app
        .services()
        .get::<RecordingHandle>()
        .expect("headless recording handle")
        .0
        .clone();
    app.run_headless(2);
    let recording = recording.borrow();
    let frames = recording.frame_count();
    let (commands, text_commands, labels, in_bounds, parameters_visible) = recording
        .last_frame()
        .map(|frame| {
            let mut texts = 0;
            let mut labels = Vec::new();
            let mut in_bounds = true;
            let mut parameters_visible = false;
            for command in frame.commands() {
                if let igui_render::DrawCommand::DrawText { text, position, .. } = command {
                    texts += 1;
                    labels.push(text.clone());
                    if text == "PARAMETERS" && position.x >= theme::VIEW_W - theme::RIGHT_W {
                        parameters_visible = true;
                    }
                    if position.x < 0.0
                        || position.y < 0.0
                        || position.x > theme::VIEW_W
                        || position.y > theme::VIEW_H
                    {
                        in_bounds = false;
                    }
                }
            }
            (
                frame.command_count(),
                texts,
                labels,
                in_bounds,
                parameters_visible,
            )
        })
        .unwrap_or((0, 0, Vec::new(), false, false));
    let required = ["SFX LAB", "MENU", "LIBRARY", "PARAMETERS", "SOUND", "MUSIC"];
    let labels_ok = required
        .iter()
        .all(|needle| labels.iter().any(|text| text == needle));
    println!(
        "sfx-studio selfcheck: {frames} frames, {commands} commands, {text_commands} text runs"
    );
    if frames == 0
        || commands < 100
        || text_commands < 20
        || !labels_ok
        || !in_bounds
        || !parameters_visible
    {
        eprintln!(
            "selfcheck failed: labels_ok={labels_ok} in_bounds={in_bounds} parameters_visible={parameters_visible}"
        );
        std::process::exit(1);
    }
}
