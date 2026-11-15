# Review 0037 — 架构 P2a：命令元数据 / revision 事务

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass on the current tree)
- scope: `src/ui.rs`, `src/app.rs`, `src/app/commands.rs`, `docs/PLAN.md`
- diff_base: `142559d`（统一 Library 前的基线；本轮基于 0036）
- source_hash: b7e3e8cf93774a4bc911d5c913be1ca60eac98792a1785c4801c9c766959a08d
- status: approved

## 变更摘要

对应 0034 的 P2 前半：命令元数据 + 事务化 undo。

1. **`ActionMeta` 表**：新增 `ActionScope`
   （Document / ProjectEdit / AssetEdit / Transport / View）、
   `ActionMeta { scope, undo }` 与 `action_meta(action)`。
   新增 `Action` 时编译器强制补分类。
2. **可用性收敛**：`action_available` 只保留三个目标特例
   （Rename/DeletePreset、RestorePreset、ExportWav），其余由 scope 派生。
   `is_project_edit()` 变成 `action_meta(action).scope == ProjectEdit`。
3. **撤销策略收敛**：`activate` 删除手写 `snapshot` 白名单，改用
   `action_meta(action).undo`；标签从 `"edit"` 变为 `"randomize"`、
   `"delete layer"`、`"change wave"` 等。
4. **revision 事务**：`DocState.pending_edit` 从 `Option<Project>` 改为
   `Option<PendingEdit { project, version, music_version }>`；
   `commit_edit` 只比较 revision，不再 `before != project`。
   这消除了指针松手时对 `Arc<SampleData>` 的全量 PCM 比较。
5. **测试**：新增 `action_meta_is_the_single_scope_and_undo_source`；
   更新 `undo_transactions_ignore_noop_gestures`，直接字段变更后必须
   `mark_dirty()`（revision 事务的前提）。

## 自审发现（已修）

1. `LoadSample` 不在表里压 undo（由 `apply_sample` 自带事务），
   `UpgradeDsp`、`NewPatch`、`RenamePatch` 也由内部/模态自行处理；
   这些使用 `undo: None`，避免空快照。
2. `Favorite` 保留 `Some("favorite")`，与星标路径一致；PRESET 焦点下
   仍是 ProjectEdit，不可用。
3. `DemoTrack` 之前标签是 `"edit"`，现在是 `"load demo track"`。
4. `ExportWav` 标为 Document，但可用性仍由 `export_patch()` 目标判定；
   AUDIO/MUSIC 焦点继续返回不可用及明确提示。
5. `action_available` 的 `_ => true` 覆盖 Transport/View/AssetEdit 的
   目标特例之外命令；AssetEdit 的三项已单独处理，不会漏。

## 第二审查发现

### Blocker

无。

### Major

1. **revision 事务依赖 `mark_dirty` / `mark_music_dirty`**：直接改字段而忘记
   bump revision 的手势不会产生 undo。现有生产路径都调用 mark；测试已把
   “直接改字段”的旧写法修正为显式 mark。后续 P2b 应把字段修改封装进
   `App::edit`，让编译器而不是约定来保证。
2. **`action_meta` 与 `MENU_ENTRIES` 仍有两份命令清单**：菜单标签/顺序与
   scope/undo 分离；P2b 再把 label/shortcut 合并，避免“菜单有、快捷键无”。
3. **`Action::Play` 与 Space 语义仍未统一**：Play 走 tab 文档，Space 在资产
   焦点下试听资产。属于 Transport（P4），本轮未改。
4. **`ActionScope::AssetEdit` 只有三项**：预设生命周期之外没有资产编辑命令；
   若后续加 AUDIO/MUSIC 重命名，需新增 scope 或扩展 AssetEdit 的判定。

### Minor

1. `ActionMeta.undo` 是 `Option<&'static str>`，没有单独的 `UndoPolicy` 枚举；
   当前足够，若出现“成功后撤销”/“多次合并”再升级。
2. `action_meta` 的 match 必须保持穷尽；新增变体会得到编译错误，这是预期的。
3. `PendingEdit` 的字段私有；测试只通过 `begin_edit`/`commit_edit` 操作，
   没有直接构造。
4. `push_undo_label` 仍先 `commit_edit` 再压快照；对 `activate` 路径是幂等的
   （前面已 commit 过）。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 187 passed（186 → 187）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `b7e3e8cf…`

新增/更新测试：

- `action_meta_is_the_single_scope_and_undo_source`
- `undo_transactions_ignore_noop_gestures`（显式 `mark_dirty`）

## 残余风险与后续

- P2b：`MENU_ENTRIES`、快捷键、tooltip 统一进命令元数据；`App::edit` 封装
  字段修改，彻底移除“忘记 mark_dirty”的可能。
- P3：Layout/Present 缓存 + Overlay 栈。
- P4：Transport 统一 + `Arc` 播放 buffer。

## 签署

- reviewer_1: approved
- reviewer_2: approved（对抗性检查：全部 Action 分类、undo 双压栈、
  modal 取消、revision 漏 bump、preset 只读边界）
