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
│   ├── app.rs             # igui AppLogic, input, undo, audio bridge
│   ├── ui.rs              # dark compact workspace, waveform, timeline, menu
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
# quality gate (fmt + clippy + tests + selfcheck)
./scripts/preflight.sh
```

The release binary is `target/release/sfx-studio`.

CLI helpers (also used by the asset pipeline):

```bash
cargo run -- --dump  <project-dir>
cargo run -- --bake  <project-dir>   # audio/baked + sfx_manifest.json
cargo run -- --export <project-dir>  # audio/sfx/*.tres + music.tres
cargo run -- --project <project-dir> # open the GUI on a folder
cargo run -- --help
```

## Workflow

- **Library** — search (`/`), category filter (`ALL` dropdown / `C`), favorites
  filter (star), selectable sound list; each row's star toggles its favourite
  and a thin scrollbar appears when the list overflows.
- **Sound** — waveform, play/stop/loop, randomize, A/B/C variations, favorite.
- **Timeline** — drag clips to move/trim (5 ms snap, hold Shift for free
  placement); ⌘/Ctrl+wheel zooms around the cursor and Shift+wheel pans.
- **Parameters** — scrollable panel; click a slider to jump, drag to adjust,
  Shift/Alt for fine control, double-click to reset. Waveform/ruler can be
  dragged to scrub the playhead.
- **Menu** — `MENU` holds Open/Save/New/Rename/Duplicate/Delete, Randomize/Vary/Favorite, Export WAV/Bake/TRES.
- **Export** — menu entries write a 48 kHz WAV, or Godot baked assets / `.tres`.
- **Hover tips** — abbreviated buttons explain themselves in Chinese in the
  status line and a small tooltip; labels keep the English key terms.

## Keyboard

| Key | Action |
| --- | --- |
| Space | Play / pause |
| / | Focus search |
| 1–8 | Trigger patch 1–8 |
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
| ⌘/Ctrl + wheel (timeline) | Zoom timeline |
| Shift + wheel (timeline) | Pan timeline |
| Shift + drag clip | Free placement (no 5 ms snap) |

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
| `docs/FORMAT.md` | project schema, export layout, migrations |
| `docs/PARITY.md` | Rust ↔ Godot audio parity rules and known gaps |
| `docs/reviews/` | signed review records (required by the gate) |

## Design

Dark Minimal Studio variant: near-black surfaces, one lime accent, 4px spacing
scale, compact 12px labels, a dedicated darker playback surface, and Lucide
icons from `classic-game-box/assets/icons` (ISC, license in
`assets/icons/LICENSE`).
