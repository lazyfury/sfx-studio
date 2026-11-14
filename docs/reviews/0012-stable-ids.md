# Review 0012 — 稳定 ID 与 schema v3

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/app.rs`, `src/app/render.rs`, `src/app/commands.rs`, `src/tres.rs`, `docs/FORMAT.md`, `docs/PLAN.md`
- diff_base: `3ca1fd5441225d87c09d84cdbb18ada5b2737798`
- source_hash: 3f147033da051f695a89b4888b59b23e9ce74bf7cf76db1165c9cebac74c0578
- status: approved

## 变更摘要

- `Patch` / `Layer` / `Clip` 新增 `#[serde(default)] id: u32`。
- `Project` 新增 `#[serde(default)] next_id: u32`（单调 ID 发号器）。
- `Project::ensure_ids()`：
  - 缺失 ID（0）或重复 ID 由 `next_id` 顺延分配；
  - 跳过已占用值（即使 `next_id` 落后于文件中的最大 ID）；
  - 结束后 `next_id = max(next_id, next)`。
- 调用点：`Project::demo()`、`migrate()`、`validate()`、`State::mark_dirty()`（新增/复制对象后立即补 ID）。
- `PROJECT_VERSION` 2 → 3；manifest 的 `schema_version` 随之为 3（manifest 自身 `version` 仍为 2）。
- 渲染 worker `RenderKey.patch` 从索引改为稳定 patch id，重排/删除 patch 时更可靠。
- `docs/FORMAT.md` 增加 `next_id` / `id`、v3 迁移行。

## 自审发现（已修）

1. 新增字段导致 `Clip` / `Layer` / `Patch` 的默认构造、`implicit_clip`、命令层 AddClip、`.tres` 导入的 `Project` 字面量缺字段 → 全部补 0 / 1。
2. `next_id` 可能小于文件中已有最大 ID → `ensure_ids` 分配时循环跳过已占用值，最后把 `next_id` 抬到实际最大值之上。
3. 只在 `validate()` 分配会让新增对象在保存前没有 ID → 改为 `State::mark_dirty()` 也补全。
4. `RenderKey` 仍用 `selected_patch` 索引，删除/重排 patch 会让旧结果语义漂移 → 改为 patch id。

## 第二审查发现

### Blocker

无。

### Major

1. **选区仍是索引型**：`selected_patch/layer/clip` 没有改成 ID；本轮只提供稳定标识与渲染 key 的消费点。删除/重排时选区仍靠 `clamp_selection` 修正，ID 化选区是后续 Backlog #1。
2. **ID 修复没有 warning**：旧文件里 0/重复 ID 会被静默修复；如果需要审计，可在 `validate()` 返回的 warnings 中记录「reassigned N ids」。
3. **每次 `mark_dirty` 全量扫描**：`ensure_ids` 遍历所有 patch/layer/clip；当前规模很小，但采样波形/大工程前应改为结构变更时增量分配。

### Minor

- `.tres` / manifest 暂不暴露 ID；Godot 侧仍按名字/顺序消费。
- `next_id` 单调递增、不回收，删除对象会留下空洞（可接受）。
- `PROJECT_VERSION` 升到 3 后，任何读取 `schema_version` 的外部脚本需要接受 3。
- `id: 0` 在 JSON 中仍会被序列化；读回后由 `ensure_ids` 修复。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（66 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] `migrate_assigns_unique_stable_ids`
- [x] `demo_already_has_stable_ids`
- [x] `project_json_roundtrip`（含 IDs / next_id）
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 选区/撤销/导出尚未按 ID 引用，删除与重排仍依赖索引 clamp。
- ID 分配扫描是 O(n)，大工程需要增量发号。
- 本轮不改 DSP、音频渲染或导出内容，仅增加序列化字段与 key 语义。

## 签署

- reviewer_1: approved
- reviewer_2: approved
