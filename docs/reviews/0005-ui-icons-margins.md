# Review 0005 — UI 打磨：主题色、时间线、面板标题、Lucide 图标与边距

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/theme.rs`, `src/ui.rs`, `src/icons.rs`, `assets/icons/*`, `scripts/source_hash.sh`, `docs/PLAN.md`
- diff_base: `7a5a8341afcc0380e4ce3b172bba303fa469ab50`
- source_hash: 7a4e939c868a06428db5bbeea730e4321557690504b10bd6796a311945fef715
- status: approved

## 变更摘要

- **主题色弱化 / 面板中性化**：`ACCENT` 由高饱和 `#B7F36B` 调整为柔和 `#A8CE86`；面板表面（BG/PANEL/RAISED/BORDER/HOVER/ACTIVE/TRACK_BG）全部改为中性石墨灰，不再叠加绿色；按钮激活态为中性灰底 + `ACCENT_LINE` 描边 + 主题色文字；Pattern 开启格用中性 `CELL_ON` + 顶部指示线；主题色只保留在指示/交互标记上。
- **时间线提亮**：新增 `TRACK_BG` 表面，标尺刻度线用 `BORDER_HI`、标签用 `DIM`；Clip 填充透明度 0.9→1.0、gain 下限 0.25→0.5；播放头时间标签加暗色底片，消除与标尺 `0.00` 叠字。
- **左侧标题栏**：新增与 `PARAMETERS` 一致的 26px raised 标题条（图标 + `LIBRARY` + 数量），`LIBRARY_TOP` 76→86，分类/搜索行随之下移。
- **收藏星标与标签重叠**：星标固定占右侧槽位（`right-20…right-10`），波形标签统一右对齐到 `right-26`，名称省略宽度按标签实测宽度计算。
- **面板边距**：参数面板内容再内缩 `INSPECTOR_PAD = 6`；选项按钮上下留 3px；中央标题卡按钮垂直居中并留右侧 6px；自动化 `GAIN/CUTOFF` 留边；滚动条移到 `right-9`。
- **Lucide 图标**：新增 18 个图标（lucide-static v1.47.0，ISC），用于顶栏动作、标签页、MENU、搜索框、系统菜单项、检查器按钮；图标与文字作为整体居中；<1260px 退化为纯图标。
- **标题/参数行**：`Layout.header` 成为唯一标题栏几何来源；标题与 meta 按「首个按钮左边界」省略，1024 宽不再压到按钮。
- **流程**：`scripts/source_hash.sh` 纳入 `assets/`，图标资源进入审查门禁。

## 自审发现（已修，含 2026-11-14 中性面板增补）

1. 加图标后检查器出现 `+ +LAYER`、`+ + CLIP` 双加号 → 标签改为 `LAYER` / `CLIP`。
2. `Layout.center` 在改用 `Layout.header` 后仅测试读取，触发 `dead_code` → 删除字段，紧凑布局测试改用 `header` 边界断言。
3. 1024 宽时 header meta 可能压到按钮 → `title_width` 统一截断，并按 `ellipsis` 绘制。

## 第二审查发现

### Blocker

无（代码路径）。流程性 Blocker 已在本记录写入前修复：PLAN 计数/阈值更新、审查记录补齐、source_hash 纳入 `assets/`。

### Major（已记录到 PLAN Backlog #11，下一迭代处理）

1. **拖动 Clip 时时间线比例随 `patch.end()` 变化**：`Layout::new` 的 `timeline_total` 由 clip 结束时间推导，Move/Resize 会改变比例，手感呈橡皮筋。建议在 `Drag::Clip` 冻结按下时的 scale，或引入独立 view span/zoom。
2. **收藏星标不可点击**：行内星标目前只是指示器，点击会选中/双击改名。建议新增 `Hit::PatchFavorite`，点击切换收藏。
3. **播放中每帧锁音频 mutex**：`audio.position()` 与音频回调共用 `Mutex<Playback>`，每帧 UI 可能被整段 buffer 阻塞。建议回调写 `AtomicU64` 位置，UI 无锁读取。
4. **时间线/波形高度不随内容自适应**：少量图层时时间线留大片空白、波形固定 128px。建议按图层数收缩时间线，把空间给波形/自动化。

### Minor

- `Icon::ALL` / `index()` / `svg()` 三张并行表，漏改会画错图或 `cache[icon.index()]` 越界 panic（`document()`）。建议 `get()` + `indices match ALL` 测试，长期改宏/单表。
- `ACCENT_TINT` / `ACCENT_LINE` 手写 RGB 与 `ACCENT` 重复，建议由共享 `ACCENT_RGB` 派生。
- `visible_patches()` 每帧 3 次分配；`Layout::new` 每帧重建多个 Vec，可按 `view_version` 缓存。
- Clip 吸附 `200.0`（5ms）硬编码；Move 未 clamp 到 `layer.duration`。
- 搜索框/分类/星标/检查器按钮高度 18–20px，鼠标可用但低于常规 24px 目标；主要动作文字 10px 偏小。
- macOS 原生标题栏与顶栏都显示 `SFX LAB`；可考虑 `TitlebarMode::Transparent` 或去掉顶栏品牌。
- 窗口尺寸/最近工程未持久化。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（48 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2537 commands / 118 text runs）
- [x] 实机启动 + 窗口截图核对：中性灰面板、图标按钮、左侧标题栏、星标/标签不重叠、检查器边距、播放头底片
- [x] `inspector_controls_keep_panel_margins`
- [x] `compact_layout_stays_inside_a_1024_window`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 图标表仍靠人工同步；新增图标必须同时改 enum / ALL / svg / index，建议下轮补一致性测试。
- 时间线比例与音频锁的两个 Major 需在下一迭代修；本次仅做视觉/布局，不改变音频与导出行为。
- 新图标资源已纳入 source_hash，但图标的三方许可元数据仍依赖 `assets/icons/LICENSE`。

## 签署

- reviewer_1: approved
- reviewer_2: approved
