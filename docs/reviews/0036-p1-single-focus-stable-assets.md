# Review 0036 — 架构 P1：单一焦点 / 稳定资产 ID

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass on the current tree)
- scope: `src/app.rs`, `src/app/{input,commands,audio_bridge}.rs`, `src/ui.rs`, `src/ui/{layout,paint}.rs`, `src/library.rs`, `src/presets.rs`, `src/music_bank.rs`, `src/project_io.rs`, `docs/PLAN.md`
- diff_base: `142559d` (unified library; working tree before 0034)
- source_hash: ad832ad3e64dc896d814390409b1883a0626f6d0efa6677ee1a5b54dd88caf38
- status: approved

## 变更摘要

对应 0034 的 P1：删除统一 Library 之后的第二套焦点/滚动状态，并让资产选择
使用稳定 ID。

1. **删除 `LibraryMode`**：只读/预设面板由 `State::preset_focus()` 与
   `preset_library_focus()` 派生。项目、预设、音频、音乐不再有独立模式字段。
2. **删除 `selected_preset`**：`selected_preset_index()` 从
   `LibraryKey::Preset(id)` 反查；导出、重命名、删除、Space 试听共用同一来源。
3. **删除旧滚动**：`patch_scroll` / `preset_scroll` / `ensure_preset_visible`
   移除，统一列表只保留 `library_scroll`。
4. **稳定资产 ID**：新增 `AssetId = u32`。
   - `AudioBank` / `MusicBank` 按相对/绝对路径复用 ID；
   - `PresetBank` 按 JSON 路径复用 ID，重命名保留 ID 并更新路径映射；
   - `LibraryKey::{Audio,Preset,Music}` 从索引改为 ID；
   - 重扫后 `clamp_selection` 用 ID 校验，而不是 `get(index).is_some()`。
5. **单一过滤谓词**：`filters_match()` 同时服务统一条目和
   `patch_visible` / `preset_visible`，删除重复的 `matches_filters`。
6. **选择跨重扫**：新增测试证明插入一个排序更靠前的文件后，AUDIO/PRESET
   选中项仍解析到原文件。

## 自审发现（已修）

1. `PresetBank::load_project` 最初从 `self.next_id` 继续发号，导致
   `State::demo` 的 session bank（ids 0..N）之后加载磁盘预设时，第一个预设拿到
   id=N。改为每次重扫从既有 `path_ids` 的最大值 + 1 开始，首次加载从 0 开始。
2. `AudioBank` / `MusicBank` 同样改为从 `path_ids` 推导下一个 ID，避免
   “新建工程后 ID 跳号”与测试字面量不一致。
3. `PresetBank::rename` 需要同步 `path_ids`，否则重扫会丢失原 ID；已在写文件后
   更新映射。
4. `clamp_selection` 原来只校验 AUDIO，未校验 PRESET/MUSIC；三者都改为 ID 校验。
5. `load_sample` 重扫 AUDIO 时原来按路径找回选中项，现改为按 ID 保留；
   文件消失时留待 `clamp_selection` 清空。
6. 删除 `LibraryMode` 后，`Space` 的预设分支改用
   `preset_library_focus()`：类型筛选为 PRESET 且未选中时提示“先选预设”，
   不会误播项目 patch。

## 第二审查发现

### Blocker

无。

### Major

1. **AssetId 仅会话内稳定**：工程 JSON 仍保存路径/名称，不保存资产 ID。
   这是有意的：不改变 schema。跨会话选择不恢复，符合当前“打开工程重置选择”的行为。
2. **重命名保留 ID**：`PresetBank::rename` 更新路径映射；删除后重扫会丢弃该 ID，
   恢复文件会得到新 ID。可接受，因为删除已经清空选中项。
3. **`PresetBank::from_patches` 的 ID 与磁盘加载不同源**：session bank 用
   0..N；一旦 `load_project` 会重新按路径发号。测试覆盖了磁盘路径；session-only
   场景不涉及跨重扫选择。
4. **`patch_visible` / `visible_patches` 仍保留**：它们现在共用
   `filters_match()`，不再有第二套过滤逻辑；后续 Command/UI 重构可再收敛。
5. **`MusicBank::get_id` 未保留**：只需 `index_of`，避免 dead code。

### Minor

1. `AssetId` 是 `u32` 类型别名，不是 newtype；变体名（Audio/Preset/Music）已提供
   类型区分。若后续要更强类型安全，再引入 newtype。
2. `LibraryKey` 的 ID 与 `Sound(u32)` 的 patch id 共用 `u32` 表示，但 variant 不同，
   不会误用。
3. 存储路径在 Windows 下统一 `/`；ID 复用使用同一字符串，跨平台一致。
4. `PresetBank::load_project` 对坏 JSON 不分配 ID；下次修复文件会拿到新 ID，
   符合“该行原本不可用”的预期。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 186 passed（181 → 186）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `ad832ad3…`

新增测试：

- `audio_ids_are_stable_across_rescans`
- `preset_ids_are_stable_across_rescans`
- `music_ids_are_stable_across_rescans`
- `audio_selection_survives_a_rescan_with_new_files`
- `preset_selection_survives_a_rescan_with_new_files`

## 残余风险与后续

- Command 表、`App::edit` 事务、Layout 缓存、Overlay 栈、Transport 统一仍属
  P2–P4。
- `visible_patches` / `visible_presets` 仍作为便捷索引 API 存在；等 Command/UI
  重构时再决定是否删除。
- 未做真实窗口手测（本环境无窗口）；ID/选择行为由纯状态测试覆盖。

## 签署

- reviewer_1: approved
- reviewer_2: approved（对抗性检查：路径复用、重命名/删除、排序插入、
  ID 冲突、`clamp_selection`、Space 预设语义）
