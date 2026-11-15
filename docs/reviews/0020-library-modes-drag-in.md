# Review 0020 — 工程目录 / 工程内预设 / 拖入

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/presets.rs`, `src/project_io.rs`, `src/model.rs`, `src/app.rs`, `src/app/{commands,input,audio_bridge}.rs`, `src/ui.rs`, `src/ui/{layout,hit,paint}.rs`, `src/cli.rs`, `src/main.rs`, `docs/{PLAN,FORMAT}.md`, `README.md`
- diff_base: `ba3b1a235f26fad01e0f348f94e57840367d99d0`
- source_hash: 6c1d306f2b41a0cce35144eb6eb01eddc7f1c72de04fa47a8362d0c778b8ca8c
- status: approved

## 变更摘要

- **工程必须以目录为单位打开**：交互启动不再自动加载 demo；先弹目录选择
  （或 `--project DIR`）。取消则进入 `State::empty()` 空状态，中央提示
  `OPEN A PROJECT / MENU → NEW PROJECT`。`State::demo()` 只保留给测试与
  headless selfcheck。
- **PRESETS 改为工程内自带**：`src/presets.rs` 从当前工程目录的
  `presets/*.json` 逐个读取 `Patch`，不再有编译进二进制的全局 factory bank。
  预设收藏存 `presets/.favorites.json`，随工程移动/复制。
- **新建工程**：GUI `MENU → NEW PROJECT…` 与 CLI `--new DIR` 共用
  `project_io::create_project`，写 1 个 starter patch 的 `sfx.project.json` +
  11 个 `presets/*.json` + 可选 bake。已存在工程文件或非空 `presets/` 时拒绝
  覆盖；当前工程 dirty 时先弹确认。
- **双库模式**：左栏 `PROJECT | PRESETS` 只切换数据源；中央时间线、工程文档、
  undo 不变。
- **预设试听**：单击试听，独立 `preset_playing` 标记，不拖动工程播放头。
- **拖入**：4 px 阈值区分单击/拖拽；拖到时间线复制 layers 到当前 patch 并锚定
  落点；拖到左栏复制整份预设为工程 patch、名称去重并切回 PROJECT；整次 drop
  一条 undo。多图层全部展开；无 clip 层生成显式 clip，有 clip 层保留相对编排。
- **只读边界**：PRESETS 模式禁用 New Sound/Rename/Delete/Duplicate/Randomize/
  Vary/Favorite/Export/Bake/TRES；菜单置灰，快捷键走同一 `action_available`。
  Save/Open/播放/图层编辑保留。
- **上限**：拖入前检查 32 层 / 512 patch，避免保存时静默截断。
- 工程 schema 仍为 v4；DSP、CLI dump/bake、`.tres` 导出路径未改。

## 自审发现（已修）

1. **最初把 PLAN 默认理解成全局编译预设库**：修正为工程目录内
   `presets/*.json`，并删除全局 config path / `SFX_STUDIO_CONFIG`。
2. **启动仍自动 demo**：改为 `State::empty()` + 启动目录选择；测试/selfcheck
   显式使用 `State::demo()`，避免自动加载污染。
3. **打开工程未加载预设**：`open_folder` 现在调用
   `PresetBank::load_project(folder)`，并把解析警告并入状态栏/toast。
4. **新建工程没有预设目录**：`create_project` 写 `presets/`；`--new` 输出
   `N sounds · N presets`，CLI 不再把“写了预设”当作 warning。
5. **预设 JSON 不可信**：加载后复用 `Project::validate()` clamp NaN/巨值、
   重名去重、数组上限，避免坏文件进 DSP。
6. **预设文件重名**：`write_presets` 对文件名做大小写去重（`same_2`），
   测试覆盖。
7. **空工程 UI 无提示**：header/时间线/状态栏显示 `NO PROJECT` 与
   OPEN/NEW PROJECT 提示；测试 `empty_project_paints_open_hints`。
8. **SfxLogic 覆盖状态栏**：加载工程后状态栏不再被 “OPEN a project folder”
   覆盖；区分 project loaded / unsaved project / no project。
9. **空工程仍渲染默认 Patch**：`preview_pending`/`ensure_cache` 在没有工程
   patch 时返回空缓存，避免空状态显示/播放默认正弦波。
10. **NEW PROJECT 可能覆盖预设**：`create_project` 除工程文件外，还会拒绝
   非空 `presets/` 目录，避免 `write_presets` 覆盖已有预设。
11. **启动失败提示被音频状态覆盖**：启动目录选择失败改用 toast（构建 app 后
   仍保留），不再只写 `ui.status`。
12. **拖拽/试听/undo/ID/上限**：pointer-up + 4 px 阈值、`preset_playing`
   隔离播放头、`begin_edit/mark_dirty/commit_edit` 单条 undo、复制前清 ID、
   32/512 上限。

## 第二审查发现

### Blocker

无。

### Major

1. **仍是复制，不是引用**：拖入后修改 `presets/*.json` 不会影响已插入的
   layers；一个 track 不能放不同音色。引用式 clip 需要 schema/DSP/Godot 导出
   整体改造，已写入 PLAN 0020「非目标」。
2. **只读边界不是全局冻结**：PRESETS 模式仍允许编辑当前工程 patch 的
   layer/clip/参数（中央区始终是工程）。若要求完全冻结工程编辑，需要给
   inspector/header 增加统一的 disabled 状态。
3. **当前 patch 的 DSP/空间不随预设改变**：拖入只复制 layers。v2 工程拖入 v3
   预设仍走 legacy 干声，直到 `UPGRADE TO HD`；已在 README/FORMAT 写明。
4. **预设试听同步渲染**：当前工程内预设较短，可接受；长混响/用户长预设后续
   应接入 `app/render.rs` 的 worker。
5. **没有 `SAVE AS PRESET`**：目前只能由 NEW PROJECT 初始化 `presets/`，或在
   文件系统里放 Patch JSON。已列入非目标/后续。

### Minor

1. 拖拽没有自动滚动；列表很长时需要先滚到目标预设。
2. PRESETS 模式没有独立预设详情检查器；中央时间线仍显示工程 patch。
3. 切换库只有鼠标点击，无键盘快捷键。
4. 预设收藏以名称为 key；重命名预设文件内容会丢失收藏。
5. 外部 Finder WAV 拖放仍未支持；igui `InputEvent` 没有文件拖放事件。
6. 启动目录选择使用 `rfd`，未在无窗口 CI 路径测试；可用
   `SFX_STUDIO_NO_PICKER=1` 跳过。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 123 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `--new /tmp/...`：1 个 starter patch、11 个 `presets/*.json`、baked 音频；
      `--dump` 仍为 schema v4
- [x] 工程 schema 未变；bake/`.tres` 代码路径未改
- [x] 单测覆盖：空工程（含不渲染默认 patch）、工程目录加载预设/收藏、预设写入去重、
      模式切换/过滤隔离、试听状态、时间线/左栏 drop、ID 重发、undo、拖拽 ghost 绘制
- [ ] GUI 窗口内手动目录选择与指针拖拽（本环境无窗口；由
      `create_project`/`open_folder`/`drop_preset`/paint 单测与 headless
      selfcheck 覆盖）

## 残余风险与后续

- 引用式 clip / 工程级多轨编排：需要新的 `Arrangement/Track/SoundClip` 与
  schema v5，暂不做。
- `SAVE AS PRESET`：把当前 patch 写回 `<project>/presets/`，下一阶段。
- 预设试听接入后台 worker；长预设/长尾音时避免 UI 阻塞。
- 拖拽自动滚动与库切换快捷键。
- 外部 WAV 拖放需要平台层扩展；当前先保留 `LOAD WAV…`。

## 签署

- reviewer_2: approved
