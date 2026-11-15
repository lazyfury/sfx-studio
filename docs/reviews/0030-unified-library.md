# Review 0030 — 统一 Library：sound / audio / preset / music

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/library.rs`, `src/app.rs`, `src/app/{commands,input,audio_bridge}.rs`, `src/ui.rs`, `src/ui/{layout,paint,hit}.rs`, `src/project_io.rs`, `src/cli.rs`, `src/main.rs`, `docs/{PLAN,FORMAT}.md`, `README.md`
- diff_base: `753c41c` (add CURVE wave with editable cycle and amp curves)
- source_hash: ca14cbb30c19980cd7914ea5450a17c6e8cc484d9852808e45a2bf5c98be208c
- status: approved

## 变更摘要

- **统一左栏条目**：新增 `src/library.rs`，用 `LibraryKind::{Sound,Audio,Preset,Music}`、
  `LibraryKey`、`LibraryEntry` 描述四类资源。`LibraryKey` 是 `Copy` 小枚举，
  可直接进入 `Hit`/`Drag`，不再用「当前 LibraryMode 解释同一下标」。
- **四类数据源**：
  - SOUND = `Project.patches`
  - AUDIO = `<project>/audio/**/*.wav`，跳过隐藏文件与生成的
    `audio/baked/`、`audio/demo/`（`AudioBank`）
  - PRESET = `<project>/presets/*.json`
  - MUSIC = 单份 `Project.music` + `demo::tracks()` 三个内置 track
- **筛选**：左栏标题条新增类型下拉（`ALL/SOUND/AUDIO/PRESET/MUSIC`）与 `×`
  清除按钮；保留搜索、分类下拉、收藏星标。`×` 清空类型 + 分类 + 搜索 + 收藏。
  分类/收藏只对 SOUND/PRESET 生效。
- **行行为**：SOUND 点击选中；PRESET 点击试听、拖入时间线/左栏（保留原行为）；
  AUDIO 点击试听、拖到时间线生成一个 `Wave::Sample` 层（工程相对路径 +
  5 ms 吸附 + 单条 undo）；MUSIC 切到 MUSIC tab，demo track 点击载入
  `Project.music`（可撤销）。
- **只读与导出边界**：AUDIO/MUSIC/PRESET 选中时工程编辑命令按只读处理；
  `EXPORT WAV` 只在选中 SOUND/PRESET 时可用。
- **工程打开**：`open_folder` 重新扫描 `audio/` 并重置统一筛选/选区；
  `LOAD WAV` 复制采样后刷新 audio bank。
- **自检**：headless required labels 从已移除的 `PROJECT/PRESETS` 模式按钮改为
  `ALL/SND`。
- 工程 schema 仍为 v4；DSP、`--dump/--bake/--export`、Godot `.tres` 路径未改。

## 自审发现（已修）

1. **旧 mode 兼容层**：保留 `LibraryMode`/`set_library_mode` 供预设导出、只读
   边界和既有测试使用，但左栏 UI 已不再绘制两个模式按钮；避免一次性拆掉
   导出/检查器语义。
2. **预设筛选回归**：`refresh_filter` 最初在 PRESETS 模式仍会
   `keep_selection_visible`，导致搜索时自动改选工程 patch；已按旧语义禁止。
3. **stale 选区**：删除/重装预设、重扫 audio bank 后 `selected_library` 可能
   指向已不存在的 index；删除/安装时清空，`clamp_selection` 对失效
   Sound/Preset/Audio key 回落到当前 Sound 或 `None`。
4. **WAV 导出可用性**：最初只看 `selected_preset`，选中 AUDIO 后仍可能显示可用；
   改为要求 `selected_library == Preset`。
5. **音频试听采样率**：`AudioEngine` 按 48 kHz 源帧消费；任意 WAV 在试听前
   线性重采样到 `PREVIEW_RATE`。
6. **AUDIO drop 边界**：检查 SOUND tab、当前 patch、32 层上限；生成显式 Clip
   并复用单条 undo，避免插入后保存被静默截断。
7. **左栏拖拽 ghost**：AUDIO 拖到左栏不再显示 “COPY TO PROJECT”，只提示
   拖到时间线。
8. **LOAD WAV 后刷新**：复制到 `audio/sfx/samples/` 后重扫 bank，并按原路径
   恢复 AUDIO 选区，避免索引移动后选错文件。
9. **selfcheck 标签**：移除模式按钮后同步更新 required labels，避免门禁假失败。

## 第二审查发现

### Blocker

无。

### Major

1. **music 仍是单份**：MUSIC 行中的 “PROJECT MUSIC” 就是 `Project.music`；
   demo track 是内置模板。真正多首 music 需要 `music/` 目录或
   `Project.music_tracks`，即 schema v5，本轮未做。
2. **sound 指 Patch，不是时间线 Clip**：Clip 仍属于 Layer，不能作为独立库
   资源插入/排序；独立 Clip 库需要新的资源模型。
3. **AUDIO 只扫描工程内文件**：外部 Finder 拖放仍不可用；用户需用
   `LOAD WAV` 或手动放入 `<project>/audio/`。没有文件监听，磁盘变化需重新打开。
4. **同步解码试听**：AUDIO/PRESET 试听在 UI 线程解码，最长 60s 的 WAV 可能
   造成短暂卡顿；后续应接入后台 worker。
5. **每帧重建 entry 列表**：`all_library_entries` 会克隆名称并按需读取 demo
   tracks；当前 512 patch + 少量 audio/preset 规模可接受，超大 audio 目录应加缓存。

### Minor

1. 类型筛选没有键盘快捷键；只有鼠标点击下拉。
2. `C` 从 AUDIO/MUSIC 过滤态会先清类型再循环分类，但没有专门 toast 说明。
3. AUDIO 行只显示文件大小，不显示时长/波形缩略图。
4. 点击 demo track 会立即替换 `Project.music`（可撤销），不是非破坏性试听。
5. 没有多选类型或“仅未收藏”之类的组合筛选。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 162 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3949 commands / 191 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `ca14cbb3…`
- [x] `library_keys_keep_their_kind`
- [x] `audio_bank_scans_wavs_but_skips_generated_folders`
- [x] `unified_library_lists_all_four_kinds_and_filters_them`
- [x] `dropping_audio_on_the_timeline_creates_a_sample_layer`
- [x] `music_rows_load_the_project_or_a_demo_track`
- [x] `kind_menu_lists_every_resource_and_clear_is_clickable`
- [x] `--new /tmp/...`：schema v4、22 presets、`audio/baked`；`--dump` 正常
- [x] AUDIO 扫描跳过 `audio/baked`；分类/收藏/搜索/类型过滤单测覆盖
- [ ] 真实窗口鼠标拖拽/下拉手测（本环境无窗口；由 headless + 单测覆盖）

## 残余风险与后续

- schema v5 多 music / 独立 Clip 库 / 外部文件拖放。
- AUDIO 试听与预设试听接入后台渲染/解码 worker。
- 为超大 `audio/` 目录增加 bank 缓存与文件监听。
- AUDIO 行显示时长、采样率、波形缩略图；支持删除/重命名素材文件。
- 类型筛选快捷键与组合筛选（类型 + 收藏 + 分类）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
