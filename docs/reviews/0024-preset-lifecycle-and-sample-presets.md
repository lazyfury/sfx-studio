# Review 0024 — 预设文件生命周期 / 采样预设

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/app/commands.rs`, `src/model.rs`, `src/presets.rs`, `src/ui.rs`, `src/ui/paint.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `8e47a1e` (improve preset workflow and sample memory)
- source_hash: 30a69a1defe348cc770b623b17d75d8a6a29ab26f11b2bfcd28fa4b6f2205ca7
- status: approved

## 变更摘要

把 PRESETS 从“只读展示 + 导出”补成可管理的工程内资产：

- **采样预设**：`Patch::load_samples` 从 `Project` 中抽出；`PresetBank::load_project`
  对每个预设按工程根目录解析 `sample_path`，带 `Wave::Sample` 的预设不再静音。
- **重命名预设**：`MENU → RENAME PRESET` 弹出输入框；`PresetBank::rename`
  同时更新 JSON 文件名、`patch.name` 和 `.favorites.json`，同名拒绝、已有文件
  不覆盖。
- **删除预设**：`MENU → DELETE PRESET` 先确认；`PresetBank::delete` 把 JSON
  移到 `presets/.trash/<file>.<stamp>.bak`，再更新内存库和收藏。
- **菜单分组**：新增 `PRESETS` 菜单段（SAVE / RENAME / DELETE）；PROJECT 模式
  只启用 SAVE，PRESETS 模式只对选中预设启用 RENAME/DELETE。
- **只读 inspector**：显示预设来源文件名（`FILE ...`），便于理解当前编辑对象。
- **和弦高亮**：`chord_preset_index` 按当前 bass/chords 计算 active，不再硬编码
  第一个 chip。

## 自审发现（已修）

1. **`PresetBank` 路径索引**：新增 `paths` 与 `presets` 平行保存来源 JSON；
   `rename` 写新文件后删旧文件，`delete` 移到 `.trash`，避免删除即丢数据。
2. **收藏迁移**：重命名时若旧名字在收藏中，删除旧 key、插入新 key 并保存；
   删除时同时移除收藏。
3. **文件冲突**：`rename` 使用 `preset_file_stem` + `_2` 后缀，绝不覆盖已有
   JSON；同名 preset 直接报错。
4. **空/坏库**：`load_project` 保持原有警告路径；采样加载失败只产生 warning，
   不阻断整个预设库。
5. **菜单边界**：`action_available` 对 RENAME/DELETE 额外要求 PRESETS + 选中
   预设；PROJECT 模式不可用，避免误删预设。
6. **`PresetBank::is_empty`**：补上方法，测试和空库判断不再直接摸内部 Vec。

## 第二审查发现

### Blocker

无。

### Major

1. **预设采样是 eager load**：打开工程时会把所有预设的采样 PCM 全部解码进
   内存。单文件有 64MB/60s 上限，但 22 个长采样预设仍可能占用较大内存；
   后续可做按需加载或 LRU。
2. **退出拦截仍未实现**：宿主 `AppLogic` 没有关闭请求回调，直接关窗仍可能
   丢失未保存工程；需要 igui_app 层支持。

### Minor

1. `DELETE PRESET` 进入 `.trash` 后没有恢复 UI；用户需要手动移动文件。
2. 重命名只允许改名称，不允许改分类/文件名模板。
3. 预设采样路径解析与工程 patch 共用“相对工程根目录”规则；移动工程目录后
   外部绝对路径仍可能失效，属于既有模型行为。
4. 无批量删除/批量导出。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 147 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `loads_preset_sample_layers_relative_to_the_project`
- [x] `rename_moves_the_file_and_favourite_key`
- [x] `delete_moves_the_preset_file_into_the_trash`
- [x] `deleting_a_preset_goes_through_the_confirmation_modal`
- [x] `chord_preset_index_follows_the_current_progression`
- [x] `preset_mode_disables_project_only_actions` 覆盖 RENAME/DELETE 可用性
- [x] 工程 schema 仍为 v4；DSP / bake / `.tres` 路径未改

## 残余风险与后续

- 下一轮优先：预设采样按需加载；退出拦截；`.trash` 恢复入口；Bake 后台任务；
  RT buffer 归还队列。
- 真实窗口下的重命名模态、删除确认、采样预设试听未手测（本环境无 GUI）；
  由 PresetBank 文件级测试、command/modal 测试与 headless selfcheck 覆盖。

## 签署

- reviewer_1: approved
- reviewer_2: approved
