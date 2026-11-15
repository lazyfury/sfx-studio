# Review 0029 — CURVE 通用波形 / 曲线编辑器

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/synth.rs`, `src/export.rs`, `src/ui.rs`, `src/ui/layout.rs`, `src/ui/paint.rs`, `src/ui/hit.rs`, `src/app.rs`, `src/app/input.rs`, `docs/PLAN.md`, `docs/FORMAT.md`, `README.md`
- diff_base: `bb2c0af` (add Chinese parameter subtitles and wave guide)
- source_hash: b8e068394b6b6141bb4f6e2e265041c72b4b1ee821caa805c9b4f0da3c19f4b4
- status: approved

## 变更摘要

- **新声部**：`Wave::Curve`，一个不替代现有算法的通用可绘制振荡器。
- **数据**：`Layer.curve`（单周期 -1…1，最多 64 点）和
  `Layer.amp_curve`（音量 0…1，最多 64 点）。`serde(default)` 追加，不升
  schema；`sanitize()` 对 NaN/越界 clamp、截断 64 点。
- **DSP**：`cycle_value` 线性插值；v3 以 2× 渲染后走既有半带滤波抽取；
  `amp_curve` 非空时覆盖 ADSR / Exp 音量包络，频率、颤音、滤波、Pan、Space
  继续复用现有链。
- **编辑器**：右侧 `CURVE` 画布（单周期）和 `AMP CURVE` 画布（32 点），
  拖动绘制、双击重置。固定 32 点分辨率，不做任意横坐标控制点。
- **显示**：CURVE 加入 WAVE 选项、中文说明、WAVE TYPES 和时间线颜色。
- **导出**：`Patch::needs_baked_tres()` 包含 CURVE；`.tres` 整段 bake 到
  WAV，因为 Godot 的 live `.tres` 无法表达自绘曲线。

## 自审发现（已修）

1. **曲线与参数链解耦**：CURVE 没有另造一套频率/包络/滤波参数，而是复用
   Layer 的 PITCH / AMP ENV / FILTER；避免两套 DSP 参数漂移。
2. **空数据回退**：`curve` 为空回退正弦；`amp_curve` 为空回退当前 ADSR/Exp，
   保证切换到 CURVE 时不会突然静音。
3. **命中与绘制同源**：`curve_point` / `curve_index` /
   `curve_value_from_y` 被 paint 和 input 共用；拖动不会出现“画到的点和改到
   的点不是同一个”。
4. **Undo 事务**：pointer-down `begin_edit`，拖动期间 `mark_dirty`，pointer-up
   `commit_edit`；一次绘制只进一条 undo；双击重置也先 push_undo。
5. **导出安全**：CURVE 不能映射到 Godot live 参数，`needs_baked_tres` 强制
   整段 bake；已有 v3/自动化/采样 bake 路径复用。
6. **参数面板兼容**：CURVE 有独立的 curve section，不污染其他波形的字段
   可见性；TONE_PITCH / TONE_AMP / TONE_FILTER 直接复用。

## 第二审查发现

### Blocker

无。

### Major

1. **固定 32 点**：v1 不做任意横坐标控制点和点数增减；复杂波形需要导入
   WAV 或等后续版本。已在 PLAN 非目标中写明。
2. **抗混叠有限**：CURVE 采用 2× + 现有半带滤波，不是按曲线谐波生成
   mipmap；尖锐折线的极高次谐波仍可能有少量混叠。
3. **Godot 侧无 live CURVE**：`.tres` 只通过 baked WAV 播放；Godot 编辑器
   里不能继续改曲线。

### Minor

1. 没有独立的“重置为 Sine / 从当前 ADSR 生成”按钮；双击画布就是重置。
2. 没有音高曲线 / 滤波曲线；当前只有单周期波形 + 音量曲线。
3. 曲线点之间是线性插值，没有 Catmull-Rom 平滑选项。
4. 真实窗口拖动画布的鼠标手感未手测；由纯映射测试和 headless 覆盖。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 156 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2584 commands / 138 text runs
- [x] `./scripts/preflight.sh`
- [x] `curve_and_amp_curve_roundtrip_and_clamp`
- [x] `curve_wave_uses_the_drawn_cycle_and_amp_curve`
- [x] `curve_wave_exposes_editable_canvases`
- [x] `curve_patches_bake_on_tres_export`
- [x] `wave_guide_lists_every_wave_type`
- [x] 工程 schema 仍为 v4；旧 JSON 缺失 curve 字段时默认空数组

## 残余风险与后续

- 下一步可做：任意控制点/点数、Catmull-Rom 平滑、音高/滤波曲线、单周期
  WAV 导入、CURVE 预设示例。
- 仍需处理：退出拦截、Bake 后台任务、RT buffer 归还队列。
- 真实 UI 曲线拖动手感需要窗口环境手测。

## 签署

- reviewer_1: approved
- reviewer_2: approved
