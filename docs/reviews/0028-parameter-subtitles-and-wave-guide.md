# Review 0028 — 参数中文副标题 / 波形指南

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/ui.rs`, `src/ui/layout.rs`, `src/ui/paint.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `4d53c85` (align wheel direction with igui_core)
- source_hash: d467fbfa425652f3da9e3be8dbb685693a396d6b3465c80063ae46ea1e6acb05
- status: approved

## 变更摘要

- **参数中文副标题**：`LayerField` / `PatchField` / `ClipField` / `MusicField`
  新增 `desc()`；`RawItem::Param` / `Item::Param` 携带副标题。
  参数行高 26 → 38px，滑块轨道下移到 `top+30`，文字与控件不重叠。
- **当前波形解释**：WAVE 选项下方显示 `SINE：...` 一行，内容来自
  `Wave::desc()`。
- **WAVE TYPES 指南**：PATCHES 面板底部列出 14 种波形（SINE/TRI/SAW/PULSE/
  SQR/NOISE/ARP/FM/PLUCK/SAMPLE/KICK/SNARE/HAT/CLAP）的中文说明。
- **Hover 提示**：`Action::SetWave(wave)` 的 tooltip 直接显示该波形说明。
- **测试**：新增参数副标题与波形指南两个回归测试。

## 自审发现（已修）

1. **滑块命中区随行高变化**：`param_track` 同步下移到 `top+30`，点击跳转、
   拖动和双击复位仍走同一几何；没有出现“文字挡住滑块”的命中错位。
2. **描述文本来源统一**：参数说明挂在各 `Field` 上，不在 paint 里散写；
   Wave 说明挂在 `Wave::desc()`，tooltip 和指南共用。
3. **中文截断**：副标题走 `ellipsis`，过长的说明会以省略号收尾，不会压到
   右侧数值或面板边缘。
4. **内容变长后的滚动**：0026/0027 的 inspector max-scroll 修复覆盖新增
   14 行指南；`inspector_scroll_reaches_the_last_row` 仍通过。

## 第二审查发现

### Blocker

无。

### Major

1. **指南常驻**：14 行 `WAVE TYPES` 在每个 patch 的检查器底部常驻，普通参数
   编辑需要多滚一点。当前按“可发现性优先”接受；后续可做可折叠 section。
2. **文案硬编码**：中文说明直接写在 Rust 代码里，未走 i18n 资源；符合当前
   项目的轻量策略，但后续要资源化时需迁移。

### Minor

1. 参数副标题没有单独的可点击帮助图标；只随行显示。
2. `WAVE TYPES` 只覆盖 `Wave`，不覆盖 `MusicWave`（BASS/ARP 的 5 种波形）。
3. 极窄窗口下中文副标题可能被省略为较少字符；现有 1024px 布局测试通过。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 152 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2581 commands / 137 text runs
- [x] `./scripts/preflight.sh`
- [x] `inspector_parameters_have_chinese_subtitles`
- [x] `wave_guide_lists_every_wave_type`
- [x] `inspector_scroll_reaches_the_last_row` 仍通过
- [x] 工程 schema / DSP / 导出路径未改

## 残余风险与后续

- 真实窗口下中文副标题与 WAVE TYPES 的观感需手测；可用折叠/搜索降低噪音。
- 下一轮可考虑：可折叠参数组、MusicWave 指南、i18n 资源化。
- 仍待处理：退出拦截、Bake 后台任务、RT buffer 归还队列。

## 签署

- reviewer_1: approved
- reviewer_2: approved
