# SFX Studio — Rust 端整改计划（v1）

状态：**P0 + P1 全部完成；P2 已落地参数元数据单表、对数映射、CLI/project_io 拆分、系统菜单、首帧延迟渲染、0005 UI 打磨、0006 时间线/音频交互修复、0007 图层检查器注册表与 0008 时间线缩放/列表交互收尾与 0009 app/ui 模块拆分、0010 后台预览渲染、0011 State 字段分组、0012 稳定 ID/schema v3 与 0013 ID 化选区；音频 RT 化见 Backlog**。
执行原则：先修正确性/数据安全，再修性能与交互，最后做结构性重构；每一项都要有可重复的验收命令。

## 0. 本轮目标与非目标

**目标**

- 消除会丢数据、导出互相覆盖、播放/定位错误、撤销语义错误等 P0 缺陷。
- 让缓存失效与文档修订解耦，拖动参数时不再每帧重渲染音频。
- 建立工程格式版本、加载校验、原子保存、唯一导出名、`.tres` 元数据往返。
- 补齐 `tone_env / exp_decay`（Godot 已有、Rust 缺失）的模型、DSP、导入导出与 UI。
- 把「二次审查 + 质量门禁」固化为强制流程（脚本 + hook + 审查记录）。

**非目标（本轮不做，已排期）**

- 后台渲染线程、无锁音频环形缓冲、重采样。
- 模块拆分（`app.rs` / `ui.rs` 大文件）与参数注册表全面重构。
- 稳定 UUID、跨语言 PCG32 一致、采样波形（Godot `Wave.SAMPLE`）支持。
- i18n 资源化（仅修文案错误，不引入框架）。

这些项依赖更大范围改动，按 Backlog 分阶段推进；本轮先把「地基」打稳。

## 1. 优先级矩阵

### P0 — 正确性与数据安全（全部完成 ✅）

| # | 项目 | 结果 | 验收 |
|---|---|---|---|
| P0-1 | 循环播放首样本重复 | `Playback::render_frame` 回绕不自增问题修复 | `audio::tests::looping_wraps_without_duplicating_the_first_sample` |
| P0-2 | seek 后播放头回 0 / 播放中 seek 无效 | 引入 `start_frame` + `seek()`（seek 时停流，避免 UI/音频线程分歧） | 手测 + `--selfcheck` |
| P0-3 | 保存失败写空文件 / 非原子 | 移除 `unwrap_or_default`；temp + rename；`.bak` 备份 | `model::tests::save_is_atomic_and_roundtrips` |
| P0-4 | 打开目录失败静默回退 demo | `load_project`/`import_project` 返回 `Result`；失败保留当前工程并报错 | 代码路径 + `save_is_atomic...` 的坏 JSON 断言 |
| P0-5 | 导出文件名冲突 | `unique_names()`；中文/重名不再互相覆盖 | `export::tests::unique_names_never_overwrite` |
| P0-6 | `.tres` 丢 `normalize_layers=false` | 按 bool 读取 | `export::tests::patch_metadata_and_tone_env_roundtrip` |
| P0-7 | Vary 产生两条 undo | 移除 `vary()` 内部重复快照 | `undo_transactions_ignore_noop_gestures` + 手测 |
| P0-8 | 缓存失效绑定 view revision | 拆 `version` / `music_version` / `view_version`；按 tab 渲染 | `app::tests::view_changes_do_not_rerender_audio` |
| P0-9 | `.tres` 丢 `tone_env/exp_decay` | 模型、DSP、导入、导出、UI 全链路补齐 | `exp_tone_envelope_matches_godot_formula`、`tone_env_survives_json`、`patch_metadata...` |
| P0-10 | 中文名被 `sanitize` 吃掉 | 显示名保留 Unicode；仅导出文件名 sanitize + 去重 | `display_names_keep_unicode`、`validate_repairs...` |

### P1 — 性能、健壮性与一致性（全部完成 ✅）

| # | 项目 | 结果 | 验收 |
|---|---|---|---|
| P1-1 | 拖动/滚动触发音频重渲染 | 内容/视图修订分离 + 50 ms 预览节流；`play()` 仍立即渲染 | `view_changes_do_not_rerender_audio` |
| P1-2 | 每帧全量 peak + 波形扫描 | 缓存 `cache_peak` / `music_cache_peak` + 2048-bucket 包络 | `envelope_buckets_track_min_and_max`、selfcheck |
| P1-3 | 未保存时 Open 无确认 | `ConfirmOpen` 模态：Enter 丢弃并打开 / Esc 取消 | `--selfcheck` 覆盖模态绘制路径 |
| P1-4 | 布局容量常量三处重复 | `ui::library_capacity/layer_capacity/timeline_height` 单一来源 | `compact_layout_stays_inside_a_1024_window` |
| P1-5 | 窄窗口顶栏重叠 | <1260 px 切 icon-only 顶栏；标题/参数行按可用宽度省略，不再压到按钮 | 1024 宽度布局测试 |
| P1-6 | 加载未校验（NaN/巨值/OOM） | `Project::migrate/validate`；时长/采样率/数组/自动化点全部 clamp；DSP 侧 30s/300s 防御上限 | `validate_repairs_untrusted_values_and_duplicate_names` |
| P1-7 | 点击/拖动无变化也产生 undo | `begin_edit` / `commit_edit` 事务化，仅真实变更入栈 | `undo_transactions_ignore_noop_gestures` |
| P1-8 | 撤销无标签 | undo/redo 记录 `(label, Project)`，状态栏提示 | 手测 |
| P1-9 | 打开工程不清 undo/redo | `open_folder` 清空并重置全部选区/过滤器 | 代码审查 |
| P1-10 | `Music` 空和弦行为与 Godot 不一致 | 空 notes/chords 返回空流 | `music_without_notes_is_empty_like_the_godot_plugin` |
| P1-11 | 鼓层 Seed 对 Kick 无效果却展示 | Kick 隐藏 SEED | 手测 |
| P1-12 | clippy 未清零 | `cargo clippy --all-targets -- -D warnings` 通过 | preflight |
| P1-13 | `.tres` 元数据往返丢失 | `resource_name` + `group`（映射 Category）写入/读取 | `patch_metadata_and_tone_env_roundtrip` |
| P1-14 | manifest 信息不足 | v2：patch 原名、category、favorite、dsp_version、schema_version | `bake_and_export_write_unique_assets...` |

### P2 — 结构性改进（本轮部分完成，其余进入 Backlog）

| # | 项目 | 状态 |
|---|---|---|
| P2-1 | 共享布局 metrics | ✅ 已完成 |
| P2-2 | 预览渲染节流 | ✅ 已完成（50 ms） |
| P2-3 | 波形包络 | ✅ 已完成 |
| P2-4 | 参数元数据单表（`FieldSpec`：label/unit/range/scale/format）+ section 表 | ✅ 已完成（0007/0008 起 `layer_sections()` / `music_sections()` 驱动 inspector） |
| P2-5 | 模块拆分（app/ui → state/commands/input/ui/*） | 🟡 部分完成：`cli.rs`、`project_io.rs` 已拆；`app.rs`/`ui.rs` 主体待拆 |
| P2-13 | UI 精简：顶部按钮收进 `MENU`、移除 Trigger Pad | ✅ 已完成 |
| P2-14 | 启动/切换 tab 首帧延迟渲染（先画壳，下一帧渲染音频） | ✅ 已完成（后台渲染仍为 Backlog） |
| P2-6 | 后台渲染线程 + generation 取消 | ⏳ Phase 2 |
| P2-7 | 稳定 ID（Patch/Layer/Clip） | ⏳ Phase 2 |
| P2-8 | 参数对数映射（频率/截止/Arp/Bake Rate） | ✅ 已完成 |
| P2-9 | i18n 资源化 | ⏳ Phase 3 |
| P2-10 | 采样波形 / `Wave.SAMPLE` | ⏳ Phase 3 |
| P2-11 | 跨语言 RNG / 精度一致性 | ⏳ Phase 3（见 `docs/PARITY.md`） |
| P2-12 | 噪声低通系数按采样率归一化 | ⏳ Phase 3（需同步 Godot，否则破坏一致性） |

## 2. 本轮验收证据

审查记录：`docs/reviews/0001-rust-hardening.md`、`0002-field-spec.md`、`0003-module-split.md`、`0004-ui-menu-deferred-render.md`、`0005-ui-icons-margins.md`、`0006-timeline-audio-interaction.md`、`0007-layer-inspector-registry.md`、`0008-timeline-zoom-and-library.md`、`0009-module-split.md`、`0010-background-preview-render.md`、`0011-state-field-groups.md`、`0012-stable-ids.md`、`0013-id-based-selection.md`（status: approved，source_hash 匹配）。

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                 # 68 passed
cargo run -- --selfcheck   # 2 frames / 2552 commands / 118 text runs
./scripts/preflight.sh
./scripts/review_gate.sh   # 需要 docs/reviews/*.md 中 approved + 匹配 source_hash
```

导出链路端到端测试（临时目录）：

- `bake` 写出 `audio/baked/*.wav` + `music.wav` + `.wav.import` + `sfx_manifest.json`。
- `export_tres` 写出 `audio/sfx/*.tres` + `audio/music.tres`。
- 重新 `import_project` 后，`resource_name`、`group → Category`、`tone_env`、`normalize=false` 保持。
- 重名/中文名通过 `unique_names` 生成 `coin`、`coin_2`、`sound`、`sound_2`。

## 3. Backlog（下一迭代，按依赖排序）

1. **音频 RT 化**：无锁 ring buffer、设备热插拔、48k 固定渲染 + `rubato` 重采样、欠载统计。
2. **跨语言一致性**：PCG32 测试向量、相位回绕、f32/f64 策略、容差 golden（见 `docs/PARITY.md`）。
3. **采样波形**：`Wave::Sample` + WAV 导入 + `sample_pitch/loop` 导入导出。
4. **i18n 与可访问性**：文案表、焦点遍历、键盘可达全部操作。
5. **CLI 产品化**：`clap` 子命令、`--golden` 渲染校验、批量导出接入游戏仓库 CI。
6. **可选项**：自动化点稳定 ID；`Drag::Clip` 的瞬时索引改为 ID；导出 manifest 暴露 patch id。

## 4. 已验证的端到端场景

- 临时工程包含两个同名 patch：加载时重命名为 `beep` / `beep_2`，导出得到同名唯一文件与 manifest。
- `--dump` 显示 schema v2、category 与去重后的名称。
- `--bake` 生成 `beep.wav`、`beep_2.wav`、`music.wav` 及三个 `.wav.import`。
- `--export` 生成 `beep.tres`、`beep_2.tres`、`music.tres`；重新导入后 `resource_name`/`group`/`tone_env` 保持。

## 5. 本轮增量（0002）

- `FieldSpec` 统一 `LayerField/MusicField/ClipField` 的 label/unit/range/scale/format，删除三套重复的 label/range/format。
- 频率、截止、Arp 频率、Music Bake Rate 改为对数滑块；点击滑块中点得到几何中点（如 cutoff 20–18000 的中点 ≈ 600 Hz）。
- 新增 `log_fields_map_to_a_geometric_midpoint`、`clicking_the_cutoff_track_midpoint_uses_a_log_curve`。

## 6. 本轮增量（0003）

- 新增 `src/cli.rs`：`run`、`build_app`、`selfcheck`、`--help/--bake/--export/--dump/--project`。
- 新增 `src/project_io.rs`：`load_folder`、`open_folder`、`print_warnings`、`display_name`。
- `app.rs` 从 2,4k 行降到约 2.2k 行，只保留交互状态机、输入、命令与音频桥接。
- `main.rs` 改为 `cli::run()`；CLI 端到端（dump/bake）已复测。

## 7. 本轮增量（0004）

- 顶部 12 个操作按钮收进左上角 `MENU` 下拉（FILE / SOUND / EXPORT 三组）；保留 SOUND/MUSIC tabs 与 header 的播放/撤销/图层按钮。
- 移除底部 Trigger Pad（键盘 `1`–`8` 仍可触发 patch）；释放 74px 垂直空间给时间线/波形。
- 首次启动/切换 tab/选择 patch 时先画界面，下一帧再渲染音频，避免首击卡顿。
- 移除 `.github/workflows`（不使用 GitHub Actions）；`scripts/ci.sh` 仍可作为本地/共享 runner 入口。
- `cargo test` 47 passed；新增菜单绘制/命中测试与延迟渲染测试。

## 8. 本轮增量（0005）

- 新增 18 个 Lucide 图标（lucide-static v1.47.0，ISC），用于顶栏 UNDO/REDO/PLAY/STOP/LOOP/图层操作、SOUND/MUSIC 标签页、MENU、搜索框、系统菜单项与检查器按钮；图标与文字作为整体居中。
- 左侧列表修复收藏星标与波形标签重叠：星标固定槽位、标签统一右对齐、名称省略宽度按实测文本计算。
- 面板边距统一：检查器内容内缩 6px（分隔线/滑杆/按钮不贴边）、中央标题卡按钮垂直居中并留右内边距、自动化 tabs 留边距。
- 标题栏对齐：左侧 LIBRARY 与右侧 PARAMETERS 使用同一套 26px raised 标题条。
- 播放头时间标签加暗色底片，消除与标尺 0.00 的叠字；面板表面统一为中性石墨灰（不叠加主题色），时间线表面提亮，主题色仅用于指示/交互反馈。
- `scripts/source_hash.sh` 纳入 `assets/`，图标资源不再绕过审查门禁。
- 新增 `inspector_controls_keep_panel_margins` 布局测试；`cargo test` 48 passed。

## 9. 本轮增量（0006）

- **Clip 拖拽不再改变比例**：手势开始时把 `State::timeline_span()` 固定到 `timeline_pin`，`Layout` 与拖拽换算都用冻结比例；松开后重新 fit。时间线内容裁剪到卡片内，溢出不再画到相邻卡片。
- **共享吸附网格**：新增 `CLIP_SNAP = 5ms` 与 `ui::snap_time()`，Clip Move/Resize/Trim 与自动化点共用；状态栏拖拽提示显示 `SNAP 5ms`。
- **收藏星标可点击**：新增 `Hit::PatchFavorite(usize)` 与 `patch_star_rect()`；点击星标切换收藏、不动当前选中、hover 变主题色、可撤销；仍支持收藏过滤。
- **播放头无锁**：音频回调每 buffer 把帧位置写入 `AtomicU64`，`AudioEngine::position()` 直接读原子；UI 不再每帧抢 `Mutex<Playback>`。
- **自适应高度**：时间线按图层数收缩（3 行下限，音乐页按 pattern 行数），剩余高度给波形；时间线加入与标尺对齐的淡竖网格。
- `Icon` 表新增一致性测试，`document()` 改为安全 `get()`，避免新增图标时静默越界。
- 新增 4 条测试：星标命中、pin 比例稳定、自适应高度、星标点击撤销；`cargo test` 53 passed。

## 10. 本轮增量（0007）

- 新增声明式图层检查器表：`LayerRowSpec` / `LayerRow` / `LayerSectionSpec` 与 `model::layer_sections(wave)`；按波形返回 PITCH / AMP ENV / FILTER / NOISE / ARP / FM / PLUCK / DRUM 分组。
- 行级可见性由谓词驱动：Sine 显示 HARSH、Pulse 显示 DUTY、Exp 包络显示 EXP DECAY、Kick 隐藏 SEED，删除 `inspector_items` 中按波形手写的六段分支。
- `ui::inspector_items` 只负责把 section 表渲染成 `RawItem`（含 TONE ENV / ENV / FILTER 选项行），约减 120 行重复代码，UI 输出保持不变。
- 新增 `layer_sections_expose_wave_specific_fields`（模型）与 `inspector_renders_the_layer_registry`（UI 顺序）测试；`cargo test` 55 passed。

## 11. 本轮增量（0008）

- **检查器注册表收尾**：`CLIP_FIELDS` 与 `music_sections()`（MUSIC / INSTRUMENTS / MIX / HARMONY / PATTERN）驱动 Clip/Music inspector；行类型含字段、波形选项、和弦/移调、Pattern 开关、说明文案。`inspector_items` 不再手写这两块。
- **时间线缩放/平移**：`State::timeline_zoom`（1–32×）与 `timeline_offset`；`Layout::time_x/x_time` 统一带偏移，标尺按可见窗口取 tick；⌘/Ctrl+滚轮围绕指针缩放，Shift+滚轮平移，状态栏显示倍率；播放头移出视口时不再画在错误位置。
- **库列表滚动条**：行右侧留 8px gutter，溢出时出现 4px 轨道 + 可拖动 thumb（`Hit::LibraryScrollbar` / `Drag::LibraryScroll`），滚轮与拖动共用 `patch_scroll`。
- **分类下拉**：`ALL` 按钮改为弹出列表（`CategoryPick(Option<Category>)`），当前项打勾；点击外部/Esc 关闭，`C` 快捷键仍可循环。
- **吸附与边界**：Shift 关闭 Clip/自动化 5ms 吸附（状态栏显示 FREE）；Clip Move 起始时间 clamp 到 `ClipField::Start` 上界。
- 标尺末端放不下的 tick 文案不再被卡片裁切。
- 新增 7 条测试（缩放映射、库滚动条、分类菜单、Music 注册表、Shift 关闭吸附、分类过滤）：`cargo test` 62 passed。

## 12. 本轮增量（0009）

- **ui 拆分**：`ui.rs` 3300 → 620 行（共享 Action/Hit/RawItem/Item/Button + 常量 + 测试），新增 `ui/layout.rs`（634）、`ui/paint.rs`（1893）、`ui/hit.rs`（176）；`pub use` 保持 `ui::Layout`、`ui::hit`、`ui::paint` 等外部接口不变。
- **app 拆分**：`app.rs` 2488 → 891 行（State/Tab/Drag/Modal + 工具函数 + 测试），新增 `app/input.rs`（963，AppLogic + 指针拖拽）、`app/commands.rs`（587，文件/导出/编辑命令）、`app/audio_bridge.rs`（67，play/seek/stop）。
- 跨子模块调用的方法改为 `pub(super)`；子模块统一 `use super::*;` 复用父模块导入，无重复依赖声明。
- 纯结构调整：62 tests、selfcheck `2552 commands / 118 text runs` 与拆分前完全一致，默认界面截图无变化。

## 13. 本轮增量（0010）

- **后台预览渲染**：新增 `app/render.rs` + 独立 `sfx-render` 线程；UI 只发送 `RenderJob`（Patch/Music clone + `RenderKey{tab, rate, version, music_version, patch}`），worker 通过共享 `AtomicU64` 跳过过期任务，并把 `samples/envelope/peak` 回传。
- **结果校验**：只有 `pending key == 当前 key` 时才写入缓存；切 patch / 编辑 / undo / open 都会让旧结果自动失效。
- **节流与取消**：请求间隔 80ms；同一 key 不会重复排队；`play()` 先 `cancel_preview()` 再同步 `ensure_cache`，避免旧预览覆盖刚渲染的播放数据。
- **兜底**：worker 不可用或任务卡住 >5s 时自动取消并退回同步 `ensure_cache_soon`；headless selfcheck 关闭后台线程（`SfxLogic::new(state, background)`），保持确定性输出。
- `synth::ENVELOPE_BINS` 抽为共享常量，同步/异步渲染使用同一包络规格。
- 新增 2 条 worker 测试（同 key 去重、实际渲染回传）；`cargo test` 64 passed；实机截图确认异步结果到达后波形正常绘制。

## 14. 本轮增量（0011）

- `State` 从 50+ 个平铺字段拆成四个组：
  - `DocState`：`project`、`root`、`dirty`、`version`、`music_version`、`undo`、`redo`、`pending_edit`。
  - `ViewState`：tab/选区/滚动/过滤/搜索/菜单/拖拽/模态/hover/光标/timeline 视口。
  - `PlaybackState`：音频缓存、播放头、loop/seek 状态、音频设备信息、预览节流。
  - `UiState`：`status`、`toast`。
- 所有访问路径机械迁移（`state.project` → `state.doc.project` 等）；`State` 上的方法（`mark_dirty` / `ensure_cache` / `select_patch` / `toast` 等）保持原签名，调用方无需改动。
- 纯结构调整：64 tests、selfcheck `2552 commands / 118 text runs` 与拆分前一致，无行为变化。
- 稳定 ID / schema v3 未包含在本轮，仍为 Backlog 第 1 项。

## 15. 本轮增量（0012）

- `Patch` / `Layer` / `Clip` 新增 `#[serde(default)] id: u32`；`Project` 新增 `next_id` 发号器。
- `Project::ensure_ids()`：补全缺失 ID、修复重复 ID、顺延 `next_id`；`migrate()` / `validate()` / `demo()` / `State::mark_dirty()` 调用，新增对象在编辑后立即获得稳定 ID。
- `PROJECT_VERSION` 2 → 3；`docs/FORMAT.md` 增加 `next_id` / `id` 字段与 v3 迁移说明，manifest `schema_version` 随之为 3。
- 渲染 worker 的 `RenderKey.patch` 改为稳定 patch id，切换/重排 patch 后旧结果不会误用。
- 新增 3 条测试：迁移后 ID 唯一且 `next_id` 顺延、demo 自带 ID、JSON 往返保留 IDs；`cargo test` 66 passed。

## 16. 本轮增量（0013）

- `ViewState.selected_patch/selected_layer/selected_clip` 从索引改为 `Option<u32>` 稳定 ID。
- 新增 `patch_index/patch/patch_mut`、`layer_index/layer/layer_mut`、`clip_index/clip/clip_mut` 与 `select_clip(index)`；`select_patch/select_layer` 仍按行索引调用，内部写入 ID。
- 所有直接比较/查找改为 ID：库行高亮、timeline 行/Clip 高亮、inspector 的 CLIP 段、automation 命中/绘制、hover 信息、渲染 key。
- 删除/复制/新增 patch/layer/clip 后先 `mark_dirty()`（分配 ID）再按索引选中；timeline 点击 materialize clip 时用 `ensure_ids()` 避免无谓的版本变更。
- `select_patch` 在新 patch 中找不到原 layer ID 时，按之前的 layer 槽位 clamp 选择，保持旧交互习惯。
- 新增 2 条测试：重排后选区仍指向同一 ID、删除选中 patch 后按邻居 ID 选中；`cargo test` 68 passed。

## 17. Definition of Done（每个 PR 必须满足）

- [ ] `docs/PLAN.md` 对应条目状态更新。
- [ ] `scripts/preflight.sh` 全绿（fmt / clippy -D warnings / test / selfcheck）。
- [ ] 新增或修改行为有对应测试；无法自动化的写入手测清单。
- [ ] 独立第二审查完成，`docs/reviews/NNNN-*.md` 写入 `status: approved` 且 `source_hash` 匹配。
- [ ] `scripts/review_gate.sh` 通过（pre-push 强制；共享 runner 可执行 `scripts/ci.sh`）。
- [ ] 数据格式/导出行为变化时，同步更新 `docs/FORMAT.md`、`docs/PARITY.md`。
