# Review 0023 — 预设易用性 / 模式边界 / 采样内存

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/audio_bridge.rs`, `src/app/commands.rs`, `src/app/input.rs`, `src/model.rs`, `src/synth.rs`, `src/export.rs`, `src/ui.rs`, `src/ui/hit.rs`, `src/ui/layout.rs`, `src/ui/paint.rs`, `src/wav.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `364b9afbb12317c058edba6fe7b5bb3199287ce644d4b99c3ac59ea959031c93`
- source_hash: f5d62b50da1af7d6e40b122def55ef736a3445c9405a211c711657f04e8b3c1f
- status: approved

## 变更摘要

本轮把上一轮审查中的 P0/P1 项落成可验收的小步改动：

- **空 PRESETS 自救**：左侧空库显示 `INSTALL BUILTIN PRESETS`，调用
  `PresetBank::install_missing`，写入 11 个内置 family；已有文件不覆盖。
  若目录里只有坏 JSON，会提示检查文件而不是误报“已安装”。
- **PRESETS 只读边界**：右侧检查器在 PRESETS 模式显示选中预设的只读摘要；
  增删层/Clip、采样、`UPGRADE TO HD` 等工程编辑命令禁用。
- **Space 语义**：PROJECT 模式仍是播放/暂停当前 patch；PRESETS 模式试听
  选中预设，未选中时提示先选择。
- **SAVE AS PRESET**：`MENU → SAVE AS PRESET` 把当前 patch 写入
  `<project>/presets/`，同名不覆盖。
- **采样内存**：`Layer.sample_data` 改为 `Option<Arc<SampleData>>`，undo、
  预览、`with_pitch` 和拖入复制共享 PCM；新增 64MB 源文件上限与
  60 秒 @48kHz 解码帧上限。
- **ID 修复复杂度**：`assign_id`/`ensure_ids` 从 `Vec::contains` 改为
  `HashSet`，避免大工程的 O(n²) ID 修复。

## 自审发现（已修）

1. **只读边界不完整**：只禁用了 New/Rename/Delete 等命令，`AddLayer` 等
   仍能改工程。现已把层、Clip、采样、DSP 升级命令一起禁用，并把右侧
   inspector 替换为只读预设摘要。
2. **空库按钮可能误报**：`install_missing` 返回 0 时可能只是存在损坏文件。
   现在会重新加载并在库仍为空时提示检查 `presets/` JSON。
3. **采样上限只在 decode 阶段**：`fs::read` 仍可能先分配超大文件。现增加
   64MB 元数据检查，在读取前拒绝。
4. **Arc 迁移后的测试**：所有直接构造 `SampleData` 的测试改为
   `Arc::new(...)`，并新增 `Arc::ptr_eq` 断言锁定“不复制 PCM”的行为。
5. **Space 回归**：把播放切换收拢到 `toggle_play()`，避免 PRESETS 模式
   继续播放工程 patch。

## 第二审查发现

### Blocker

无。

### Major

1. **退出拦截仍未实现**：宿主 `AppLogic` 没有关闭请求回调，本轮保留为
   Backlog；用户直接关窗仍可能丢失未保存改动。
2. **PRESETS 预设采样仍不解析 `sample_path`**：`PresetBank::load_project`
   只做 JSON/validate；带 `Wave::Sample` 的预设仍可能静音。Arc 化只解决
   内存复制，不解决预设采样文件解析。

### Minor

1. `SAVE AS PRESET` 使用当前 patch 名称，没有单独的命名对话框；同名预设
   会直接拒绝而不是提示覆盖/改名。
2. `INSTALL BUILTIN PRESETS` 只补 11 个 builtin，不补 11 个手调 demo
   预设；与新工程 22 个预设的初始状态不完全一致。
3. 预设重命名/删除、批量导出、Bake 后台任务仍未实现。
4. `ChordPresets` 的 active 高亮仍是硬编码，未在本轮修正。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 142 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `empty_presets_offer_an_install_action`
- [x] `installing_presets_fills_an_empty_bank_without_a_project_file`
- [x] `preset_mode_inspector_describes_the_selected_preset`
- [x] `preset_mode_disables_project_only_actions`
- [x] `space_in_presets_targets_the_selected_preset`
- [x] `saving_the_current_patch_adds_a_project_preset`
- [x] `sample_fields_roundtrip_and_load` 中的 `Arc::ptr_eq`
- [x] `rejects_oversized_files_before_reading`、`rejects_samples_over_the_frame_limit`
- [x] 工程 schema 仍为 v4；DSP / bake / `.tres` 路径未改

## 残余风险与后续

- 下一轮优先：关闭拦截 / Save-Discard-Cancel；PRESETS 采样路径解析；
  预设重命名/删除；Bake 后台任务。
- 真实窗口下的空库按钮、Space 试听、保存预设未手测（本环境无 GUI）；
  由 layout/hit、inspector、command 单测与 headless selfcheck 覆盖。

## 签署

- reviewer_1: approved
- reviewer_2: approved
