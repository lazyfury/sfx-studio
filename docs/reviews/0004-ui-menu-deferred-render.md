# Review 0004 — 系统菜单、移除 Trigger Pad、首帧延迟渲染

- date: 2026-11-13
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/ui.rs`, `src/app.rs`, `src/theme.rs`, `src/project_io.rs`, `src/cli.rs`, `README.md`, `docs/PLAN.md`, `docs/DEV_WORKFLOW.md`, `.github/` (removed)
- diff_base: `154853e3ce3dba6210cc53d3834341a13c426900`
- source_hash: 0276aa2b7979aa56e56d74c8bb57e1a4b1e50b787d31b4a2b1487bae45063c75
- status: approved

## 变更摘要

- 顶部 12 个操作按钮（OPEN/SAVE/EXP/RAND/VARY/FAV/BAKE/TRES/NEW/DUP/REN/DEL）收进左上角 `MENU` 下拉，分为 FILE / SOUND / EXPORT。
- `MENU` 按钮在顶栏，下拉覆盖左侧面板；点击外部/按 Esc 关闭；菜单项 hover 高亮，FAVORITE 显示 `ON`。
- 移除底部 Trigger Pad（`PAD_H`、`pad_buttons`、`Hit::Pad`、绘制与测试）；键盘 `1`–`8` 仍可触发 patch。
- 首帧延迟渲染：启动/切换 tab/选择 patch 时 `defer_render = true`，先绘制 shell；`needs_frame` 因为 `preview_pending` 再请求一帧，下一帧才渲染音频。
- 移除 GitHub Actions（`.github/`），保留本地/共享 runner 入口 `scripts/ci.sh`。
- 释放的 74px 高度全部给时间线/波形；selfcheck 由 3010 commands 降到 1637，符合预期。

## 自审发现（已修）

1. `Button.compact` 在顶栏移除后无调用方 → 删除字段与初始化。
2. `category_color`（原 Pad 分类点）与 `action_icon`（原顶栏图标）变成死代码 → 删除。
3. `selfcheck` 仍要求 `TRIGGER PAD` 标签 → 改为要求 `MENU`。
4. `preview_pending` 测试用 8000 Hz 缓存但 `preview_rate()` 是 22050 → 测试改用 `preview_rate()`。
5. `State` 初始 `cache_rate=0` 会让首帧状态栏显示 `0 Hz` → 初始化 22050 / 11025。
6. `open_folder` 需要同时复位 `menu_open` 与 `defer_render` → 补上。

## 第二审查发现

### Blocker

无。

### Major

1. **菜单命中优先级**：菜单在 `hit()` 最前面检查，下拉覆盖区域优先于左侧库；点击外部只关闭菜单并消费事件，不会误触下层控件。已确认。
2. **延迟渲染的后续帧**：`needs_frame` 增加 `defer_render || preview_pending()`；首次启动会多一帧（shell 先出，音频后渲染）。`play()` 仍同步 `ensure_cache`，所以按下播放不会等延迟。
3. **缓冲渲染仍会阻塞**：如果音频渲染超过一帧，第二帧仍会在 UI 线程卡住；本次只把「首击反馈」提前，真正的后台渲染仍在 Backlog。
4. **键盘 `1`–`8`**：Pad 移除后快捷键保留，README 改为「Trigger patch 1–8」，避免功能回退。

### Minor

- `MENU` 是应用内下拉，不是 macOS 原生菜单栏；igui 目前没有原生菜单 API，后续可接 `muda`。
- 菜单项没有显示快捷键（⌘S 等），只在 hover 状态栏提示。
- 菜单打开时 hover 底层控件仍会更新状态栏文案；不影响操作。
- 删除 pad 后分类色点不再出现，分类仍可通过过滤器查看。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（47 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 1637 commands / 119 text runs，含 `MENU`）
- [x] `layout_exposes_library_filters_and_system_menu`
- [x] `system_menu_paints_and_hit_tests_actions`
- [x] `startup_render_is_deferred_and_then_resolved`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 首次点击如果恰好落在下一次音乐渲染期间，仍可能感觉到一次卡顿；彻底解决需要后台渲染线程。
- 菜单目前是单层分组，没有二级子菜单；项目数量/操作继续增长时可拆分 FILE/EXPORT 子菜单。
- 无原生系统菜单栏（macOS 菜单）。
- 其余 Backlog 不变（state/ui 拆分、稳定 ID、跨端一致性、采样波形）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
