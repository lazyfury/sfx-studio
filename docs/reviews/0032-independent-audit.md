# Review 0032 — 独立审查：统一 Library / 预设文件夹 / 交互回归

- date: 2026-11-15
- author: pi (independent audit pass)
- reviewer_1: pi (author fixes)
- reviewer_2: pi (adversarial review pass)
- scope: 当前工作树（0030 + 0031）：`src/library.rs`, `src/presets.rs`, `src/model.rs`, `src/app.rs`, `src/app/{commands,input,audio_bridge}.rs`, `src/ui.rs`, `src/ui/{layout,paint,hit}.rs`, `src/project_io.rs`, `src/cli.rs`, `src/demo.rs`, `docs/{PLAN,FORMAT}.md`, `README.md`
- diff_base: `753c41c` (add CURVE wave with editable cycle and amp curves)
- source_hash: f350ba0a0421463a0897712e8b029e70a45673f0d2ba27d281b8a6389c969082
- status: approved

## 审查范围与方法

按 `docs/REVIEW_CHECKLIST.md` 逐项检查，重点为：

1. 统一 library 的行身份、筛选、滚动、键盘导航；
2. 预设递归目录的读写、删除/恢复路径安全；
3. AUDIO 试听/拖入时间线的采样率、层上限与 undo；
4. music 行点击、Space 语义、只读/导出目标；
5. 旧平铺预设与旧 `.trash` 的兼容；
6. 文档、盘点与门禁哈希。

## 发现并修复

### Blocker

无。

### Major（已修复）

1. **方向键会开始试听/载入**：`navigate_library` 最初调用
   `activate_library_row`，按 ↑/↓ 会试听 AUDIO/PRESET，甚至载入 demo music，
   与“浏览”语义冲突。改为 `select_library_row` 只移动选中项。
2. **类型筛选与选中/导出目标失配**：切到 SOUND/PRESET/AUDIO/MUSIC 过滤时，
   旧的 `selected_library`/`library_mode` 可能仍指向被隐藏的行；例如选过
   preset 后切 SOUND，右侧仍显示 preset inspector，或 `export_patch` 取错目标。
   `pick_kind` 现在同步 mode + selected key，并自动选中当前过滤的第一个可见行；
   `export_patch` 优先按 `selected_library == Preset` 解析预设。
3. **列表缩短后滚动位置可隐藏所有行**：删除预设/折叠目录后
   `library_scroll` 可能大于新的 `max_start`，layout 只 clamp 到
   `len-1`，可能只显示一行或空列表。Layout 改为 clamp 到
   `len.saturating_sub(capacity)`，新增 `stale_library_scroll_cannot_hide_every_row`。
4. **折叠含选中项的目录会立即弹开**：`toggle_library_folder` 曾调用
   `ensure_library_visible`，后者会为了让选中项可见而解除折叠。现在折叠只
   clamp scroll，不再改 folded 集合；测试覆盖“选中的 impact 目录仍可折叠”。
5. **AUDIO/MUSIC 选中后仍保留 Presets 模式**：先选 preset 再点 audio/music 时
   `library_mode` 仍为 Presets，菜单不可用提示/空预设按钮可能错位。现在
   `select_audio` / `select_music` 都回到 Project 模式；只读由 selected kind
   统一判断。
6. **折叠状态跨工程残留**：打开新工程时只清筛选/选区，未清
   `folded_folders`，同名 `impact` 目录会在新工程里保持折叠。`open_folder`
   现在清空折叠集合。

### Minor

1. `AudioBank.root` 字段目前只写不读；`load_project` 返回的空 warning Vec
   可以后续简化为 `()`。
2. AUDIO 与 PRESET 的文件夹标题都只显示相对路径；若两类的目录同名（如
   `impact`），标题看起来会重复。后续可加 `WAV ·` / `PRE ·` 前缀。
3. 用方向键选中 demo music 但未点击时，Space 仍播放当前 `Project.music`，
   不会载入选中的 demo track；需要“试听模板”语义时再单独设计。
4. 文件夹计数是当前筛选后的匹配数，不是磁盘总数；文档已说明。
5. 折叠状态不持久化；GUI 内没有新建/移动文件夹命令。
6. 旧平铺预设不会自动迁移到分类目录。

## 正确性 / 数据安全核对

- [x] 预设递归扫描跳过隐藏目录与 `.trash`；旧平铺文件仍可读。
- [x] `write_presets` / `install_missing` 使用 `write_atomic`，按名称与目标
      相对路径去重。
- [x] 删除保留相对路径到 `presets/.trash/<folder>/...`；恢复递归找最新 `.bak`
      并还原目录；旧平铺 `.trash` 仍兼容。
- [x] `PresetBank` 的 paths/folders 与 presets 同步截断，无越界。
- [x] `delete` / `rename` / `restore` 后选中项由 `clamp_selection` 或显式清空
      处理，不会指向不存在的 index。
- [x] AUDIO 试听重采样到 48 kHz；拖入检查 SOUND tab、patch、32 层上限，
      生成显式 Clip 与单条 undo。
- [x] `--new` 递归计数生成 22 个预设、7 个分类目录；schema 仍 v4。
- [x] `--dump/--bake/--export` 在临时工程成功；`.tres`/manifest 路径未改。
- [x] 无新增依赖；无 `_keep` / 注释掉的大段代码 / 无理由 allow。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 170 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3949 commands / 191 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `f350ba0a…`
- [x] `--new` / `--dump` / `--bake` / `--export` 临时工程
- [x] `stale_library_scroll_cannot_hide_every_row`
- [x] `kind_filter_keeps_selection_and_export_target_in_sync`
- [x] `preset_folders_group_in_the_library_and_can_collapse`
- [x] `folder_headers_are_hit_testable`
- [x] `clicking_a_music_row_switches_to_the_music_tab`
- [x] `delete_and_restore_preserve_nested_folders`
- [ ] 真实窗口中的文件夹点击、拖拽与 music 切换手测（本环境无窗口）

## 残余风险与后续

- 多 music / 独立 Clip 库仍需要 schema v5。
- AUDIO/PRESET 同步解码与试听仍在 UI 线程；长文件需要后台 worker。
- 文件夹 UI 的重命名/移动/持久化折叠状态未做。
- 建议下一轮补 `AudioBank` 缓存与文件监听，避免大 `audio/` 目录每帧重建。

## 签署

- reviewer_1: approved（修复后）
- reviewer_2: approved
