# Review 0041 — P5 安全项：Wave 注册表 / 手势测试 / AUDIO·MUSIC 生命周期

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass; see 0042)
- scope: `src/model.rs`, `src/library.rs`, `src/music_bank.rs`, `src/app/{input,commands}.rs`, `src/ui.rs`, `src/ui/paint.rs`, `src/export.rs`, `README.md`, `docs/PLAN.md`
- diff_base: `142559d`（统一 Library 前基线；承接 0035–0039）
- source_hash: e563d3b6f2798ad1730c7f382dc9886c3d2296ec958a97c8bec6b7585ceb37a9
- status: approved

## 变更摘要

完成 P5 中不需要改变工程 schema / Godot 导出的部分。

### Wave 注册表

- 新增 `WaveDef` / `WaveFamily` / `WAVE_DEFS`。
- `Wave::label` / `desc` / `family` / `is_tone` / `godot_id` 从表派生；
  `layer_sections` 按 family 分派。
- `Curve` 的 Godot id 8 改为显式 `godot_alias = Pluck`；新增 native id
  唯一性测试，导出路径有 debug 断言。
- `is_tone()` 仍包含 `Curve`（它走 tone 振荡器路径），但 inspector 使用
  `family()` 给它独立 CURVE 段。

### 手势测试入口

- `AppLogic::event` 只委托 `SfxLogic::handle_event(event)`。
- 测试可直接驱动 PointerDown/PointerUp/KeyDown，不再需要构造
  `EventContext`。
- 新增 4 条手势测试：图层夺回焦点、MUSIC tab 夺回焦点、Overlay 外部点击
  消费、Cmd+Z 走命令表。

### AUDIO / MUSIC 生命周期

- `AudioBank::rename(root, index, name)` / `delete(root, index)`：
  - 稳定 ID；Unicode 文件名保留；删除进入 `audio/.trash/`。
- `MusicBank` 新增 `MusicAsset.path`；`rename` / `delete` 进入
  `music/.trash/`。
- 新 Action：`RenameAudio/DeleteAudio/RenameMusic/DeleteMusic`；
  `ActionScope::AssetEdit`；菜单新增 AUDIO/MUSIC 段。
- 新 ModalAction：`RenameAudio/ConfirmDeleteAudio/RenameMusic/ConfirmDeleteMusic`；
  删除被 SAMPLE 层引用的音频时确认文本提示引用关系。
- 删除模态提示修正为 `ENTER DELETE → .trash/`。

## 自审发现（已修）

1. `Wave::is_tone` 最初只认 `WaveFamily::Tone`，导致 CURVE 的 HQ 渲染落到
   `_ => Vec::new()`；改为 Tone | Curve，保持原有 DSP 路径。
2. `is_drum` 在 `layer_sections` 改用 family 后变成死代码，删除。
3. `godot_alias` 仅测试使用会被 dead_code 拒绝；导出 `write_layer` 增加
   debug 断言，让别名契约进入生产路径。
4. 音频/音乐重命名最初用 `model::sanitize`，中文名会变成 `sound`；
   新增 `model::safe_file_stem`，保留 Unicode、只剔除路径分隔符/控制字符。
5. 删除模态的 hint 只识别 `ConfirmDeletePreset`，AUDIO/MUSIC 会显示
   “DISCARD & OPEN”；已统一为 `.trash/` 删除提示。

## 第二审查发现

### Blocker

无。

### Major

1. **Sound/Arrangement 分离未完成**：这是 P5 最大项，需要 schema v5、
   `.tres`/Godot 插件同步与 parity golden 更新；本轮明确不做，避免破坏
   跨端一致性。迁移方案见 0042。
2. **删除引用音频不会清空 SAMPLE 层**：当前内存中的 `Arc<SampleData>` 仍可
   播放；重新打开工程时 `load_samples` 会 warning。确认文本已提示引用关系，
   但仍需后续“同时清理引用”或“阻止删除”的策略。
3. **Wave 注册表仍不是单点**：新增波形仍需 `WAVE_DEFS` + DSP 分派 +
   `from_godot_id` + `wave_color`；比之前少，但未完全收敛。
4. **手势测试只覆盖点击/单键**：拖拽、滚轮缩放、自动化点拖动仍靠状态级测试。
5. **AUDIO/MUSIC 删除没有恢复 UI**：文件在 `.trash/` 可手工恢复，但没有
   RESTORE 按钮。

### Minor

1. AUDIO/MUSIC 菜单段始终显示，未选中时灰显；可后续按焦点动态隐藏。
2. `safe_file_stem` 保留空格/Unicode，便于阅读；跨平台特殊字符仍可能受
   文件系统限制。
3. `MusicBank::rename` 只改文件名，不改变库内显示名与 folder 的语义。
4. `from_godot_id` 仍是手写 match；`godot_id` 的唯一性由测试保证。
5. `DeleteAudio` 的引用扫描只覆盖工程 patch 与预设，不扫描外部引用。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 199 passed
- [x] `cargo test --release` — 198 passed（debug-only 守卫测试排除）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `e563d3b6…`

新增测试：

- `wave_registry_covers_every_wave_and_documents_godot_aliases`
- `clicking_a_layer_reclaims_focus_from_a_preset_row`
- `clicking_the_music_tab_reclaims_preset_focus`
- `outside_click_closes_the_overlay_and_is_consumed`
- `cmd_z_shortcut_runs_through_the_command_table`
- `audio_rename_and_delete_keep_ids_and_use_trash`（含 Unicode 文件名）
- `music_rename_and_delete_keep_ids_and_use_trash`
- `audio_and_music_lifecycle_actions_need_a_selection`

## 残余风险与后续

- **v5**：Sound / Arrangement 分离、多 music、引用式 clip；需要独立迁移 PR。
- 资产 `.trash` 恢复 UI。
- 手势级拖拽/缩放测试框架。
- 实时 voice 引擎。

## 签署

- reviewer_1: approved
- reviewer_2: approved（详见 0042）
