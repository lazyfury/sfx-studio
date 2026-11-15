# Review 0026 — 右侧检查器滚动修复

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/ui.rs`, `src/ui/layout.rs`, `src/app/input.rs`, `src/app.rs`, `docs/PLAN.md`
- diff_base: `3e82031` (lazy-load preset samples and restore deleted presets)
- source_hash: b0dc3a5d208e9cdee5433b296f9527b27480be5bea5cbe47d73b48b79222e452
- status: approved

## 根因

1. **滚轮方向反了**：左侧库列表用 `delta.y > 0` 向上、`< 0` 向下；右侧
   inspector 却写成 `scroll - step * 22`，从顶部向下滚时算出负值并被 clamp
   回 0，看起来完全不能往下滚。
2. **最后一行少 4px**：内容从 `inspector.top() + 4` 开始，但滚动最大值只按
   `content_h - inspector.height` 计算；滚到底时最后一行 bottom 比面板 bottom
   低 4px，永远露不全。

## 变更

- 新增 `INSPECTOR_TOP_PAD = 4.0`，`Layout::inspector_max_scroll()` 统一计算
  `content_h + top_pad - height`。
- `Layout` 的 thumb、滚轮、拖动滚动条全部使用同一个 max。
- 新增 `inspector_wheel_scroll(current, delta_y, layout)`：向下滚增大 scroll，
  向上滚减小 scroll，并 clamp 到 max。
- 初始 item 位置改为 `inspector.top() + INSPECTOR_TOP_PAD - scroll`，与 max
  计算保持一致。
- 新增回归测试：
  - `inspector_wheel_scroll_moves_down_on_a_downward_wheel`
  - `inspector_scroll_reaches_the_last_row`

## 自审发现（已修）

1. 滚动条拖动和滚轮各自推导 max，容易再次漂移；现在统一走
   `Layout::inspector_max_scroll()`。
2. 只修方向不修 top inset，最后一行仍会被裁 4px；两者一起修。
3. 测试使用 1024×400 的短窗口强制内容溢出，不依赖真实窗口尺寸。

## 第二审查发现

### Blocker

无。

### Major

无。

### Minor

1. 不支持惯性滚动 / 滚动条轨道点击跳转；保持原有交互范围。
2. 滚轮方向和平台“自然滚动”设置相关；修复后与左侧库列表方向一致。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 150 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `inspector_wheel_scroll_moves_down_on_a_downward_wheel`
- [x] `inspector_scroll_reaches_the_last_row`
- [x] 工程 schema / DSP / 导出路径未改

## 残余风险与后续

- 真实触控板/鼠标滚轮仍需手测；逻辑已由纯函数回归测试锁定。
- 后续可增加滚动条轨道点击、参数组折叠、滚动位置持久化。

## 签署

- reviewer_1: approved
- reviewer_2: approved
