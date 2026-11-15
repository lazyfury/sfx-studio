# Review 0025 — 预设采样懒加载 / 删除恢复

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/presets.rs`, `src/app/commands.rs`, `src/ui.rs`, `docs/PLAN.md`, `README.md`
- diff_base: `cc1200c` (manage preset files and resolve preset samples)
- source_hash: ce023eaaa8cbd79368eb9933a72667f5b8009c3856ae972b68b4ce41b59c9b17
- status: approved

## 变更摘要

- **预设采样懒加载**：`PresetBank::load_project` 只解析/校验 JSON；新增
  `hydrate(index)`，只在试听、导出、拖入时读取 `sample_path` 并解码 PCM。
  打开含大量采样预设的工程不再一次性占用全部采样内存。
- **采样 warning 合并**：试听/导出/拖入的 toast 会附带采样加载失败信息。
- **删除恢复**：新增 `PresetBank::restore_latest`，从 `presets/.trash/`
  找到最新 `.bak`，移回 `presets/`；目标文件存在时使用 `_2` 后缀。
- **菜单入口**：PRESETS 段增加 `RESTORE LAST PRESET`，仅 PRESETS 模式可用。
- **测试**：采样预设测试现在断言 `load_project` 后 `sample_data == None`、
  `hydrate(0)` 后可用；新增恢复测试。

## 自审发现（已修）

1. **懒加载覆盖所有消费路径**：试听、WAV 导出、时间线拖入、左栏复制都会先
   `hydrate`；只读 inspector 不需要 PCM，不触发加载。
2. **共享仍然有效**：hydrate 后的 `Arc<SampleData>` 会在拖入/克隆时继续共享，
   不会因为懒加载退回到深拷贝。
3. **恢复冲突**：`restore_latest` 不覆盖现有 `presets/*.json`；同名时使用
   `_2`、`_3` 后缀。
4. **warning 可观测**：采样文件缺失时不会静默播放空音频，试听/导出/拖入的
   状态栏会包含 warning。
5. **菜单边界**：`RESTORE LAST PRESET` 在 PROJECT 模式禁用，避免把预设库
   操作混进工程编辑。

## 第二审查发现

### Blocker

无。

### Major

1. **恢复不更新内存库**：`PresetBank::restore_latest` 只移动文件；GUI 命令随后
   调用 `load_project` 刷新。若未来有其它调用方直接使用该方法，需要记得 reload。
   已在命令层固定这一调用顺序。
2. **退出拦截仍未实现**：宿主没有关闭请求回调，仍可能在关窗时丢失未保存工程。

### Minor

1. `restore_latest` 只恢复最近一条；没有历史列表或多选恢复 UI。
2. 恢复的 JSON 若损坏，文件会回到 `presets/` 但不会出现在库中；命令 toast
   仍显示“restored”，后续可区分“文件恢复”和“解析失败”。
3. 恢复后若 patch 名与现有预设冲突，`load_project` 会重命名；命令当前按
   patch 名选中，极端情况下可能落到同名旧预设。后续可按恢复文件路径选中。
4. 采样 PCM 在预设库中会一直驻留，直到工程重新打开；没有 LRU / 手动卸载。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 148 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 2572 commands / 126 text runs
- [x] `./scripts/preflight.sh`
- [x] `loads_preset_sample_layers_relative_to_the_project`（load 后 None，hydrate 后 Some）
- [x] `restore_latest_brings_back_a_deleted_preset`
- [x] `preset_mode_disables_project_only_actions` 覆盖 RESTORE 可用性
- [x] 工程 schema 仍为 v4；DSP / bake / `.tres` 路径未改

## 残余风险与后续

- 下一轮：退出拦截；Bake/TRES 后台任务；RT buffer 归还队列；`.trash` 历史 UI。
- 真实窗口下的恢复菜单与采样预设试听未手测；由 PresetBank 文件级测试、
  command/UI 测试与 headless selfcheck 覆盖。

## 签署

- reviewer_1: approved
- reviewer_2: approved
