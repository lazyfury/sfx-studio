# Review 0031 — 预设文件夹 / 折叠目录 / music 点击

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/library.rs`, `src/presets.rs`, `src/app.rs`, `src/app/{commands,input}.rs`, `src/ui.rs`, `src/ui/{layout,paint,hit}.rs`, `src/cli.rs`, `src/demo.rs`, `docs/{PLAN,FORMAT}.md`, `README.md`
- diff_base: `753c41c` (add CURVE wave with editable cycle and amp curves)
- source_hash: 76882e05e2ddc5832279f5d17eac1d179279e04a8ed9bb78c9f97472e1ed6297
- status: approved

## 变更摘要

- **预设默认文件夹**：新增 `Category::folder()`，新工程/`--new`/`--demo` 把
  22 个默认预设写入 `presets/{impact,laser,ui,movement,magic,environment,combat}/`。
- **递归预设库**：`PresetBank::load_project` 递归扫描 `presets/**/*.json`，
  跳过隐藏目录（含 `.trash`）；每个预设记录相对 `folder`。旧工程平铺
  `presets/*.json` 继续可读。
- **文件夹生命周期**：
  - `install_missing` / `write_presets` 按分类目录写入，并按名称/目标路径去重。
  - `delete` 移到 `presets/.trash/<相对路径>.<stamp>.bak`，保留目录。
  - `restore_latest` 递归找最新备份并恢复到原目录；旧平铺备份也兼容。
- **左栏折叠目录**：新增 `LibraryListRow::{Folder,Entry}`；layout/paint/hit/
  keyboard 使用同一行序列。PRESET/AUDIO 按相对目录分组，点击标题折叠/展开；
  折叠包含当前选中的文件夹不会自动弹开。
- **music 点击修复**：`select_music` 直接切到 `Tab::Music` 并设置延迟渲染；
  点击 music 行时立即激活，demo track 载入 `Project.music`（单条 undo）。
- **CLI**：`--new` 的 preset 计数改为递归加载 `PresetBank`，不再只数顶层 json。
- 工程 schema 仍为 v4；DSP、导出路径、`--dump/--bake/--export` 未改。

## 自审发现（已修）

1. **折叠目录被自动弹开**：`toggle_library_folder` 最初调用
   `ensure_library_visible`，会把包含选中项的刚折叠目录重新展开；改为只 clamp
   scroll，不再改动 folded 集合。
2. **删除嵌套预设无法恢复**：最初 `.trash` 放在预设所在子目录，而
   `restore_latest` 只扫根 `.trash`；改为统一 `presets/.trash/` 并保留相对路径。
3. **CLI 预设计数**：`--new` 仍只数顶层 json，加入子目录后会显示 0/错误；改为
   `PresetBank::load_project` 递归计数。
4. **旧平铺兼容**：读取、删除、恢复都保留旧平铺路径行为；新增测试覆盖。
5. **文件重名语义**：`install_missing` 的冲突检查从全局 stem 改为
   「分类目录 + stem」，不同分类可同名文件，但预设名称仍全局去重。
6. **music 点击**：仅设置 selected_library 不够，demo row 没有拖拽 payload，
   pointer-up 不会走激活分支；改为 `select_music` 内部切 tab，并在
   pointer-down 对 Music 行立即激活。
7. **文件夹命中**：`Hit` 仍是 `Copy`，用 `LibraryFolder(usize)` 指向
   `Layout::library_rows` 的行索引，paint/hit/input 使用同一索引。
8. **排序稳定**：统一 entry 按 `kind > folder > name` 排序，保证 folder header
   连续出现；旧平铺条目（folder=None）排在分类目录之前。

## 第二审查发现

### Blocker

无。

### Major

1. **只支持读取任意相对目录，不提供新建/移动文件夹命令**：用户可以在文件
   系统中组织 `presets/`，左栏会自动分组；GUI 内暂不能拖拽移动预设到别的
   文件夹。
2. **旧平铺预设不自动迁移**：老工程仍显示为无文件夹条目，`install-presets`
   只补缺失项，不会把已有文件搬进分类目录。
3. **文件夹 header 计数是当前筛选后的数量**：搜索/分类过滤时显示匹配数，
   不是磁盘总数；行为一致但可能与用户预期不同。
4. **AUDIO 文件夹来自相对路径**：`audio/sfx/samples` 会显示为
   `sfx/samples` header；`baked/`、`demo/` 仍被跳过。
5. **music 仍是单份工程音乐 + demo tracks**：没有多 music 列表/schema v5；
   本轮只修复点击行为。

### Minor

1. 文件夹标题没有单独的右键菜单/重命名/删除。
2. 折叠状态不持久化，重新打开工程恢复为全部展开。
3. 文件夹图标只有 chevron，没有类型图标。
4. 点击 demo track 会立即替换 `Project.music`（可撤销），不是非破坏试听。
5. `delete` 的 `.trash` 目录结构由相对路径决定；极端路径（非 UTF-8 文件名）
   会退化为文件名，可能丢失目录层级。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 168 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3949 commands / 191 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `76882e05…`
- [x] `loads_nested_preset_folders_and_reports_them`
- [x] `write_presets_uses_category_folders`
- [x] `delete_and_restore_preserve_nested_folders`
- [x] `preset_folders_group_in_the_library_and_can_collapse`
- [x] `folder_headers_are_hit_testable`
- [x] `clicking_a_music_row_switches_to_the_music_tab`
- [x] `--new /tmp/...`：22 presets 分布在 7 个分类目录；`--dump` schema v4
- [x] 旧平铺预设读取/删除/恢复测试仍通过
- [ ] 真实窗口鼠标点击文件夹/music 手测（本环境无窗口；由 headless + 单测覆盖）

## 残余风险与后续

- GUI 内新建/重命名/移动预设文件夹，及折叠状态持久化。
- 旧平铺预设的一键迁移到分类目录。
- 多 music 列表 / schema v5；独立 Clip 库。
- AUDIO 文件夹显示更细的目录树/面包屑。

## 签署

- reviewer_1: approved
- reviewer_2: approved
