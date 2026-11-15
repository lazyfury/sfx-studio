# Review 0022 — 预设直接导出 WAV

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/commands.rs`, `src/export.rs`, `src/ui.rs`, `src/ui/paint.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `312931c0fa68c13e941f07b0391379c511ca151d`
- source_hash: 364b9afbb12317c058edba6fe7b5bb3199287ce644d4b99c3ac59ea959031c93
- status: approved

## 变更摘要

PRESETS 库中的预设现在可以直接生成 WAV，无需先拖入工程或时间线。

- **导出目标**：新增 `State::export_patch()`。PROJECT 模式返回当前 patch；
  PRESETS 模式返回 `selected_preset` 对应的预设。
- **命令入口**：`MENU → EXPORT WAV` 与 `⌘/Ctrl+E` 在 PRESETS 模式下不再被
  只读边界拦截；未选中预设时菜单置灰，快捷键给出“先选择一个预设”的提示。
  其他工程编辑 / Bake / `.tres` 命令仍保持禁用。
- **渲染路径**：新增 `export::write_patch_wav()`，复用 `fx::render_patch`
  输出与现有手导出完全一致的 48 kHz / 24-bit stereo TPDF WAV；只是把命令
  层重复的渲染 + 写文件代码收拢到一处。
- **UI 文案**：预设行提示、状态栏与工具提示改为说明 `⌘E` 可直接导出；
  不可用原因由 `action_unavailable_message()` 统一生成，区分“只读”与
  “未选预设”。
- **文档**：README 的 Library / Keyboard 段与 PLAN 第 24 节同步更新。

## 自审发现（已修）

1. **只读边界误伤导出**：原先 PRESETS 模式把所有 Export 命令一律禁用。
   现在只放行 WAV 导出，并显式要求 `selected_preset`，Bake/TRES 仍不可用。
2. **导出目标可能静默错位**：没有在命令里直接读 `patch()`，而是新增
   `export_patch()` 按当前库来源取目标，并加单测锁定行为。
3. **命令层渲染重复**：`write_patch_wav()` 收拢到 `export.rs`，CLI/GUI 的
   48k / 24-bit / TPDF 参数只保留一份。
4. **无选中预设时的提示**：tooltip 与状态栏不再显示“PRESETS 只读”，改为
   可操作的“先选中一个预设再导出 WAV”。

## 第二审查发现

### Blocker

无。

### Major

无。

### Minor

1. 导出依赖系统保存对话框，无法在无 GUI 的 headless 环境端到端手测；
   已用 `write_patch_wav` + 真实 WAV 头断言覆盖渲染与编码路径。
2. PRESETS 库仍只能逐个导出；批量导出整个预设库未在本轮范围内。
3. 预设导出不写 `sfx_manifest.json` / `.tres`，符合“只读预设 + 手工 WAV”
   的定位；需要 Godot 资源时应先拖入工程再 Bake/TRES。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 135 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] 新单测 `app::tests::wav_export_target_follows_the_active_library`
- [x] 新单测 `export::tests::preset_wav_export_is_48k_24bit_stereo`：
      RIFF / 2 channels / 48000 Hz / 24-bit
- [x] `ui::tests::preset_mode_disables_project_only_actions` 更新：
      未选预设不可导出、选中后可导出、PROJECT 模式仍可导出
- [x] 工程 schema、DSP、bake / `.tres` 路径未改

## 残余风险与后续

- 预设批量导出 / 预设 `.tres` 导出可作为后续独立项。
- 无 GUI 环境下保存对话框与真实鼠标操作未手测；逻辑与渲染路径由单测覆盖。

## 签署

- reviewer_1: approved
- reviewer_2: approved
