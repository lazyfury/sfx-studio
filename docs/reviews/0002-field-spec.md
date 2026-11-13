# Review 0002 — 参数元数据单表与对数映射

- date: 2026-11-13
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/ui.rs`, `src/app.rs`, `docs/PLAN.md`
- diff_base: `154853e3ce3dba6210cc53d3834341a13c426900`（含 0001 之后的增量）
- source_hash: 3eeca416a2c94d215e5580acbda8787868fbf5fff2a36b3434220901311fadf7
- status: approved

## 变更摘要

- 新增 `Scale` / `FieldSpec`，成为 `LayerField`、`MusicField`、`ClipField` 的 label/unit/range/step/scale/format 唯一来源。
- `LayerField/MusicField/ClipField` 的 `label/range/format/norm/denorm` 全部委托给 `spec()`；删除三套重复 match（保留 get/set）。
- UI 参数条使用 `field.norm(value)`；鼠标点击使用 `field.denorm(t)`，因此频率类参数变成对数映射。
- Cutoff 范围从 12000 提升到 18000，与 Godot `SfxLayer` 的导出范围一致。
- 新增两个回归测试：对数中点/往返、cutoff 滑块中点。

## 自审发现（已修）

1. 初始测试使用 demo 的第一个 patch（Noise 层），inspector 没有 Cutoff 行 → 测试改为先放入 Sine 层。
2. `LayerField::unit()` 在格式化改为 `FieldSpec` 后失去调用方 → 删除。
3. `snap_to()` 被 `FieldSpec::snap` 取代 → 删除，避免两套吸附逻辑。

## 第二审查发现

### Blocker

无。

### Major

1. **对数映射改变了已有交互**：频率/截止滑块中点不再等于算术中点。已通过测试固定几何中点，并在 PLAN 记录行为变化。
2. **Cutoff 上限变化**：12000 → 18000，旧工程值不受影响（clamp 只限制滑块可达范围，`set` 仍允许旧值），但 UI 手感变化。已记录。
3. **`MusicField::format` 仍保留自定义格式**（`132 BPM`、`4 bars`），没有完全走 `FieldSpec::format`。这是有意的：单位前有空格且 Bars 需要 `bars` 后缀；`FieldSpec` 仍负责 range/norm。

### Minor

- `FieldSpec::snap` 在 log 域按值吸附，低端（20–100 Hz）分辨率仍受 step=1 限制，可接受。
- `LayerField::Arp*` 的 UI 最大值 5000 与导入 clamp 12000 不一致；后续统一。
- `inspector_items` 仍按波形手写分组，下一迭代改为 `visible_when` 表驱动。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（45 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 3010 commands / 148 text runs）
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）
- [x] 对数中点：cutoff 20–18000 的 0.5 → ≈600 Hz；线性 Gain 保持 0.5

## 残余风险

- 参数分组仍是 UI 手写，增加波形/字段时需要同步 inspector 列表。
- 对数滑块与 fine-drag 的增量仍按线性值计算，低端细调可以再优化。
- 其余 Backlog 不变（后台渲染、模块拆分、稳定 ID、跨端一致性、采样波形）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
