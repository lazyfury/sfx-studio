# Review 0013 — ID 化选区

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/{input,commands}.rs`, `src/project_io.rs`, `src/ui/{paint,hit}.rs`, `docs/PLAN.md`
- diff_base: `3f36a194583630d9eb1197bf24de28450772c480`
- source_hash: 3df4185a9f9f4a6d69c08454858507cff294997cc04221688df01c1edaee58c5
- status: approved

## 变更摘要

- `ViewState` 的 `selected_patch` / `selected_layer` / `selected_clip` 从索引改为 `Option<u32>` 稳定 ID。
- 新增解析辅助：
  - `patch_index()/patch()/patch_mut()`
  - `layer_index()/layer()/layer_mut()`
  - `clip_index()/clip()/clip_mut()`
  - `select_clip(index)`；`select_patch/select_layer` 仍按行索引调用，内部写入 ID。
- 所有直接比较/查找改为 ID：
  - 库行高亮（`Some(patch.id) == selected_patch`）
  - timeline 行/Clip 高亮（`selected_layer == Some(layer.id)`、clip id）
  - inspector 的 LAYER / CLIP 段
  - automation 命中、绘制、hover 信息
  - 后台渲染 `RenderKey`（0012 已改为 patch id）
- 结构编辑顺序统一为「先改文档 → `mark_dirty()` 分配 ID → 按索引选中」；timeline 点击 materialize clip 时使用 `ensure_ids()`，避免仅选择就 bump 版本/重渲染。
- `select_patch` 在新 patch 中找到原 layer ID 时保留，否则按之前的 layer 槽位 clamp，保持旧交互。
- 新增测试：
  - `selection_uses_stable_ids_across_reorders`
  - `deleting_the_selected_patch_selects_its_neighbour_by_id`

## 自审发现（已修）

1. `selected_patch/layer` 改动导致大量命令层索引运算失效 → 逐处改为 `patch_index()/layer_index()/clip_index()`，结构编辑后先 mark_dirty 再选中。
2. 自动化点删除/命中仍用索引型选中 → 改为 `clip_index()/clip()`，`selected_env_point` 保留点索引（点没有 ID）。
3. `paint.rs` 中 LAYER/CLIP 高亮、automation hover 仍按索引比较 → 改为比较 ID。
4. `open_folder` 重置为 `None` 后需要重新选中 → 末尾调用 `select_patch(0)` / `select_layer(0)`。
5. 测试里直接 `selected_clip = Some(0)` 语义变化 → 改为 `mark_dirty()` + `select_clip(0)`。

## 第二审查发现

### Blocker

无。

### Major

1. **瞬时 drag 仍携带索引**：`Drag::Clip` / `Drag::ClipField` / `Drag::AutomationPoint` 保存的是手势开始时的 layer/clip 索引。手势期间文档结构不应变化，因此可接受；若后续支持手势中插入/删除，需要改成 ID。
2. **`selected_env_point` 仍是索引**：automation 点没有稳定 ID，删除点或增删点会依赖 `clamp_selection`/清空。Backlog 已记录。
3. **`patch_index/layer_index/clip_index` 是 O(n) 查找**：当前列表很小；大规模 patch（512）下 UI 每帧多次调用仍可接受，但绘制循环内尽量直接用行数据比较 ID（已如此）。

### Minor

- `select_patch` 保留 layer 槽位的逻辑依赖切换前 `layer_index()`，语义与旧行为一致但需要测试覆盖（已有 reorder 测试）。
- `mark_dirty` 在删除 patch 时也会 `ensure_ids`，属于无害的全量扫描。
- 导出/manifest 仍未暴露稳定 ID；对 Godot 消费方无影响。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（68 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] `selection_uses_stable_ids_across_reorders`
- [x] `deleting_the_selected_patch_selects_its_neighbour_by_id`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 手势期间若结构变化，瞬时索引可能失效；当前交互不支持该场景。
- 自动化点选择仍为索引，增删点后依赖清空/校验。
- 选区 ID 化不改变音频/导出行为。

## 签署

- reviewer_1: approved
- reviewer_2: approved
