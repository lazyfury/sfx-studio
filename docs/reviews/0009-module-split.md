# Review 0009 — app / ui 模块拆分

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/{input,commands,audio_bridge}.rs`, `src/ui.rs`, `src/ui/{layout,paint,hit}.rs`, `README.md`, `docs/PLAN.md`
- diff_base: `c282266d979e5ff710dacbbe80b34d6a53074f33`
- source_hash: 365bbf1dcee82f61c5bc682802edde4ddd3ae3ed93b92c0666cb0ac8813eaefd
- status: approved

## 变更摘要

纯结构调整，行为不变：

- `src/ui.rs` 3300 → 620 行：保留共享类型（`Action` / `Hit` / `RawItem` / `Item` / `Button` / `MenuEntry`）、常量、`action_icon` / `action_tip` 与测试。
- 新增 `src/ui/layout.rs`（634 行）：metrics/capacity 辅助、`Layout` 结构与其几何、`automation_tabs`、`pattern_rows` / `pattern_on`。
- 新增 `src/ui/hit.rs`（176 行）：全部命中测试。
- 新增 `src/ui/paint.rs`（1893 行）：所有绘制、drag/hover 提示与绘画辅助。
- `src/app.rs` 2488 → 891 行：`State`、`Tab`、`Drag`、`TimelineDrag`、`Modal`、顶层工具函数与测试。
- 新增 `src/app/input.rs`（963 行）：`impl AppLogic for SfxLogic`（事件/更新/绘制桥接/光标）与参数拖拽。
- 新增 `src/app/commands.rs`（587 行）：`SfxLogic::new`、activate、文件/导出、随机/Vary、收藏/分类过滤、modal 确认等。
- 新增 `src/app/audio_bridge.rs`（67 行）：play / seek / stop。
- 跨模块方法加 `pub(super)`；子模块 `use super::*;` 复用父模块导入。
- `ui.rs` 通过 `pub use` 保持 `ui::Layout` / `ui::hit` / `ui::paint` / `ui::layer_capacity` 等外部 API 不变。

## 自审发现（已修）

1. 首次按行号切分时 `mod input;` 在 app.rs 出现两次，导致同一文件被编译两次（重复 `impl AppLogic` / `new` 等）→ 合并为顶部一次声明并加入 `mod commands; mod audio_bridge;`。
2. `set_param_from_x` 与 input 内 `handle_drag` 拆分后测试访问不到 → 分别标记 `pub(super)` / 保留私有。
3. `ui` 根模块曾重新导出未使用的 `body_height` / `timeline_height*` 触发 `unused_imports` → 只导出外部实际使用的 metrics。
4. `pattern_rows` / `pattern_on` 同时被 layout 与 paint 使用 → 放进 `ui/layout.rs` 并 `pub(super)`。
5. `inspector_items` 由 layout 调用、根测试使用 → `pub(crate)` 并在测试中显式导入。

## 第二审查发现

### Blocker

无。

### Major

1. **层次依赖**：`ui/layout.rs` 依赖 `ui/paint::inspector_items` 来构建 inspector 几何，layout → paint 不是理想方向。后续可把 inner items 构造移到 `ui/layout` 或独立 `ui/inspector.rs`，paint 只消费 `Layout.items`。
2. **State 仍是单体**：本轮只拆文件，`State` 仍混合 doc/view/playback/ui 字段；Backlog #1 的后半部分未完成。
3. **paint.rs 仍偏大**（1893 行）：可继续按 library / timeline / inspector / modal 拆分，但属于可选优化。

### Minor

- 子模块统一 `use super::*` 提升迁移速度，但隐式依赖父模块导入；后续可改为显式导入降低耦合。
- `snap_drag_time` / `param_row_rect` 留在 app 根，属于 input 辅助函数，可随 input 一起下沉。
- `commands.rs` 里仍有 modal 确认（`handle_modal` 在 input、`confirm_modal` 在 commands），后续可归并到 `app/modal.rs`。
- 大文件移动使 diff 很大，review 以「文件是否纯移动 + 行为是否不变」为准；本轮已核对函数集合与测试计数一致。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（62 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs，与拆分前一致）
- [x] 实机截图：默认界面与 0008 一致
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 纯文件拆分，无逻辑变更；音频/导出/格式风险不变。
- `pub(super)` 暴露面增加，但仅限 crate 内部模块。
- layout→paint 的 `inspector_items` 依赖需在 State 拆分时一并理顺。

## 签署

- reviewer_1: approved
- reviewer_2: approved
