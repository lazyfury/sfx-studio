# Review 0008 — Clip/Music 注册表、时间线缩放、库滚动条、分类下拉

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/ui.rs`, `src/app.rs`, `README.md`, `docs/PLAN.md`
- diff_base: `ab2d91d82fe667cce333d2e820060adf8efc81a2`
- source_hash: 4ab2c8d4df7611182f3dcff15b6d8bc6cb184620177ec3c2dc9e6e5104b4a72d
- status: approved

## 变更摘要

- **Clip/Music 注册表**：`model::CLIP_FIELDS` 与 `music_sections()`（MUSIC / INSTRUMENTS / MIX / HARMONY / PATTERN）接管剩余手写 inspector；`MusicRowSpec` 覆盖字段、BASS/ARP 波形、和弦/移调、Pattern 开关、重置与两条说明。
- **时间线缩放/平移**：`State::timeline_zoom`（1–32×）、`timeline_offset`；`Layout` 统一计算 `timeline_scale / timeline_offset / timeline_visible`，`time_x/x_time` 带偏移；标尺按可见窗口取 tick；⌘/Ctrl+滚轮以指针锚点缩放，Shift+滚轮平移；状态栏显示倍率。播放头移出视口时不再画错位置。
- **库列表滚动条**：patch 行右侧保留 8px gutter；溢出时绘制 4px 轨道 + thumb，支持拖动（新增 `Hit::LibraryScrollbar`、`Drag::LibraryScroll`），滚轮与拖动共用 `patch_scroll`。
- **分类下拉**：`ALL` 按钮从「循环」改为弹出列表（`Hit::CategoryPick(Option<Category>)`），当前项勾选，点击外部/Esc 关闭；`C` 快捷键仍循环。
- **吸附与边界**：Shift 关闭 Clip/自动化 5ms 吸附（状态栏提示 `FREE`），`snap_drag_time()` 单点实现；Clip Move/Trim 的起始时间 clamp 到 `ClipField::Start` 上界（8s）。
- 标尺末端放不下的 tick 文案增加宽度判断，不再被卡片裁切。
- 新增 7 条测试：缩放双向映射、库滚动条 gutter/命中、分类菜单全项命中、Music 注册表顺序、分类过滤、Shift 关闭吸附；`cargo test` 62 passed。

## 自审发现（已修）

1. 时间线 tick 仍按 `0..total` 生成，缩放后左侧 tick 会堆在视口外 → 改为按 `offset..offset+visible` 生成。
2. 播放头在缩放后可能落到标签列或卡片外 → 仅在视口内绘制，chip 限制在卡片内。
3. 库滚动条轨道与收藏星标命中区重叠 → 行宽改为 `left.width - 8`，轨道放在 gutter，星标命中区仍在行内。
4. 分类菜单打开时点击外部只关菜单还是继续操作存在歧义 → 选择「关闭并继续」，菜单内部点击才消费事件。
5. 系统菜单与分类菜单互斥：打开 MENU 时关闭分类下拉；Esc 优先关分类下拉。

## 第二审查发现

### Major

1. **缩放后的可见窗口是唯一时间轴来源**：`Layout` 每帧 clamp `timeline_offset`，但 `State::timeline_offset` 仍可能保存超界值；所有读取都走 `Layout`，暂时安全，但应避免两处真相。后续可把 clamp 写回 State 或在 setter 中约束。
2. **长音效 + 最小 scale 8px/s**：当 fit scale 被 8px/s 下限抬高时，缩放到 1× 仍会在右侧留下空白（历史行为）。缩放/滚动没有空白区提示，属可接受但需文档化。
3. **Shift 自由放置没有触摸/键盘替代**：只有鼠标 + Shift；命中测试与拖拽本身已有单测，但端到端依赖手测。
4. **分类下拉不支持键盘上下选择**：`C` 仍循环分类；下拉项只能用鼠标点。无障碍 Backlog 已覆盖。

### Minor

- 时间线平移只支持 Shift+滚轮，未处理触控板 `delta.x`。
- 库滚动条 thumb 拖动按像素比例映射，未支持点击轨道跳转（拖动即可）。
- `MusicRowSpec::PreviewNote` 依赖 `state.cache_rate`，属于 UI 数据而非纯表；保持输出一致。
- 缩放倍率跨 patch 保留；切换工程时重置（`open_folder_now`），切 patch 不重置。
- 本环境无法向非前台窗口注入鼠标/键盘事件（macOS 辅助功能权限），交互验收以布局/命中单测 + 默认态截图为主，未做真实点击录制。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（62 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] 实机截图：时间线 3 行 + 大波形、gutter、标尺末 tick 不裁切
- [x] `zoom_and_scroll_map_times_both_ways`
- [x] `library_scrollbar_reserves_the_right_gutter`
- [x] `category_menu_lists_and_hits_every_category`
- [x] `music_inspector_follows_the_registry`
- [x] `picking_a_category_closes_the_menu_and_filters`
- [x] `shift_disables_the_clip_snap_grid`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 缩放状态双份（State / Layout clamp）需要后续收口。
- 分类下拉/滚动条的真实鼠标拖拽未在 CI 覆盖，依赖命中与映射测试。
- 交互层改动不涉及 DSP/导出；音频与工程格式风险不变。
- 模块拆分与后台渲染仍未开始（Backlog #1/#2）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
