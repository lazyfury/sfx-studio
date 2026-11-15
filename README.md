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
├── docs/                  # plan, workflow, review records, format, parity
├── scripts/               # preflight / review gate / source hash / hooks
├── src/
│   ├── main.rs
│   ├── app.rs             # AppLogic state machine + undo/history
│   ├── app/               # input events, commands, audio bridge
│   ├── ui.rs              # shared actions/hit types + widget contracts
│   ├── ui/                # layout, paint and hit-testing modules
│   ├── model.rs           # Project / Patch / Layer / Clip / categories
│   ├── presets.rs         # project-local presets/<name>.json loader
│   ├── synth.rs           # v2 + v3 voices: oscillators, drums, noise, music
│   ├── fx.rs              # DSP v3: pan, de-click, ER, reverb, master
│   ├── audio.rs           # cpal stereo playback
│   ├── export.rs          # WAV + Godot .tres / baked assets
│   ├── tres.rs            # Godot .tres import
│   ├── theme.rs           # dark Minimal Studio tokens
│   └── icons.rs           # embedded SVG icons
└── run.sh
```

## Run

```bash
cd ~/Documents/sfx-studio
./run.sh                   # cargo run --release (prompts for a project folder)
# or
cargo run --release

# headless frame/label/panel check
cargo run -- --selfcheck
# unit tests
cargo test
# quality gate (fmt + clippy + tests + selfcheck)
./scripts/preflight.sh
# Rust ↔ Godot deterministic golden (needs Godot 4.x)
./scripts/parity.sh
```

The release binary is `target/release/sfx-studio`.

CLI helpers (also used by the asset pipeline):

```bash
cargo run -- --new   <project-dir>   # project + presets/ + baked audio
cargo run -- --dump  <project-dir>
cargo run -- --bake  <project-dir>   # audio/baked + sfx_manifest.json
cargo run -- --export <project-dir>  # audio/sfx/*.tres + music.tres
cargo run -- --project <project-dir> # open the GUI on a folder
cargo run -- --help

# Builtin presets and demo audio
cargo run -- --list-presets
cargo run -- --preset pickupCoin --seed 3 --out coin.wav
cargo run -- --install-presets <project-dir>   # add missing builtins
cargo run -- --demo <new-dir>                  # playable demo pack
cargo run -- --demo-track boss_theme --out boss.wav
cargo run -- --demo-reel --out reel.wav
```

## Workflow

- **Library** — `PROJECT | PRESETS` segmented control in the left title strip.
  PROJECT lists the sounds stored in `sfx.project.json`; PRESETS is the
  project-local library at `<project>/presets/*.json`. Search (`/`), category
  filter (`ALL` dropdown / `C`), favorites filter (star) and the scrollbar work
  in both modes. Clicking a preset auditions it without touching the project;
  the star (or `F`) keeps a preset favourite in
  `<project>/presets/.favorites.json`. With a preset selected, `MENU → EXPORT
  WAV` (or `⌘/Ctrl+E`) renders it directly to a 48 kHz / 24-bit stereo WAV
  without adding it to the project. Space auditions that preset instead of the
  project patch. PRESETS is strictly read-only: the right inspector shows the
  selected preset summary and project-edit controls are disabled. An empty
  library offers `INSTALL BUILTIN PRESETS`; `MENU → SAVE AS PRESET` adds the
  current project sound to the same library.
- **Preset drag-in** — drag a preset row onto the timeline to copy its layers
  into the current patch at the drop time (5 ms snap, Shift for free
  placement); drop it on the left panel to copy it into the project library.
  A 4 px threshold keeps click-to-audition separate from dragging. Project
  patches stay click-to-open; only PRESETS rows start a drag.
- **Sound** — waveform, play/stop/loop, randomize, A/B/C variations, favorite;
  `SAMPLE` layers load a WAV with pitch, start, reverse and crossfaded loop
  controls (see `docs/FORMAT.md`). v3 voices are band-limited (2× oversampled
  PolyBLEP) and the drum models are rebuilt; existing projects stay on v2 until
  `UPGRADE TO HD`.
- **Timeline** — drag clips to move/trim (5 ms snap, hold Shift for free
  placement); ⌘/Ctrl+wheel zooms around the cursor and Shift+wheel pans.
- **Parameters** — scrollable panel; click a slider to jump, drag to adjust,
  Shift/Alt for fine control, double-click to reset. `PAN` sits in the MIX
  block; the `SPACE` block holds the per-sound reverb amount/size and the
  legacy `UPGRADE TO HD` button. Waveform/ruler can be dragged to scrub the
  playhead.
- **Menu** — `MENU` holds Open/Save/New/Rename/Duplicate/Delete, Randomize/Vary/Save as Preset/Favorite, Export WAV/Bake/TRES.
- **Export** — `EXPORT WAV` writes 48 kHz · 24-bit stereo (TPDF dither);
  `BAKE GODOT` writes 48 kHz · 16-bit stereo WAVs + manifest (v3; legacy
  patches keep `Patch.rate`); `.tres` export bakes any v3 / automated / sampled
  patch so the resource plays exactly as previewed.
- **Hover tips** — abbreviated buttons explain themselves in Chinese in the
  status line and a small tooltip; labels keep the English key terms.

## Keyboard

| Key | Action |
| --- | --- |
| Space | Play / pause (PROJECT) · audition selected preset (PRESETS) |
| / | Focus search |
| 1–8 | Trigger patch 1–8 |
| R | Randomize current sound |
| V | Create A/B/C variations |
| F | Favorite patch (PROJECT) / selected preset (PRESETS) |
| ↑ / ↓ | Move through the active library |
| ← / → | Select previous / next layer |
| C | Cycle category filter |
| L | Toggle loop |
| ⌘/Ctrl+Z / ⇧⌘Z | Undo / redo |
| ⌘/Ctrl+S | Save project |
| ⌘/Ctrl+O | Open project folder |
| ⌘/Ctrl+E | Export 48 kHz · 24-bit stereo WAV (PROJECT patch / selected PRESET) |
| ⌘/Ctrl+B | Bake Godot assets |
| ⌘/Ctrl + wheel (timeline) | Zoom timeline |
| Shift + wheel (timeline) | Pan timeline |
| Shift + drag clip | Free placement (no 5 ms snap) |

## Project folder and internal presets

A project is a directory containing `sfx.project.json`. On launch the app asks
for that directory (or use `--project DIR`); cancelling leaves an empty shell
with `OPEN` / `MENU → NEW PROJECT`. `NEW PROJECT`/`--new` writes a starter
project plus an internal preset library:

```text
<project>/
├── sfx.project.json
├── presets/              # project-local presets, one Patch JSON per file
│   ├── laser.json        # 11 hand-built demo sounds
│   ├── pickup_coin.json  # + 11 jsfxr-style builtin families
│   └── .favorites.json   # optional UI favourites
└── audio/...
```

The PRESETS tab is read-only and always reflects the currently open project, so
switching projects switches the library. An old project without a preset bank
can install the bundled families from the empty PRESETS view or with
`--install-presets DIR`; nothing existing is overwritten. Dropping a preset on the timeline
copies its layers (with fresh IDs) into the current patch and anchors its clip
arrangement at the drop time. The current patch's DSP version, space and room
are kept, so a v2 project still renders the inserted layers with the legacy
chain until `UPGRADE TO HD`.

## Builtin presets and demo tracks

`src/builtin.rs` mirrors the small jsfxr library surface
(`sfxr.generate("pickupCoin")`): the eleven families `pickupCoin`,
`laserShoot`, `explosion`, `powerUp`, `hitHurt`, `jump`, `blipSelect`,
`synth`, `tone`, `click` and `random` are available by name, with snake_case
and uppercase spellings accepted. Every family is a normal `Patch`, so it can
be dropped on the timeline, written to `presets/` or rendered by the CLI.
Generation is deterministic for a given `(name, seed)` pair; `--seed` is the
equivalent of jsfxr's "generate another one".

`--new` seeds a fresh project with **11 hand-built demo presets + 11 builtin
presets** (22 files). Existing projects can pull in only the missing builtins
with `--install-presets <project-dir>`; nothing already in `presets/` is
overwritten.

`src/demo.rs` ships three small arrangements — `NEON DRIVE` (138 BPM),
`BOSS THEME` (168 BPM) and `STAR MENU` (92 BPM) — plus sound-effect cues placed
over the music. The MUSIC inspector has a `DEMO TRACKS` section that loads any
of them into the current project (undoable). `--demo <new-dir>` writes a
self-contained pack:

```text
<demo-dir>/
├── sfx.project.json          # 11 builtin patches + NEON DRIVE music
├── presets/                  # the same 11 presets (draggable in the GUI)
├── audio/baked/              # per-sound WAVs + music.wav + manifest
└── audio/demo/
    ├── neon_drive.wav        # music + cue mix, 48 kHz / 24-bit stereo
    ├── boss_theme.wav
    ├── star_menu.wav
    ├── reel.wav              # every preset in jsfxr order
    └── index.json            # seed, durations, file references
```

Open it with `--project <demo-dir>`, or audition the WAVs directly. The demo
pack refuses to overwrite an existing `sfx.project.json`.

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

## Development process (mandatory)

Every change must go through a second review before it can be pushed:

1. Update `docs/PLAN.md` (priority + acceptance).
2. Implement + `./scripts/preflight.sh`.
3. Independent second review using `docs/REVIEW_CHECKLIST.md`.
4. Write `docs/reviews/NNNN-<slug>.md` with `status: approved` and the
   current `source_hash`.
5. `./scripts/review_gate.sh` must pass.

Install the hooks once:

```bash
./scripts/install_hooks.sh   # pre-commit: preflight; pre-push: preflight + review gate
```

Details and the review record template: `docs/DEV_WORKFLOW.md`.
The review gate hashes `src/`, `scripts/`, `.githooks/`, `Cargo.toml`,
`Cargo.lock` and `plugins/**/*.gd`, so editing code automatically invalidates
an old approval.

## Documents

| File | Contents |
| --- | --- |
| `docs/PLAN.md` | prioritized plan, status, acceptance evidence, backlog |
| `docs/DEV_WORKFLOW.md` | mandatory two-stage review process |
| `docs/REVIEW_CHECKLIST.md` | review checklist |
| `docs/FORMAT.md` | project schema, export layout, migrations (incl. `dsp_version` 2→3) |
| `docs/PARITY.md` | Rust ↔ Godot audio parity rules and known gaps |
| `docs/reviews/` | signed review records (required by the gate) |

## Design

Dark Minimal Studio variant: near-black surfaces, one lime accent, 4px spacing
scale, compact 12px labels, a dedicated darker playback surface, and Lucide
icons from `classic-game-box/assets/icons` (ISC, license in
`assets/icons/LICENSE`).
