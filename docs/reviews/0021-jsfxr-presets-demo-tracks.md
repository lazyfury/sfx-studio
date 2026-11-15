# Review 0021 — jsfxr 预设目录 / demo 音轨

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/builtin.rs`, `src/demo.rs`, `src/presets.rs`, `src/project_io.rs`, `src/cli.rs`, `src/main.rs`, `src/model.rs`, `src/app.rs`, `src/app/commands.rs`, `src/ui.rs`, `src/ui/paint.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `3d2c405c02aa2a56a115e2b65b9ed7fac9f7ef27`
- source_hash: 6529efadd2e159aefe525664ff8ebc94b8ce9188e6b1fc8ae70fa5ca858b1bba
- status: approved

## 变更摘要

参考 jsfxr 的 library 用法（`sfxr.generate("pickupCoin")` / `sfxr.toWave`），
新增「按名字生成预设」和可直接试听的 demo 音轨；工程 schema、DSP 公式、
Godot 导出路径均未改变。

- **`src/builtin.rs`**：11 个 jsfxr family（`pickupCoin`、`laserShoot`、
  `explosion`、`powerUp`、`hitHurt`、`jump`、`blipSelect`、`synth`、`tone`、
  `click`、`random`），接受 camelCase / snake_case / 大写拼写。每个 family
  生成普通 `Patch`（多 layer、v3 空间链、category、seed 抖动）；
  `generate(name, seed)` 对同一 `(name, seed)` 完全确定，`random` 会选中一个
  真实 family 并保留 `random_sfx` 名称。
- **`src/demo.rs`**：三条 demo 音轨 `NEON DRIVE`（138 BPM，4 bars）、
  `BOSS THEME`（168 BPM，8 bars）、`STAR MENU`（92 BPM，4 bars），music bed
  上叠加 builtin cue；另有 `render_reel` 按 jsfxr 顺序串联全部预设。
- **CLI**：`--list-presets`、`--preset NAME [--seed N] [--out FILE]`、
  `--install-presets DIR`、`--demo DIR`、`--demo-track NAME [--out FILE]`、
  `--demo-reel [--out FILE]`。
- **新工程预设**：`NEW PROJECT` / `--new` 写入 11 个手调 demo + 11 个 builtin
  preset（22 个 JSON）；`--install-presets` 只补缺失项，按 patch 名与文件名
  双重去重，已有文件绝不覆盖。
- **GUI**：MUSIC 检查器新增 `DEMO TRACKS` 段，载入任一 demo 编曲会替换当前
  `project.music`，进入 undo 历史并重渲染音乐缓存。
- **demo pack**：`--demo DIR` 写出可被 `--project DIR` 直接打开的工程、
  `presets/`、`audio/baked/`、三条 48 kHz / 24-bit stereo 音轨、`reel.wav`
  与 `audio/demo/index.json`；拒绝覆盖已有 `sfx.project.json` 或非空
  `presets/`。

## 自审发现（已修）

1. **builtin patch 全是空名**：最初 `patch()` 未写 `name`，11 个预设重名，
   写入 `presets/` 时会互相覆盖。改为 `generate` 统一按 family 赋
   `patch_name`。
2. **`explosion_burst` / `synth_lead` 等文件名不可反查**：`is_builtin` 只接受
   camelCase，导致 `--preset explosion_burst` 失败。补齐 family 名与 patch 名
   的别名归一化。
3. **空目录安全**：`demo::write_pack` 最初只检查 `sfx.project.json`，若目录里
   已有 `presets/*.json` 会被覆盖。增加与非空 `presets/` 的冲突检查，并加
   回归测试。
4. **`--install-presets` 可能覆盖同名文件**：改为
   `PresetBank::install_missing`，同时检查已存在的 patch 名和文件 stem；冲突
   时使用 `_2` 后缀，已有文件内容保持不变（`custom.json` 不被替换）。
5. **demo 混音可能削波**：music bed + cue 叠加后统一按左右声道峰值归一到
   0.90；测试断言 peak ∈ (0.05, 1.0] 且全部样本有限。
6. **`Music` 临时变量多余的 `mut`**：clippy `-D warnings` 前清理。
7. **UI 注册表测试漏掉新段**：`music_inspector_follows_the_registry` 更新
   section 顺序，并断言三个 `DemoTrack` 按钮的 label 顺序。

## 第二审查发现

### Blocker

无。

### Major

1. **不是逐样本复刻 sfxr**：`builtin` 是按 SFX LAB 的 layer 模型重写的
   family，而非 sfxr 参数空间/8-bit 波形的移植。若要求与 jsfxr 输出逐样本
   一致，需要新增独立的 sfxr DSP 与 RNG 路径；本轮目标是“同样的名字与
   可玩的音色”，已在 PLAN 0021 非目标中写明。
2. **demo 音轨不是工程级多轨编排**：cue 在渲染时叠加，不进入
   `sfx.project.json` 的时间线；编辑音乐后 demo cue 不会自动跟随。若需要
   可编辑 demo，需要 schema v5 的 Arrangement/Track 结构。
3. **demo pack 的 `music.wav` 与 `audio/demo/neon_drive.wav` 内容不同**：
   前者只有 music bed，后者叠加了 cue。两者用途不同（Godot 循环曲 /
   试听 demo），已在 README 说明；消费方需要按需选择。

### Minor

1. `--demo-reel` / `--demo-track` 的默认输出名是 `<name>.wav`，在已有同名
   文件的目录会覆盖；显式 `--out` 可避免。与 `--preset` 行为一致。
2. `random` 每次只从 10 个 family 中选一个，不会像 jsfxr 那样对参数做连续
   随机化；`--seed` 已能复现结果。
3. `DEMO TRACKS` 的 active 高亮使用 `Music` 全字段相等；手动改过任一参数后
   三个按钮都不再高亮，符合“当前编曲已被修改”的语义。
4. demo 渲染是同步的（CLI/加载按钮），三条 4–8 bars 音轨约 6–12 s，当前
   可接受；更长的编曲应接入后台 worker。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 133 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `--list-presets`：11 个 family 与 patch 名
- [x] `--preset pickupCoin --seed 3 --out /tmp/coin.wav`：0.33 s、48 kHz、
      24-bit、peak 0.950、RMS 0.153
- [x] `--demo-track boss_theme`：12.12 s（8 bars + explosion 尾音）、peak 0.900
- [x] `--demo-reel`：7.95 s、11 个预设、peak 0.900
- [x] `--demo /tmp/sfx-demo-pack --seed 7`：11 presets、3 tracks、reel、
      index.json；`--dump` 显示 schema v4、music 138 BPM / 4 bars
- [x] `--new /tmp/sfx-new-proj`：22 个 preset 文件（11 demo + 11 builtin）
- [x] `--install-presets` 空工程写入 11 个，第二次报告 already has all；
      单测覆盖同名 patch 与同名文件均不覆盖
- [x] 工程 schema 仍为 v4；bake / `.tres` / parity 代码路径未改
- [x] 单测覆盖：family 别名、确定性、11/11 可听且有限、random 多态、demo
      三条音轨、reel 顺序、demo pack 可加载、install 不覆盖、GUI demo track
      action + undo、MUSIC inspector 新 section

## 残余风险与后续

- 若需要与 jsfxr 逐样本一致：新增独立 sfxr DSP/RNG、参数序列化与 golden。
- 可编辑 demo 编排：Arrangement/Track/SoundClip + schema v5。
- `--demo-track`/`--demo-reel` 默认输出名加 `--force` 或时间戳，避免覆盖。
- demo 渲染接入后台 worker；`random` 增加连续参数域。

## 签署

- reviewer_1: approved
- reviewer_2: approved
