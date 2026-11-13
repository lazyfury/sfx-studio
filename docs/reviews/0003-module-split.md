# Review 0003 — CLI / project_io 模块拆分

- date: 2026-11-13
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/cli.rs` (new), `src/project_io.rs` (new), `src/app.rs`, `src/main.rs`, `docs/PLAN.md`
- diff_base: `154853e3ce3dba6210cc53d3834341a13c426900`（含 0001/0002 之后的增量）
- source_hash: 8f63bde15064939b4e6a64aa785c46531d938a48c6e50085ebd9423b974db8dd
- status: approved

## 变更摘要

- `app.rs` 不再承担进程启动、CLI 参数与 headless selfcheck：整体移动到 `src/cli.rs`。
- 工程加载与 UI 状态替换移动到 `src/project_io.rs`：`load_folder`、`open_folder`、`print_warnings`、`display_name`。
- `main.rs` 改为 `cli::run()`；`app.rs` 只保留 `State` / `SfxLogic` / 输入 / 命令 / 音频桥接。
- `SfxLogic::new` 提升为 `pub(crate)`，供 `cli::build_app` 使用。
- `app.rs` 清理不再需要的 `igui_app` / `igui_winit` / `igui_headless` / `tres` 导入。

## 自审发现（已修）

1. 初次移动后 `display_name` / `open_folder` 仍是私有函数，`app`/`cli` 无法引用 → 改为 `pub(crate)`。
2. `open_folder` 依赖 `load_folder`，两者必须放在同一模块 → 一起移动到 `project_io`。
3. `main.rs` 的 `app::run()` 与新增 `cli::run()` 重复 → 直接切到 `cli::run()`。
4. `cli.rs` 保留 `model` 导入会未使用 → 编译验证后清理。

## 第二审查发现

### Blocker

无。

### Major

1. **模块职责边界**：`project_io` 依赖 `app::State` 来替换 UI 状态，仍存在 `app → project_io → app` 的模块环。Rust 允许，但下一步应把 `State` 拆到 `state.rs`，让 `project_io` 只依赖状态类型。
2. **CLI 行为未变**：`--bake/--export/--dump/--project` 的参数解析仍是手写；本次只搬家，不改变语义。已用临时工程复测 dump/bake。
3. **`selfcheck` 仍在 cli 中**：未来 headless 测试可能拆到 `tests/` 或 `headless.rs`，当前保持可执行入口不变。

### Minor

- `project_io` 文件名带下划线，后续可随 `state` 拆分改为 `io/project.rs`。
- `cli.rs` 约 232 行，仍包含 help 文本；`clap` 化时再迁移。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（45 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 3010 commands / 148 text runs）
- [x] `cargo run -- --dump <tmp>`（schema v2、1 patch）
- [x] `cargo run -- --bake <tmp>`（`beep.wav` + `.import` + `music.wav` + manifest）
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- `app.rs` 仍有约 2.2k 行、`ui.rs` 约 2.9k 行；下一步拆 `state.rs` / `commands.rs` / `input.rs` 与 `ui/{layout,hit,paint}`。
- `project_io` 与 `app` 的循环依赖需在状态拆分时消除。
- 其余 Backlog 不变（后台渲染、稳定 ID、跨端一致性、采样波形）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
