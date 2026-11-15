# Review 0033 — MUSIC 文件库 / 空目录初始化 / 右侧检查器修复

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass)
- scope: `src/music_bank.rs`, `src/model.rs`, `src/library.rs`, `src/app.rs`, `src/app/{commands,input,audio_bridge}.rs`, `src/project_io.rs`, `src/cli.rs`, `src/demo.rs`, `src/ui.rs`, `src/ui/{layout,paint,hit}.rs`, `src/presets.rs`, `docs/{PLAN,FORMAT}.md`, `README.md`
- diff_base: `753c41c` (add CURVE wave with editable cycle and amp curves)
- source_hash: eedd5a34961c1f5180949164d1adc5565dcad819ce17609de470c7201f132249
- status: approved

## 变更摘要

- **移除伪 MUSIC 条目**：左栏不再生成 `PROJECT MUSIC`；MUSIC 只列出
  `<project>/music/**/*.json` 的真实文件。
- **新增 `MusicBank`**：每个文件是一个 `Music` JSON；文件名作为库显示名，
  `Music::validate()` 与工程共享 clamp/校验；坏 JSON 只 warning 并跳过。
- **点击 MUSIC**：加载文件到 `Project.music`（可撤销，单条 undo，重复点击
  同一文件不产生空 undo），切到 MUSIC tab 并 bump 视图，右侧 MUSIC 检查器
  立即更新。类型下拉切到 MUSIC 也会载入首个文件。
- **新目录初始化**：`create_project` 写 22 个预设（分类目录）+ 3 个 demo music
  JSON；新增 `project_io::open_or_create`，GUI OPEN、启动目录选择、
  `--project` 对空目录会直接初始化工程，不再要求命令行。
- **新工程当前 music**：初始化为 `neon_drive`，与写入的 music 文件一致。
- **交互回归修复**（承接 0032）：
  - 方向键只移动选中项，不再试听/载入；
  - 类型筛选同步 selected key / mode / export 目标；
  - 列表缩短后 scroll clamp；
  - 折叠目录不会被选中项强制展开；
  - AUDIO/MUSIC 选中回到 Project 模式；
  - 打开工程清空 folded folders。
- schema 仍为 v4；`Project.music` 仍是当前唯一音乐；DSP/导出路径未改。

## 自审发现（已修）

1. **MusicBank 未接入 State**：最初只写文件没有加载；`open_folder` 增加
   `music_bank.load_project`，新工程 toast 输出 presets/music 数量。
2. **Music 行点击只切 tab 不载入**：`activate_library_row(Music)` 现在调用
   `load_music`；`select_music` 也校验 index 并设置 tab/mode/selection。
3. **类型下拉 MUSIC 不更新右侧**：`pick_kind(Music)` 对首个可见 music 使用
   `activate_library_row`，因此载入并刷新检查器。
4. **重复点击同一 music 产生空 undo**：MusicBank 载入前比较
   `Project.music`，相同则只切 tab/toast，不 push undo。
5. **空目录只能报错**：`open_or_create` 在空目录执行 create_project 后打开；
   启动器、OPEN、`--project` 共用。
6. **旧平铺预设兼容**：MusicBank 独立，不影响 0031 的递归预设/折叠目录路径。
7. **`Music::validate` 抽取**：避免工程与非工程 music 文件两套 clamp 逻辑。
8. **新工程当前音乐**：`Project::starter` 后设置为首个 demo track，避免
   MUSIC 文件库与当前 138 BPM 默认值不一致。

## 第二审查发现

### Blocker

无。

### Major

1. **MUSIC 文件是 arrangement JSON，不是音频 WAV**：如果用户期望在 MUSIC
   里放成品 WAV，需要把 `music/*.wav` 映射到 AUDIO 或新的媒体条目；本轮按
   当前编辑器的 `Music` 数据结构实现。
2. **没有 GUI 内 music 文件的新建/重命名/删除**：文件由工程初始化或手工放入
   `music/`，点击载入；生命周期 UI 未做。
3. **方向键选中 music 后 Space 仍播放当前已载入的 `Project.music`**：只有
   点击/下拉激活才会载入选中的文件。避免键盘浏览触发文档修改。
4. **没有多 music 同时编排/播放**：`Project.music` 仍是单份当前音乐。
5. **AUDIO/PRESET 试听仍同步解码**：长文件会阻塞 UI；后台 worker 仍待做。

### Minor

1. MUSIC 文件夹标题与 PRESET/AUDIO 同名时没有类型前缀（如 `MUSIC · levels`）。
2. 折叠状态不持久化；重新打开工程恢复展开。
3. `.trash` 恢复对非 UTF-8 路径会退化为文件名。
4. 新工程不会把已有 `music/` 文件设为当前 music，只写 demo 模板。
5. `AudioBank.root` 仍未读；`MusicBank` 没有文件监听。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 174 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `eedd5a34…`
- [x] `--new`：1 sound、22 presets、3 music、baked audio；`--dump` schema v4
- [x] `open_or_create` 空目录测试：自动生成工程/22 预设/3 music
- [x] `music_rows_load_a_project_music_file`
- [x] `clicking_a_music_row_switches_to_the_music_tab`
- [x] `writes_and_loads_music_files` / `malformed_music_is_reported_and_skipped`
- [ ] 真实窗口点击 music 行与右侧检查器手测（本环境无窗口，由状态/命中测试覆盖）

## 残余风险与后续

- GUI music 文件生命周期（新建/重命名/删除/另存）。
- 将 `music/*.wav` 纳入 AUDIO 或媒体库；多 music 编排。
- 文件夹标题加类型前缀；折叠状态持久化。
- 试听/解码后台化。

## 签署

- reviewer_1: approved
- reviewer_2: approved
