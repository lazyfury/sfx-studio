# SFX Studio — Rust 端整改计划（v1）

状态：**P0 + P1 全部完成；P2 已落地参数元数据单表、对数映射、CLI/project_io 拆分、系统菜单、首帧延迟渲染与 0005 UI 打磨（Lucide 图标、面板边距、列表行布局）；app/ui 主体拆分与后台渲染见 Backlog**。
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
| P2-4 | 参数元数据单表（`FieldSpec`：label/unit/range/scale/format） | ✅ 已完成（inspector 分组仍手写，Phase 2 继续） |
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

审查记录：`docs/reviews/0001-rust-hardening.md`、`0002-field-spec.md`、`0003-module-split.md`、`0004-ui-menu-deferred-render.md`、`0005-ui-icons-margins.md`（status: approved，source_hash 匹配）。

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                 # 48 passed
cargo run -- --selfcheck   # 2 frames / 2537 commands / 118 text runs
./scripts/preflight.sh
./scripts/review_gate.sh   # 需要 docs/reviews/*.md 中 approved + 匹配 source_hash
```

导出链路端到端测试（临时目录）：

- `bake` 写出 `audio/baked/*.wav` + `music.wav` + `.wav.import` + `sfx_manifest.json`。
- `export_tres` 写出 `audio/sfx/*.tres` + `audio/music.tres`。
- 重新 `import_project` 后，`resource_name`、`group → Category`、`tone_env`、`normalize=false` 保持。
- 重名/中文名通过 `unique_names` 生成 `coin`、`coin_2`、`sound`、`sound_2`。

## 3. Backlog（下一迭代，按依赖排序）

1. **参数注册表**：`FieldDescriptor { key, label, unit, range, scale, get, set, visible_when }`，同时驱动 inspector 与 `LayerField/MusicField/ClipField`，消除四处重复。
2. **参数注册表（剩余）**：inspector 的字段分组/可见性改由 `visible_when` 表驱动，删除 `inspector_items` 中按波形手写的分组。
3. **状态拆分（继续）**：`State { doc, view, playback, ui }`；`app.rs` 拆 `commands/input/audio_bridge`；`ui.rs` 拆 `layout/hit/paint/*`。（`cli.rs` / `project_io.rs` 已完成）
4. **后台渲染**：`RenderRequest { revision, tab, rate }` → worker → `RenderResult { generation, samples, envelope, peak }`；UI 只接受最新 generation，拖动期间不阻塞。
5. **稳定 ID + 迁移 v3**：`Patch/Layer/Clip.id`；选区/撤销/导出引用 ID；`Project.version` 升 3 并提供迁移。
6. **音频 RT 化**：无锁 ring buffer、设备热插拔、48k 固定渲染 + `rubato` 重采样、欠载统计。
7. **跨语言一致性**：PCG32 测试向量、相位回绕、f32/f64 策略、容差 golden（见 `docs/PARITY.md`）。
8. **采样波形**：`Wave::Sample` + WAV 导入 + `sample_pitch/loop` 导入导出。
9. **i18n 与可访问性**：文案表、焦点遍历、键盘可达全部操作。
10. **CLI 产品化**：`clap` 子命令、`--golden` 渲染校验、批量导出接入游戏仓库 CI。
11. **UI 体验跟进（0005 审查遗留）**：拖动 Clip 时冻结时间线比例、收藏星标点击切换、音频 playhead 无锁读取、自适应时间线/波形高度、时间线缩放/网格、图标表一致性测试。

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

## 9. Definition of Done（每个 PR 必须满足）

- [ ] `docs/PLAN.md` 对应条目状态更新。
- [ ] `scripts/preflight.sh` 全绿（fmt / clippy -D warnings / test / selfcheck）。
- [ ] 新增或修改行为有对应测试；无法自动化的写入手测清单。
- [ ] 独立第二审查完成，`docs/reviews/NNNN-*.md` 写入 `status: approved` 且 `source_hash` 匹配。
- [ ] `scripts/review_gate.sh` 通过（pre-push 强制；共享 runner 可执行 `scripts/ci.sh`）。
- [ ] 数据格式/导出行为变化时，同步更新 `docs/FORMAT.md`、`docs/PARITY.md`。
