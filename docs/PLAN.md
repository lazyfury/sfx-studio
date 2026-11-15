# SFX Studio — Rust 端整改计划（v1）

状态：**P0 + P1 全部完成；P2 已落地参数元数据单表、对数映射、CLI/project_io 拆分、系统菜单、首帧延迟渲染、0005 UI 打磨、0006 时间线/音频交互修复、0007 图层检查器注册表与 0008 时间线缩放/列表交互收尾与 0009 app/ui 模块拆分、0010 后台预览渲染、0011 State 字段分组、0012 稳定 ID/schema v3、0013 ID 化选区、0014 音频回调去锁、0015 48k 预览/重采样/设备重连、0016 Rust 端采样波形与 0017 Rust↔Godot golden 测试；0018 DSP v3 立体声/空间/母带、0019 HD 声音模型见第 6/7 节、0020 工程目录 / 工程内预设 / 拖入见第 22 节、0021 jsfxr 预设目录 / demo 音轨见第 23 节、0022 预设直接导出 WAV 见第 24 节、0023 预设可用性 / 读-编辑边界 / 采样内存见第 25 节、0024 预设文件生命周期 / 采样预设 / 和弦高亮见第 26 节、0025 预设采样懒加载 / .trash 恢复见第 27 节、0026 右侧检查器滚动修复 / 0027 滚轮方向对齐见第 28/29 节、0028 参数中文副标题 / 波形指南见第 30 节；RNG 一致性与采样 `.tres` 往返见 Backlog**。
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

审查记录：`docs/reviews/0001-rust-hardening.md`、`0002-field-spec.md`、`0003-module-split.md`、`0004-ui-menu-deferred-render.md`、`0005-ui-icons-margins.md`、`0006-timeline-audio-interaction.md`、`0007-layer-inspector-registry.md`、`0008-timeline-zoom-and-library.md`、`0009-module-split.md`、`0010-background-preview-render.md`、`0011-state-field-groups.md`、`0012-stable-ids.md`、`0013-id-based-selection.md`、`0014-rt-audio-callback.md`、`0015-48k-resample-reconnect.md`、`0016-sample-waveform.md`、`0017-parity-golden.md`、`0018-dsp-v3-stereo-space.md`、`0019-hd-voices-and-drums.md`、`0020-library-modes-drag-in.md`（status: approved，source_hash 匹配）。

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                 # 83 passed
cargo run -- --selfcheck   # 2 frames / 2555 commands / 119 text runs
./scripts/preflight.sh
./scripts/review_gate.sh   # 需要 docs/reviews/*.md 中 approved + 匹配 source_hash
```

导出链路端到端测试（临时目录）：

- `bake` 写出 `audio/baked/*.wav` + `music.wav` + `.wav.import` + `sfx_manifest.json`。
- `export_tres` 写出 `audio/sfx/*.tres` + `audio/music.tres`。
- 重新 `import_project` 后，`resource_name`、`group → Category`、`tone_env`、`normalize=false` 保持。
- 重名/中文名通过 `unique_names` 生成 `coin`、`coin_2`、`sound`、`sound_2`。

## 3. Backlog（下一迭代，按依赖排序）

1. **RNG / D3 一致性**：PCG32 双端实现 + 噪声类 golden；噪声低通按采样率归一；`dsp_version` dispatch 完整化。
2. **采样 `.tres` 往返**：解析/写入 `AudioStreamWAV` 子资源，或导入 `baked` WAV 作为 Sample 层。
3. **音频 RT 余项**：设备重连的硬件回归、退役槽满时的回调 drop 兜底、bake 端统一 48k/重采样。
4. **CI**：服务器上安装/缓存 Godot，把 `scripts/parity.sh` 接入 `scripts/ci.sh`。
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

## 17. 本轮增量（0014）

- **音频回调去锁**：去掉 `Mutex<Playback>`。回调持有自己的 `Playback`（samples/pos/loop/fade），只通过 `Arc<Mutex<Option<Command>>>` 的单槽传递 `Play/Stop`，并用 `try_lock` 读取，回调永不阻塞。
- `AudioEngine::play/stop` 只做一次短暂加锁写命令，不阻塞音频线程；`position` / `is_playing` 仍为原子读取。
- **设备错误处理**：cpal error callback 累加 `device_errors`、置 `playing=false` 并标记 `alive=false`；UI 下一帧读取计数并 toast「audio device error · playback stopped」，`play()` 在设备失联时直接拒绝启动。
- `Playback::apply` 可测试化；新增 3 条音频测试（按帧重启、Stop 渐隐、命令槽 try_lock 读取），旧循环/渐隐测试保留。
- 本轮不包含：48k 固定渲染 + 重采样、回调内零 deallocation、设备自动重连、欠载统计。

## 18. 本轮增量（0015）

- `synth::PREVIEW_RATE = 48_000`；`State::preview_rate()` 固定返回 48k，导出 WAV 复用同一常量。
- 音频回调按设备率做线性重采样（`ratio = 48000 / device_rate`），播放游标改为 `f64` 源帧；`position()` 按 48k 源帧换算。
- 回调替换样本时把旧 `Vec` 放进 `retired` 单槽，UI 每帧 `reclaim()` 在 UI 线程释放；槽满时仍由回调兜底 drop（记入 RT 余项）。
- 设备错误后 2s 退避重连（`try_reconnect`），重连成功/失败通过 toast 提示；`play()` 在设备失联时拒绝启动。
- 新增 underrun 计数（回调发现 audible 但样本为空）并提供 `take_underruns()`；UI 与 device errors 一起提示。
- `docs/PARITY.md` D6/D7 更新为「预览固定 48k + 设备重采样，bake 仍独立」。
- 新增 2 条音频测试（重采样插值、退役 buffer 交还）；`cargo test` 73 passed。

## 19. 本轮增量（0016）

- 新增 `src/wav.rs`：RIFF/WAVE 读取，支持 PCM16 / F32、单声道/立体声，单声道归一化；拒绝未知格式。
- `Wave::Sample`（Godot id 9）；`Layer` 增加 `sample_path` / `sample_pitch` / `sample_loop` 与 `#[serde(skip)] sample_data`。
- `Project::load_samples(root)` 在打开工程时按路径解码（相对路径基于工程根目录）；失败进入 warnings。
- `synth::render_sample` 对齐 Godot `_build_sample`：线性插值、循环 wrap、源率/目标率比 + `2^(pitch/12)`、ADSR、gain。
- 检查器新增 SAMPLE 段：`LOAD WAV…`（rfd 选择 + 复制到 `audio/sfx/samples/`）、`PITCH`、`LOOP`；`Action::LoadSample` / `ToggleSampleLoop`。
- `PROJECT_VERSION` 3 → 4；`docs/FORMAT.md` 增加字段与迁移说明；`.tres` 导出的采样 patch 自动整段 bake（Godot 侧直接播放 baked）。
- 新增 6 条测试：WAV 读写、采样渲染循环/变调、JSON 字段、`load_samples`、采样导出 bake、检查器控件；`cargo test` 79 passed。

## 20. 本轮增量（0017）

- 新增 `src/parity.rs`：9 个确定性 fixture（SINE/TRI/SAW/PULSE/SQR/FM/ARP/多 clip/多层 normalize），生成 `.tres` + Rust WAV，并比较 peak / RMS / max |Δ|。
- 新增 CLI：`--parity-fixtures DIR`、`--parity-check DIR`；Godot 侧 `scripts/parity/render.gd`，编排脚本 `scripts/parity.sh`（自动建临时工程、symlink addon、editor import、headless render）。
- 修正 clip 渲染的包络时长：v2 使用 `min(clip.duration, layer.duration)`，与 Godot flatten 一致；dsp v1 保留旧行为。`DSP_VERSION` 1 → 2。
- 实测（Godot 4.7.2）：9/9 fixture peak/RMS 0.000 dB，max |Δ| 最差 0.00674，`parity ok`。
- 新增 4 条测试：fixture 确定性、比较器容差、fixture→WAV→compare 闭环、dsp v1/v2 包络差异；`cargo test` 83 passed。

## 21. Definition of Done（每个 PR 必须满足）

- [ ] `docs/PLAN.md` 对应条目状态更新。
- [ ] `scripts/preflight.sh` 全绿（fmt / clippy -D warnings / test / selfcheck）。
- [ ] 新增或修改行为有对应测试；无法自动化的写入手测清单。
- [ ] 独立第二审查完成，`docs/reviews/NNNN-*.md` 写入 `status: approved` 且 `source_hash` 匹配。
- [ ] `scripts/review_gate.sh` 通过（pre-push 强制；共享 runner 可执行 `scripts/ci.sh`）。
- [ ] 数据格式/导出行为变化时，同步更新 `docs/FORMAT.md`、`docs/PARITY.md`。

## 6. 本轮增量（0018）— DSP v3：空间 / 立体声 / 母带

目标：让 demo 与导出音频脱离「干声合成器」听感，同时不破坏 Rust ↔ Godot 的
既有 golden 一致性。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0018-1 | 去咔哒 | 每层 / 总线首尾 2–4 ms 淡入淡出 | `edges_are_declicked` |
| 0018-2 | 立体声 + PAN | `Layer.pan`，等功率声像，缓存/播放/导出全链路双声道 | `pan_hard_left_is_silent_on_the_right` |
| 0018-3 | 混响 + 早期反射 | `Patch.space` / `Patch.room`；ER 6 抽头 + 4 comb + 2 allpass，随长度生成尾音 | `space_adds_energy_after_source` |
| 0018-4 | 母带 | DC block + 20 Hz 高通 + 软限幅 + 峰值归一化（v3 始终归一到 0.95） | `master_output_never_clips` |
| 0018-5 | 24-bit 导出 | 手动 Export WAV 写 24-bit stereo + TPDF dither；bake 保持 Godot 友好的 16-bit stereo | `wav24_header_is_stereo_pcm` |
| 0018-6 | DSP 版本 | `DSP_VERSION = 3`；老工程保持 v2 声音，检查器提供 `UPGRADE TO HD` | `legacy_patches_stay_mono_and_unprocessed` |
| 0018-7 | 一致性 | `.tres` 实时播放仍为 v2 子集；带 PAN/空间/自动化的 patch 在 `.tres` 导出时整段 bake | `needs_bake` 路径 + PARITY.md D8 |

非目标（下一批）：PolyBLEP/过采样、鼓重做、采样层 round-robin、humanize。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
./scripts/review_gate.sh
```

实测（2026-11-15）：

- `cargo test` 102 passed；`--selfcheck` 2 frames / 2569 commands / 125 text runs。
- `./scripts/parity.sh` 9/9 `parity ok`，`max |Δ|` 仍为 0.00674（v2 路径未变）。
- 临时工程 `--bake/--export/--dump`：48 kHz 16-bit stereo WAV、1.334 s（含混响尾音）、
  `peak = -0.45 dBFS`、首尾样本为 0、左右 RMS 0.081/0.102。
- Godot 4.7.2 实测加载导出的 `.tres`：`baked=true len=1.334 stereo=true`，与 Rust 渲染一致。
- v2 工程保持 0.35 s、`L == R`、无尾音（PAN/SPACE 被忽略）。

## 7. 本轮增量（0019）— HD 声音模型：去混叠 / 鼓组 / 采样播放

目标：解决「合成器味」的根因——朴素波形的混叠、玩具级鼓组、采样循环接缝，
同时保持 v2 golden 逐样本不变。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0019-1 | 去混叠 | Saw/Pulse/Square 用 PolyBLEP；Triangle 用积分方波；tone/FM/Arp/Pluck 以 2× 渲染后过 31 抽头半带 FIR 抽取 | `hq_oscillators_reject_alias_energy`（5 kHz 锯齿 13 kHz 杂散 0.036 → 0.000004）、`decimator_preserves_dc_and_low_frequencies` |
| 0019-2 | 鼓组重写 | Kick：click + 音高包络 + sub + soft clip；Snare：双腔体 + SVF 带通噪声 + 瞬态；Hat：6 个 PolyBLEP 方波金属音 + 6.5 kHz 高通；Clap：4 段带通突发 + 尾音 | `hq_drums_are_finite_and_remodelled`（含 kick/hat 过零率对比） |
| 0019-3 | 采样率一致性 | 噪声低通按 22.05 kHz 参考换算成 Hz；混响 damping 按 Hz；Pluck 环路滤波/增益按 Hz 与参考率换算；v3 固定 48 kHz bake | `hq_voices_are_rate_consistent`（22k/44.1k 的 RMS 与过零率漂移 < 25%） |
| 0019-4 | 采样播放 | `sample_start`、`sample_reverse`、10 ms 交叉淡化循环；SAMPLE 检查器新增 START/REVERSE | `sample_start_reverse_and_loop_crossfade`、`sample_inspector_exposes_start_and_reverse` |
| 0019-5 | 变体/轮替 | A/B/C 变体额外抖动 `space/room/pan`；游戏内随机选一个变体即 round-robin | `variations_are_inserted_and_kept_in_order` |
| 0019-6 | 示例工程 | 新增 `--new DIR`：写出 demo 工程 + baked 音频，便于试听/回归 | CLI 手测 + `--new` 端到端 |
| 0019-7 | 版本隔离 | v2 仍走 `synth::render_layer`；v3 走 `render_layer_hq`；`TrackOptions` 显式区分 | `./scripts/parity.sh` 9/9 不变 |

非目标：真正的多文件 round-robin / velocity 层（需要模型与 Godot 侧扩展）；卷积混响。

实测（2026-11-15）：

- `cargo test` 109 passed；`--selfcheck` 2 frames / 2566 commands / 124 text runs。
- `./scripts/parity.sh` 9/9 `parity ok`（v2 路径未变）。
- `--new /tmp/sfx-demo`：11 个 patch 全部 bake，48 kHz / 16-bit stereo，峰值统一
  −0.45 dBFS；`explosion` 1.697 s、`wind` 2.649 s、`coin` 0.294 s（尾音随 space 缩放）。
- Godot 4.7.2 加载 `explosion.tres`：`baked=true len=1.697 stereo=true rate=48000`。
- 5 kHz 锯齿 13 kHz 折叠杂散：v2 0.0359 → v3 0.000004。


## 22. 本轮增量（0020）— 工程目录 / 工程内预设 / 拖入

目标：工程必须以目录为单位打开；PRESETS 数据来自当前工程目录自带的
`presets/*.json`，不是全局编译库；预设可直接拖进时间线或工程库。
不改变工程 schema、DSP 或 Godot 导出。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0020-1 | 工程目录 | 启动时 `rfd` 选择工程根目录（或 `--project DIR`）；取消则进入空状态，不再自动加载 demo | `empty_state_has_no_project_or_presets`、空状态绘制 `NO PROJECT` / `OPEN A PROJECT` |
| 0020-2 | 空状态 | `State::empty()` 无工程/无预设；中央显示 OPEN/NEW PROJECT 提示；`State::demo()` 仅供测试与 selfcheck | `empty_project_paints_open_hints` |
| 0020-3 | 工程内预设 | `src/presets.rs` 从 `<project>/presets/*.json` 加载；收藏存 `presets/.favorites.json`；不写入 `sfx.project.json` | `loads_presets_and_favorites_from_a_project_folder`、`opening_a_project_loads_its_internal_presets` |
| 0020-4 | 新建工程 | GUI `MENU → NEW PROJECT…` 写 starter 工程 + `presets/`；`--new DIR` 额外 bake；已存在工程文件/非空 `presets/` 时拒绝覆盖 | `write_presets_creates_unique_files_and_roundtrips`、CLI `--new` 端到端 |
| 0020-5 | 双库模式 | 左栏标题条 `PROJECT \| PRESETS`；只切换数据源，中央时间线与工程文档保持不变 | `preset_mode_swaps_rows_and_hit_targets` |
| 0020-6 | 试听 | 单击预设同步渲染并播放；独立 `preset_playing` 标记，工程播放头不被试听拖动 | `preset_filters_and_selection_stay_out_of_the_project` |
| 0020-7 | 拖入手势 | 4 px 阈值；ghost、目标行/时间线高亮；Esc / 指针离开取消 | `drop_target_maps_timeline_rows_and_the_left_panel` |
| 0020-8 | 时间线落点 | 复制预设 layers 到当前 patch，重新发 ID；落点作为预设 0 点，无 clip 层生成显式 clip，有 clip 层保留相对编排；整次 drop 一条 undo | `dropping_a_preset_inserts_layers_and_is_one_undo_step`、`instantiate_layers_anchors_implicit_and_explicit_clips` |
| 0020-9 | 工程落点 | 拖到左栏：整份预设复制为工程 patch，名称去重（`_2`），`favorite=false`，切回 PROJECT 并选中 | `dropping_a_preset_on_the_left_copies_it_into_the_project`、`copied_patches_get_unique_names_and_fresh_ids` |
| 0020-10 | 只读边界 | PRESETS 模式禁用 New Sound/Rename/Delete/Duplicate/Randomize/Vary/Favorite/Export/Bake/TRES；菜单置灰，快捷键同样拦截；Save/Open/播放/图层编辑保留 | `preset_mode_disables_project_only_actions` |
| 0020-11 | 上限 | 拖入前检查 32 层 / 512 patch 上限，避免保存时静默截断 | 拖入代码路径 + `validate()` 既有上限 |

非目标：引用式 clip（一个 track 放任意音色）、外部 Finder WAV 拖放、
`SAVE AS PRESET`（把当前 patch 写回 `presets/`）。这些需要 schema/DSP/平台层
扩展，保留为后续独立项。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
./scripts/review_gate.sh
```

实测（2026-11-15）：

- `cargo test` 123 passed。
- `--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `--new /tmp/...`：写出 1 个 starter patch、11 个 `presets/*.json`、baked 音频；
  `--dump` 仍为 schema v4。
- 单测覆盖：空工程状态、工程目录加载预设/收藏、预设写入去重、PROJECT/PRESETS
  切换、预设试听隔离、时间线/左栏 drop、ID 重发、undo、拖拽 ghost 绘制。
  未在窗口环境手测启动目录选择与指针拖拽（本环境无 GUI），由
  `create_project`/`open_folder`/`drop_preset`/paint 单测与 headless selfcheck 覆盖。
- 工程 schema 仍为 v4；CLI/bake/`.tres` 导出路径未改。

## 23. 本轮增量（0021）— jsfxr 预设目录 / demo 音轨

目标：参考 jsfxr 的 library 用法（`sfxr.generate(name)` + `sfxr.toWave`），
为 SFX LAB 提供「按名字生成预设」和可直接试听的 demo 音轨；不改变工程
schema、DSP 或 Godot 导出路径。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0021-1 | jsfxr 预设名 | 新增 `src/builtin.rs`：`pickupCoin`/`laserShoot`/`explosion`/`powerUp`/`hitHurt`/`jump`/`blipSelect`/`synth`/`tone`/`click`/`random` 共 11 个 family | `every_jsfxr_family_is_available_by_any_spelling` |
| 0021-2 | 确定性 | `generate(name, seed)` 对同一 `(name, seed)` 完全可复现；`random` 选择一个真实 family 并保留 `random_sfx` 名 | `generation_is_deterministic_per_seed`、`random_is_a_real_family_with_a_stable_name` |
| 0021-3 | 普通 Patch | 每个预设都是 `Patch`（多 layer、v3 DSP、space/room、category）；可直接进 `presets/*.json` / 时间线 | `bank_has_unique_names_and_renders_audible_audio`（11/11 有声、有限、≤ 1.0） |
| 0021-4 | CLI 单音 | `--list-presets`、`--preset NAME [--seed N] [--out FILE]`，输出 48 kHz / 24-bit stereo WAV | `/tmp/coin.wav`：0.33 s、peak 0.950、RMS 0.153 |
| 0021-5 | 新工程预设 | `--new` / NEW PROJECT 写入 11 个手调 demo + 11 个 builtin preset；`--install-presets DIR` 只补缺失项，不覆盖已有文件 | `--new`：1 sound / 22 presets；`install_missing_never_overwrites_existing_names_or_files` |
| 0021-6 | demo 音轨 | `src/demo.rs`：`NEON DRIVE` 138 BPM、`BOSS THEME` 168 BPM、`STAR MENU` 92 BPM；music bed + 预设 cue，混音归一到 −0.9 dBFS | `the_three_tracks_have_unique_keys_and_a_beat`、`rendered_tracks_are_finite_audible_and_cover_the_cues` |
| 0021-7 | preset reel | `--demo-reel` 按 jsfxr 顺序串联 11 个预设，间隔 0.18 s | `reel_visits_every_preset_in_order`；`reel.wav` 7.95 s |
| 0021-8 | demo pack | `--demo DIR`：工程 + `presets/` + `audio/baked` + `audio/demo/{3 tracks,reel,index.json}`；拒绝覆盖已有工程 | `demo_pack_writes_a_loadable_project_presets_tracks_and_reel`、`--dump` 可读 |
| 0021-9 | GUI | MUSIC 检查器新增 `DEMO TRACKS`（NEON DRIVE / BOSS THEME / STAR MENU）；载入替换当前 music，可撤销 | `music_inspector_follows_the_registry`、`loading_a_demo_track_replaces_the_music_and_is_undoable` |

非目标：逐样本复刻 sfxr 的 8-bit 波形与参数曲线；把 `randomize` 改成
jsfxr 的参数空间；把 demo 音轨直接嵌入 Godot 场景（demo WAV 仅用于试听）。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
./scripts/review_gate.sh
```

实测（2026-11-15）：

- `cargo test` 133 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `--demo /tmp/sfx-demo-pack --seed 7`：11 presets + 3 tracks + reel + `index.json`；
  `--dump` 显示 schema v4、music 138 BPM / 4 bars。
- `--preset pickupCoin --seed 3`：0.33 s / 48 kHz / 24-bit / peak 0.950。
- `--demo-track boss_theme`：12.12 s（8 bars + explosion 尾音），peak 0.900。
- `--demo-reel`：7.95 s / 11 presets，peak 0.900。
- `--new /tmp/sfx-new-proj`：22 个 preset 文件（11 demo + 11 builtin）；
  `--install-presets` 在空工程写入 11 个，第二次报告 already has all。
- 工程 schema 仍为 v4；bake / `.tres` / parity 代码路径未改。

## 24. 本轮增量（0022）— 预设直接导出 WAV

目标：PRESETS 库里的预设无需先复制进工程或拖上时间线，选中后即可直接
渲染为音频文件；输出与工程导出保持一致（48 kHz / 24-bit stereo）。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0022-1 | 导出目标 | `State::export_patch()` 按当前库来源选择目标：PROJECT 用当前 patch，PRESETS 用选中预设 | `wav_export_target_follows_the_active_library` |
| 0022-2 | 命令入口 | PRESETS 模式下 `MENU → EXPORT WAV` 与 `⌘/Ctrl+E` 可用；未选预设时提示先选择 | `preset_mode_disables_project_only_actions` |
| 0022-3 | 渲染路径 | 复用 v3 `fx::render_patch` + 48 kHz / 24-bit stereo TPDF WAV；不改 DSP、schema、`.tres` | `preset_wav_export_is_48k_24bit_stereo` |
| 0022-4 | 只读边界 | 导出只读预设，不修改 `presets/` 或 `sfx.project.json`；Bake/TRES/编辑命令仍禁用 | `preset_mode_disables_project_only_actions` |

非目标：把预设自动写回工程；批量导出整个 PRESETS 库；导出 `.tres` 预设。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

## 25. 本轮增量（0023）— 预设易用性 / 模式边界 / 采样内存

目标：让旧工程能自助恢复预设库，PRESETS 与 PROJECT 的编辑边界可见，
并消除采样 PCM 在 undo/预览路径上的深拷贝；不改变工程 schema、DSP 或
Godot 导出格式。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0023-1 | 空预设自救 | PRESETS 空库且有工程目录时显示 `INSTALL BUILTIN PRESETS`；调用 `PresetBank::install_missing`，不覆盖已有文件 | `empty_presets_offer_an_install_action`、`installing_presets_fills_an_empty_bank_without_a_project_file` |
| 0023-2 | 模式边界 | PRESETS 模式右侧显示选中预设的只读摘要；禁用增删层/Clip、采样、升级 DSP 等工程编辑命令 | `preset_mode_inspector_describes_the_selected_preset`、`preset_mode_disables_project_only_actions` |
| 0023-3 | Space 语义 | PROJECT 播放当前 patch；PRESETS 试听选中预设，未选则提示 | `space_in_presets_targets_the_selected_preset` |
| 0023-4 | 保存为预设 | `MENU → SAVE AS PRESET` 把当前 patch 写入 `<project>/presets/`，同名不覆盖 | `saving_the_current_patch_adds_a_project_preset` |
| 0023-5 | 采样共享 | `Layer.sample_data` 改为 `Arc<SampleData>`；undo、预览、`with_pitch`/拖入复制共享 PCM | `sample_fields_roundtrip_and_load` 中的 `Arc::ptr_eq` |
| 0023-6 | 采样上限 | 源文件 64MB 上限；解码前按帧数限制为 60 秒 @48 kHz（2,880,000 帧） | `rejects_oversized_files_before_reading`、`rejects_samples_over_the_frame_limit` |
| 0023-7 | ID 修复 | `ensure_ids` 用 `HashSet` 替换 `Vec::contains`，避免大工程 O(n²) | 既有 ID/migrate/validate 单测 |

非目标：退出拦截、预设重命名/删除、异步 Bake、RT buffer 归还队列；这些
依赖宿主关闭回调或更大范围的所有权调整，保留到下一轮。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 142 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `./scripts/preflight.sh` 通过（fmt + clippy + tests + selfcheck）。
- 空 PRESETS 工程可从 GUI 一键写入 11 个内置预设；`SAVE AS PRESET` 写入当前 patch
  且不覆盖同名预设。
- `Layer.sample_data` 的 clone 通过 `Arc::ptr_eq` 验证共享；60 秒帧上限与 64MB
  源文件上限均有单测。
- 工程 schema 仍为 v4；DSP / bake / `.tres` 导出路径未改。

## 26. 本轮增量（0024）— 预设文件生命周期 / 采样预设

目标：把 PRESETS 从“只读展示”补成可管理的工程内资产：预设采样能按工程
目录解析，预设可重命名/删除，并且删除进入隐藏回收目录；同时修正和弦预设
的激活高亮。不改变工程 schema、DSP 或 Godot 导出格式。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0024-1 | 预设采样 | `Patch::load_samples` 抽成共享方法；`PresetBank::load_project` 解析 `sample_path`，与工程 patch 一致 | `loads_preset_sample_layers_relative_to_the_project` |
| 0024-2 | 预设重命名 | `PresetBank::rename` 同时改 JSON 文件名、patch name、`.favorites.json`；同名拒绝、文件不覆盖 | `rename_moves_the_file_and_favourite_key` |
| 0024-3 | 预设删除 | `PresetBank::delete` 把 JSON 移到 `presets/.trash/<file>.<stamp>.bak`，再更新收藏与内存库 | `delete_moves_the_preset_file_into_the_trash` |
| 0024-4 | 菜单/模态 | MENU 增加 PRESETS 段：SAVE AS PRESET / RENAME PRESET / DELETE PRESET；PRESETS 模式只对选中预设启用 | `preset_mode_disables_project_only_actions` |
| 0024-5 | 和弦高亮 | `chord_preset_index` 对比当前 bass/chords，不再硬编码第一个 chip 为 active | `chord_preset_index_follows_the_current_progression` |

非目标：批量导出、异步 Bake、退出拦截、RT buffer 归还队列；仍保留在
Backlog。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 147 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `PresetBank::rename`：JSON 文件名、patch name、`.favorites.json` 同步更新；
  同名拒绝、已有文件不覆盖。
- `PresetBank::delete`：JSON 移入 `presets/.trash/`，内存库与收藏同步清理。
- 采样预设按工程根目录解析 `sample_path`；PATCH 与 preset 共用
  `Patch::load_samples`。
- 和弦 chips 的 active 状态由 `chord_preset_index` 计算。
- 工程 schema 仍为 v4；DSP / bake / `.tres` 导出路径未改。

## 27. 本轮增量（0025）— 预设采样懒加载 / 删除恢复

目标：避免打开工程时把所有预设 PCM 一次性解码，并给 `presets/.trash/`
一个可用的恢复入口；不改变预设 JSON 格式、工程 schema、DSP 或 Godot 导出。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0025-1 | 预设采样懒加载 | `PresetBank::load_project` 只解析/校验 JSON；新增 `hydrate(index)` 在试听、导出、拖入时按需解码 | `loads_preset_sample_layers_relative_to_the_project`（load 后为 None，hydrate 后 Some） |
| 0025-2 | 拖入/导出/试听 | 三条路径先 `hydrate`，失败 warning 合并进 toast；Arc 仍共享 PCM | `audition_preset` / `drop_preset` / `export_wav` 代码路径 |
| 0025-3 | 删除恢复 | `PresetBank::restore_latest` 找到 `.trash/` 中最新 `.bak`，移回 `presets/`，冲突加 `_2` | `restore_latest_brings_back_a_deleted_preset` |
| 0025-4 | 菜单入口 | PRESETS 段增加 `RESTORE LAST PRESET`；只在 PRESETS 模式可用 | `preset_mode_disables_project_only_actions` |

非目标：退出拦截、异步 Bake、RT buffer 归还队列、`.trash` 文件管理器 UI。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 148 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `PresetBank::load_project` 后采样层 `sample_data == None`；`hydrate(0)` 后
  `Arc<SampleData>` 可用，试听/导出/拖入前均调用 hydrate。
- `restore_latest` 把 `presets/.trash/one.json.<stamp>.bak` 移回
  `presets/one.json`；冲突时使用 `_2` 后缀。
- `./scripts/preflight.sh` 通过；工程 schema 仍为 v4，DSP/导出路径未改。

## 28. 本轮修复（0026）— 右侧检查器滚动

目标：修复右侧 PARAMETERS 面板滚轮方向反了、以及滚动最大值少 4px 导致
最后一行无法完全露出。不改变布局尺寸或参数面板内容。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0026-1 | 滚轮方向 | 抽出 `inspector_wheel_scroll`；向下滚（`delta.y < 0`）增大 scroll，与左侧库列表一致 | `inspector_wheel_scroll_moves_down_on_a_downward_wheel` |
| 0026-2 | 底部可达 | `INSPECTOR_TOP_PAD` 计入 `inspector_max_scroll`；滚动到底时最后一行 bottom == 面板 bottom | `inspector_scroll_reaches_the_last_row` |
| 0026-3 | 拖动滚动条 | 拖动和滚轮共用 `layout.inspector_max_scroll()`，不再各自推导 | 代码路径 + 新单测 |

非目标：惯性滚动、滚动条点击轨道跳转、分页/折叠参数组。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 150 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- 1024×400 窗口下 `inspector_max_scroll > 0`；滚动到底时最后一行 bottom 与
  inspector bottom 相差 < 0.01px。
- 同尺寸下 `delta.y = -1` 使 scroll 增大，`delta.y = +1` 使 scroll 减小。
- `./scripts/preflight.sh` 通过；工程 schema / DSP / 导出路径未改。

## 29. 本轮修复（0027）— 滚轮方向对齐 igui_core

目标：修正 0026 对 `delta.y` 符号的错误假设。`igui_core::InputEvent::Wheel`
规定 **`y > 0` 表示向下滚动**（winit 适配层已对 `LineDelta` 取反），
本项目应直接以该约定为准，而不是再取反一次。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0027-1 | 统一方向 | 新增 `wheel_steps(delta_y)`：正数返回正步进，负数返回负步进 | `inspector_wheel_scroll_moves_down_on_a_downward_wheel` 中的 `wheel_steps` 断言 |
| 0027-2 | 右侧检查器 | `inspector_wheel_scroll` 使用 `wheel_steps`；`y > 0` 增大 scroll | 同一测试 |
| 0027-3 | 左侧库列表 | 滚轮 `next = scroll + wheel_steps(delta.y)`，向下滚增加 index | 代码路径 + 方向 helper |
| 0027-4 | 时间线图层/Shift 平移 | 图层滚动使用同一方向；Shift+wheel 向下平移视图向右 | 代码路径 |

非目标：Cmd+wheel 缩放方向保持不变；惯性滚动 / 轨道点击跳转仍不在本轮。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 150 passed；`--selfcheck` 2 frames / 2572 commands / 126 text runs。
- `wheel_steps(1.0) == 1`、`wheel_steps(-1.0) == -1`；`inspector_wheel_scroll`
  在 `delta.y > 0` 时增大，在 `delta.y < 0` 时减小。
- 左侧库、右侧 inspector、时间线图层滚动统一使用同一方向约定。
- `./scripts/preflight.sh` 通过；工程 schema / DSP / 导出路径未改。

## 30. 本轮增量（0028）— 参数中文副标题 / 波形指南

目标：让右侧参数面板自解释。每个滑块显示中文副标题；WAVE 行显示当前波形
解释，并在面板底部提供 14 种波形的中文指南。不改变参数范围、默认值或 DSP。

| # | 项目 | 方案 | 验收 |
|---|---|---|---|
| 0028-1 | 参数副标题 | `FieldSpec` 的调用方（Layer/Patch/Clip/MusicField）新增 `desc()`；`RawItem::Param`/`Item::Param` 携带副标题，行高 26→38px | `inspector_parameters_have_chinese_subtitles` |
| 0028-2 | 滑块位置 | `param_track` 下移到 `top+30`，与双行文字不重叠；点击/拖动/复位仍用同一命中区 | 现有参数测试 + layout 命中 |
| 0028-3 | 当前波形解释 | WAVE 选项下方插入 `SINE：...` 一行，内容来自 `Wave::desc()` | 代码路径 |
| 0028-4 | 波形指南 | PATCHES 面板底部新增 `WAVE TYPES`，列出 14 种波形及中文说明 | `wave_guide_lists_every_wave_type` |
| 0028-5 | Hover 提示 | `Action::SetWave(wave)` 的 tooltip 直接显示该波形说明 | 现有 tooltip 路径 |

非目标：参数分组折叠、搜索参数、i18n 资源化；中文文案暂写在代码中。

验收命令：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --selfcheck
./scripts/preflight.sh
```

实测（2026-11-15）：

- `cargo test` 152 passed；`--selfcheck` 2 frames / 2581 commands / 137 text runs。
- 参数行高 38px，滑块轨道在 `top+30`；副标题使用 SMALL/FAINT。
- WAVE 选项下方显示当前波形解释；PATCHES 底部列出 14 种 `WAVE TYPES`。
- `./scripts/preflight.sh` 通过；工程 schema / DSP / 导出路径未改。
