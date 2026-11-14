# Review 0011 — State 字段分组（doc / view / playback / ui）

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/{input,commands,audio_bridge,render}.rs`, `src/ui/{layout,paint,hit}.rs`, `src/project_io.rs`, `docs/PLAN.md`
- diff_base: `619633d8fe93ac618735b69332f8cd5f229e6ea2`
- source_hash: 7d08e8cdde79d8734ae0010aa39aafb95e39dedda0696fde651b6338b747068d
- status: approved

## 变更摘要

纯结构调整，行为不变：

- `State` 从 50+ 个平铺字段拆为四组：
  - `DocState`：`project`、`root`、`dirty`、`version`、`music_version`、`undo`、`redo`、`pending_edit`。
  - `ViewState`：`tab`、选区、滚动、`category_filter`、`search`、`menu_open`、`drag`、`modal`、`hover`、`cursor`、`timeline_zoom/offset/pin`、`viewport`、`view_version`。
  - `PlaybackState`：`cache*` / `music_cache*`、`loop_play`、`playhead`、`seeked`、`was_playing`、`audio_ok/rate`、`last_render`、`defer_render`。
  - `UiState`：`status`、`toast`。
- 访问路径机械迁移（`state.project` → `state.doc.project`，`self.playhead` → `self.playback.playhead` 等）；`State` 上的方法签名保持不变，调用方只改了字段路径。
- `State::new` 改为嵌套初始化；未改任何逻辑、默认值或时间线/渲染行为。

## 自审发现（已修）

1. 机械替换把 `state.toast("...")` 方法调用误改成 `state.ui.toast(...)` → 全局恢复方法调用，仅保留字段读取的 `state.ui.toast`。
2. `undo()` / `redo()` 是 `State` 方法，不是 `DocState` 字段 → 回滚为 `state.undo()` / `state.redo()`。
3. 跨行访问（`self.state\n.selected_patch`）未被首次正则命中 → 用允许空白的模式重跑，编译器清零。
4. `impl State` 内的 `self.<field>` 只在该 impl 区间替换，避免误改其它结构体同名字段。

## 第二审查发现

### Blocker

无。

### Major

1. **访问路径变长**：`self.state.view.selected_patch`、`self.state.doc.project.patches` 等可读性下降；后续可在 `DocState` / `ViewState` 上实现领域方法（`select_patch`、`visible_patches`）把逻辑从 `State` 下沉，而不是只做字段分组。
2. **方法仍集中在 `State`**：`ensure_cache`、`select_patch`、`mark_dirty` 等仍跨组读写；下一步应把纯文档操作移到 `DocState`、播放状态操作移到 `PlaybackState`，减少 `State` 的上帝对象属性。
3. **稳定 ID 未包含**：本轮只做字段分组，`Patch/Layer/Clip.id` 与 schema v3 仍在 Backlog 第 1 项。

### Minor

- `UiState` 只有两个字段，但为后续焦点/提示/菜单状态留了位置。
- `view_version` 放在 `ViewState`，渲染 worker 的 `RenderKey` 仍使用 `version/music_version/patch`，语义未变。
- `state` 变量命名在 ui 子模块中仍指整个 `State`，读代码时需要记住组前缀。
- 没有为此纯移动新增测试；用现有 64 tests + selfcheck 计数一致性验证。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（64 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs，与 0010 一致）
- [x] 实机启动截图：界面/波形/交互与 0010 一致
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 纯字段迁移，无逻辑变化；DSP/导出/格式风险不变。
- 字段分组不等于行为分层；真正的领域方法下沉与稳定 ID 是下一步。
- 后续若用 IDE 重构，注意 `state.*` 与 `self.state.*` 两条访问路径。

## 签署

- reviewer_1: approved
- reviewer_2: approved
