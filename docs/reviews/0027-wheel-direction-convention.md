# Review 0027 — 滚轮方向对齐 igui_core

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/input.rs`, `docs/PLAN.md`
- diff_base: `5479f40` (fix inspector scrolling direction and bottom reach)
- source_hash: 98589774deca48f402e1ce69e55551189f919d9b0b2e96e7a7fc95ecd9dcc371
- status: approved

## 根因

`igui_core::InputEvent::Wheel` 的约定是 **`y > 0` 表示向下滚动**；winit
适配层已经对 `MouseScrollDelta` 取反，本项目不应再取反一次。0026 把右侧
检查器方向改成了“正 delta 向上”，实际运行中表现为鼠标向下滚动时内容向上，
用户反馈“鼠标滚动反了”。

## 变更

- 新增 `wheel_steps(delta_y)`：
  - `> 0` → 正步进（向下）
  - `< 0` → 负步进（向上）
  - `0` → 0
- `inspector_wheel_scroll` 使用 `wheel_steps`，正 delta 增大 scroll。
- 左侧库列表滚轮改用 `wheel_steps`，向下滚增加 index。
- 时间线图层滚动改用同一方向；Shift+wheel 向下平移视图向右。
- 更新回归测试：
  - `inspector_wheel_scroll_moves_down_on_a_downward_wheel`
  - 直接断言 `wheel_steps(1.0) == 1`、`wheel_steps(-1.0) == -1`

## 自审发现（已修）

1. **方向不能凭界面直觉推断**：必须以 `igui_core` 文档/适配层为准；已统一
   到一个 helper，避免库列表、inspector、图层各自取反。
2. **保留 0026 的底部可达修复**：`INSPECTOR_TOP_PAD` 与
   `inspector_max_scroll` 不动，只改方向。
3. **Shift+wheel 平移方向**：与垂直滚轮方向一致；Cmd+wheel 缩放方向保持
   原行为，避免顺便改掉未反馈的交互。

## 第二审查发现

### Blocker

无。

### Major

无。

### Minor

1. 真实鼠标滚轮 / 触控板仍需手测；本环境只验证了方向 helper 与 layout。
2. Cmd+wheel 的缩放方向没有随之下调，仍保持 0026 之前的行为。
3. 惯性滚动与滚动条轨道点击跳转仍未实现。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 150 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `wheel_steps(1.0) == 1` / `wheel_steps(-1.0) == -1`
- [x] `inspector_wheel_scroll` 正 delta 增大、负 delta 减小
- [x] 工程 schema / DSP / 导出路径未改

## 残余风险与后续

- 需要真实窗口手测鼠标滚轮方向与触控板手感；后续可补 `ScrollDevice` 区分
  的加速度曲线。
- 建议下一轮处理退出拦截、Bake 后台任务、RT buffer 归还队列。

## 签署

- reviewer_1: approved
- reviewer_2: approved
