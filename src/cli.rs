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
use crate::project_io::{create_project, load_folder, open_folder, print_warnings};
use crate::{builtin, demo, export, fx, presets::PresetBank, synth, theme};

pub fn run() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "sfx-studio [--project DIR] [--bake DIR] [--export DIR] [--dump DIR] [--selfcheck]\n\
             \n\
             --project DIR  open a project folder (otherwise a picker appears)\n\
             --new DIR      create project + presets/ + baked audio\n\
             --bake DIR     write audio/baked + manifest\n\
             --export DIR   write audio/sfx/*.tres + music.tres\n\
             --dump DIR     print a short project summary\n\
             --list-presets list the bundled jsfxr-style preset families\n\
             --preset NAME  render one builtin preset (--seed N, --out FILE.wav)\n\
             --install-presets DIR  add missing builtin presets to a project\n\
             --demo DIR     write a playable demo pack (presets + tracks + reel)\n\
             --demo-track NAME  render one demo track (--out FILE.wav)\n\
             --demo-reel    render every preset in sequence (--out FILE.wav)\n\
             --parity-fixtures DIR  write Rust/Godot parity fixtures\n\
             --parity-check DIR     compare Rust and Godot renders\n\
             --selfcheck    headless layout smoke test"
        );
        return;
    }
    if args.iter().any(|arg| arg == "--list-presets") {
        for name in builtin::PRESET_NAMES {
            println!("{:<12} {}", name, builtin::patch_name(name));
        }
        return;
    }
    let seed = flag_value(&args, "--seed")
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(0);
    if let Some(index) = args.iter().position(|arg| arg == "--preset") {
        let Some(name) = args.get(index + 1) else {
            eprintln!("--preset needs a name (try --list-presets)");
            std::process::exit(2);
        };
        let out = flag_value(&args, "--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("{name}.wav")));
        match render_preset(name, seed, &out) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--install-presets") {
        let Some(path) = args.get(index + 1) else {
            eprintln!("--install-presets needs a project folder");
            std::process::exit(2);
        };
        match install_presets(&PathBuf::from(path), seed) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("install presets failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--demo-track") {
        let Some(name) = args.get(index + 1) else {
            eprintln!("--demo-track needs a track name (neon_drive, boss_theme, star_menu)");
            std::process::exit(2);
        };
        let out = flag_value(&args, "--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("{name}.wav")));
        match render_demo_track(name, seed, &out) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if args.iter().any(|arg| arg == "--demo-reel") {
        let out = flag_value(&args, "--out")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("reel.wav"));
        match render_reel(seed, &out) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
        return;
    }
    if let Some(index) = args.iter().position(|arg| arg == "--demo") {
        let Some(path) = args.get(index + 1).map(PathBuf::from) else {
            eprintln!("--demo needs an output folder");
            std::process::exit(2);
        };
        match demo::write_pack(&path, seed) {
            Ok(message) => println!("{message}"),
            Err(error) => {
                eprintln!("demo pack failed: {error}");
                std::process::exit(1);
            }
        }
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
    if let Some(index) = args.iter().position(|arg| arg == "--new") {
        let Some(path) = args.get(index + 1).map(PathBuf::from) else {
            eprintln!("--new needs a folder");
            std::process::exit(2);
        };
        if let Err(error) = new_demo(&path) {
            eprintln!("new project failed: {error}");
            std::process::exit(1);
        }
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
                "  {:<14} layers={} wave={} duration={:.3}s gain={:.2} category={} dsp=v{} space={:.2} room={:.2}",
                patch.name,
                patch.layers.len(),
                layer.map(|l| l.wave.label()).unwrap_or("—"),
                patch.end(),
                layer.map(|l| l.gain).unwrap_or(0.0),
                patch.category.label(),
                patch.dsp_version,
                patch.space,
                patch.room
            );
        }
        return;
    }
    let mut state = State::empty();
    if let Some(index) = args.iter().position(|arg| arg == "--project") {
        match args.get(index + 1) {
            Some(path) => match open_folder(&mut state, PathBuf::from(path)) {
                Ok(warnings) => state.toast(if warnings.is_empty() {
                    "project loaded".to_string()
                } else {
                    format!("project loaded · {}", warnings.join("; "))
                }),
                Err(error) => {
                    eprintln!("project failed: {error}");
                    std::process::exit(1);
                }
            },
            None => {
                eprintln!("--project needs a folder");
                std::process::exit(2);
            }
        }
    } else if std::env::var_os("SFX_STUDIO_NO_PICKER").is_none() {
        // A project is a folder: select it before showing the editor. Cancel
        // leaves an empty shell with OPEN / NEW PROJECT affordances.
        if let Some(folder) = rfd::FileDialog::new()
            .set_title("Open a SFX project folder")
            .pick_folder()
        {
            match open_folder(&mut state, folder) {
                Ok(warnings) => state.toast(if warnings.is_empty() {
                    "project loaded".to_string()
                } else {
                    format!("project loaded · {}", warnings.join("; "))
                }),
                Err(error) => {
                    eprintln!("project failed: {error}");
                    state.ui.status = format!("open failed: {error} · OPEN a project folder");
                }
            }
        }
    }
    build_app(state, false).run();
}

fn flag_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let index = args.iter().position(|arg| arg == flag)?;
    args.get(index + 1).map(String::as_str)
}

fn render_preset(name: &str, seed: u32, out: &std::path::Path) -> Result<String, String> {
    if !builtin::is_builtin(name) {
        return Err(format!(
            "unknown preset '{name}' · available: {}",
            builtin::PRESET_NAMES.join(", ")
        ));
    }
    let patch = builtin::generate(name, seed).ok_or_else(|| format!("unknown preset '{name}'"))?;
    let rate = synth::PREVIEW_RATE;
    let stereo = fx::render_patch(&patch, rate);
    synth::write_wav_stereo(out, &stereo.left, &stereo.right, rate, 24)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "{} → {} · {:.2}s · 48 kHz / 24-bit stereo · seed {seed}",
        patch.name,
        out.display(),
        stereo.frames() as f32 / rate as f32
    ))
}

fn render_demo_track(name: &str, seed: u32, out: &std::path::Path) -> Result<String, String> {
    let tracks = demo::tracks();
    let index = demo::find(name).ok_or_else(|| {
        format!(
            "unknown demo track '{name}' · available: {}",
            tracks
                .iter()
                .map(|track| track.key)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    let track = &tracks[index];
    let rate = synth::PREVIEW_RATE;
    let stereo = demo::render_track(track, seed, rate);
    synth::write_wav_stereo(out, &stereo.left, &stereo.right, rate, 24)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "{} → {} · {:.2}s · 48 kHz / 24-bit stereo · seed {seed}",
        track.title,
        out.display(),
        stereo.frames() as f32 / rate as f32
    ))
}

fn render_reel(seed: u32, out: &std::path::Path) -> Result<String, String> {
    let rate = synth::PREVIEW_RATE;
    let stereo = demo::render_reel(seed, rate);
    synth::write_wav_stereo(out, &stereo.left, &stereo.right, rate, 24)
        .map_err(|error| error.to_string())?;
    Ok(format!(
        "preset reel → {} · {:.2}s · {} presets · seed {seed}",
        out.display(),
        stereo.frames() as f32 / rate as f32,
        builtin::PRESET_NAMES.len()
    ))
}

fn install_presets(folder: &std::path::Path, seed: u32) -> Result<String, String> {
    let written = PresetBank::install_missing(folder, &builtin::bank(seed))
        .map_err(|error| error.to_string())?;
    if written == 0 {
        return Ok(format!(
            "{} already has all {} builtin preset(s)",
            folder.display(),
            builtin::PRESET_NAMES.len()
        ));
    }
    Ok(format!(
        "installed {written} builtin preset(s) → {}/presets/",
        folder.display()
    ))
}

fn new_demo(folder: &std::path::Path) -> Result<(), String> {
    let (project, warnings) = create_project(folder)?;
    print_warnings(&warnings);
    let baked = export::bake(&project, folder)?;
    let presets = std::fs::read_dir(folder.join(crate::presets::PRESETS_DIR))
        .map(|entries| {
            entries
                .flatten()
                .filter(|entry| {
                    entry.path().extension().is_some_and(|ext| ext == "json")
                        && entry
                            .file_name()
                            .to_str()
                            .is_some_and(|name| !name.starts_with('.'))
                })
                .count()
        })
        .unwrap_or(0);
    println!(
        "created {} · {} sounds · {} presets · {baked}",
        folder.display(),
        project.patches.len(),
        presets
    );
    Ok(())
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
    let mut app = build_app(State::demo(), true);
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
    let required = [
        "SFX LAB",
        "MENU",
        "LIBRARY",
        "PROJECT",
        "PRESETS",
        "PARAMETERS",
        "SOUND",
        "MUSIC",
    ];
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
