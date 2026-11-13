# SFX LAB — standalone client

Compact standalone game-audio synthesizer built on the sibling
[`igui`](../igui) checkout.

This is a normal single-binary Cargo project: no workspace, no separate plugin
crate, no symlinked Rust sources.

```
~/Documents/sfx-studio/
├── Cargo.toml
├── Cargo.lock
├── assets/icons/          # Lucide SVGs copied from classic-game-box
├── plugins/sfx_studio/    # Godot plugin (runtime + read-only Inspector)
├── src/
│   ├── main.rs
│   ├── app.rs             # igui AppLogic, input, undo, audio bridge
│   ├── ui.rs              # dark compact workspace, waveform, timeline, pad
│   ├── model.rs           # Project / Patch / Layer / Clip / categories
│   ├── synth.rs           # oscillators, noise, FM, pluck, drums, music
│   ├── audio.rs           # cpal playback
│   ├── export.rs          # WAV + Godot .tres / baked assets
│   ├── tres.rs            # Godot .tres import
│   ├── theme.rs           # dark Minimal Studio tokens
│   └── icons.rs           # embedded SVG icons
└── run.sh
```

## Run

```bash
cd ~/Documents/sfx-studio
./run.sh                   # cargo run --release
# or
cargo run --release

# headless frame/label/panel check
cargo run -- --selfcheck
# unit tests
cargo test
```

The release binary is `target/release/sfx-studio`.

## Workflow

- **Library** — search (`/`), category filter (`ALL` / `C`), favorites filter
  (star), selectable sound list.
- **Sound** — waveform, play/stop/loop, randomize, A/B/C variations, favorite.
- **Parameters** — scrollable panel; click a slider to jump, drag to adjust,
  Shift/Alt for fine control, double-click to reset. Waveform/ruler can be
  dragged to scrub the playhead.
- **Trigger pad** — bottom 8 pads, click or `1`–`8`.
- **Export** — `EXP` writes a 48 kHz WAV; `BAKE`/`TRES` write Godot assets.
- **Hover tips** — abbreviated buttons explain themselves in Chinese in the
  status line and a small tooltip; labels keep the English key terms.

## Keyboard

| Key | Action |
| --- | --- |
| Space | Play / pause |
| / | Focus search |
| 1–8 | Trigger pad |
| R | Randomize current sound |
| V | Create A/B/C variations |
| F | Toggle favorite |
| C | Cycle category filter |
| L | Toggle loop |
| ⌘/Ctrl+Z / ⇧⌘Z | Undo / redo |
| ⌘/Ctrl+S | Save project |
| ⌘/Ctrl+O | Open project folder |
| ⌘/Ctrl+E | Export 48 kHz WAV |
| ⌘/Ctrl+B | Bake Godot assets |

## Godot side

The Godot plugin is part of this repository at `plugins/sfx_studio/`, so the
Rust client and the Godot integration ship together in one Git repo. Each
Godot project uses it through a relative symlink:

```text
<game>/addons/sfx_studio -> ../../../Documents/sfx-studio/plugins/sfx_studio
```

The game repo script `tools/sync_sfx_addon.sh` creates/checks those links.

The Godot plugin is now read-only: it only adds a read-only Inspector summary
and waveform for `SfxPatch` / `SfxLayer` / `SfxClip` / `SfxMusic` /
`SfxAutomationPoint`. There is no dock, toolbar button or parameter editor.

## Design

Dark Minimal Studio variant: near-black surfaces, one lime accent, 4px spacing
scale, compact 12px labels, a dedicated darker playback surface, and Lucide
icons from `classic-game-box/assets/icons` (ISC, license in
`assets/icons/LICENSE`).
