# Review 0035 — 架构 P0：焦点归位 / 命令作用域 / 导出元数据

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass on the current tree)
- scope: `src/app.rs`, `src/app/{input,commands}.rs`, `src/ui.rs`, `src/export.rs`, `README.md`, `docs/PLAN.md`
- diff_base: `142559d` (unified library; working tree before 0034 P0)
- source_hash: cce778f8307e01ab030ba908c6efd8c31f5b6b5f2901a05877028f6598bd1125
- status: approved

## 变更摘要

对应 `docs/reviews/0034-architecture-audit.md` 的 P0。**不改 schema / DSP / 文件格式**，
只修交互状态边界和导出元数据。

1. **焦点归位**：新增 `State::focus_document()`。点击图层、Clip、参数、自动化点、
   波形/标尺、pattern 时清掉 AUDIO/PRESET/MUSIC 行焦点，右侧检查器回到当前文档。
   `select_layer` / `select_clip` 也调用它，覆盖内部命令路径。
2. **Tab 归位**：新增 `SfxLogic::switch_tab()`，`Tab` 键与 SOUND/MUSIC 按钮共用：
   停止旧播放、切 tab、归位焦点。选中 PRESET 后切 MUSIC 不再显示 PRESET 检查器。
3. **命令作用域**：`ui::is_project_edit()` + 重写 `action_available`。
   - PRESET 焦点：工程编辑命令禁用，但 SAVE / OPEN / BAKE / EXPORT .TRES 等文档命令可用。
   - AUDIO/MUSIC 焦点：不再禁用工程编辑；菜单编辑先 `focus_document()` 再执行。
   - WAV 导出按实际目标判定（PROJECT patch 或选中 PRESET）。
4. **筛选无副作用**：`pick_kind` 只 `select_library_row`，不再 `activate_library_row`。
   选 MUSIC 类型只选中首个文件，不自动替换 `Project.music`；筛选清空选中时同步退出
   PRESETS 模式，消除“幽灵只读”。
5. **Undo 事务**：`A::LoadSample` 移出手写 snapshot 白名单；`load_sample` 拆出
   `apply_sample`，成功后才 `begin_edit` / `commit_edit`。取消对话框不再产生空快照，
   成功载入只产生一次撤销。菜单 Favorite 加入 snapshot 白名单，与星标路径一致。
6. **导出元数据**：manifest `duration` 改用实际烘焙率；`.tres` 的 `rate` 改用
   `Patch::bake_rate()`。修复 v3 patch（`rate = 22050`、实际 48 kHz bake）的 2.18×
   时长错误。
7. **旧播放停止**：patch/tab/undo 将 preview cache 版本置 0 后，`update` 停止项目播放；
   `preset_playing` 的资产试听不受影响（`project_playback_is_stale` helper）。

## 自审发现（已修）

1. `select_layer` 直接改焦点会导致 `focus_document` 在 MUSIC tab 下把
   `selected_library` 清成 `None`；确认这是期望行为（MUSIC tab 文档编辑不应高亮
   左侧文件）。
2. `action_available` 若继续用 `library_mode` 判 PRESET，会在
   `selected_library == Preset` 但 mode 被外部置回 Project 时误判；改为
   `preset_focus` 同时看 mode 与 row key。
3. `RestorePreset` 原来只认 `library_mode == Presets`；统一 Library 下新增
   `kind_filter == Preset` 也可用，避免必须先点一行。
4. `LoadSample` 拆出 `apply_sample` 后，测试可直接构造 `SampleData` 验证
   “一次撤销”，不需要弹窗 mock。
5. 旧测试 `preset_mode_disables_project_only_actions` 把 Bake/Tres 也当作
   project-edit；与 0034 的因果分析冲突。拆成
   `preset_focus_disables_project_edits_but_not_document_commands`，并新增
   `audio_focus_does_not_disable_document_commands`。

## 第二审查发现

### Blocker

无。

### Major

1. **AUDIO/MUSIC 焦点语义改变**：以前所有工程编辑被禁用，现在可用。
   这是 0034 P0 的直接目标；README 已同步说明。若产品希望 AUDIO/MUSIC 也保持
   只读，应改回 `preset_focus` 扩展而不是恢复 blanket 判定。
2. **切 tab 现在会停止播放**：旧行为是旧 buffer 继续响。停止比“UI/声音不同步”安全，
   但会影响“切 tab 继续听”的用法；后续 Transport（P4）可提供显式继续策略。
3. **`Key::Delete` 的资产焦点守卫没有 event 级测试**：当前由
   `selected_library_kind` 三分支早退覆盖；真正的键盘事件测试要等手势测试框架。
4. **`.tres` 的 `rate` 对 baked v3 资源改为 48000**：Godot 的 `baked` 分支直接
   返回 `AudioStreamWAV`，不读该字段；但外部脚本若读 `rate`，新值才是真实烘焙率。
5. **manifest duration 修复只覆盖 bake**：`write_patch_wav`（PRESET 直接导出 WAV）
   返回秒数，未读 manifest；无需改。

### Minor

1. `LoadSample` 复制 WAV 到工程目录仍在 undo 之前；撤销只回滚引用，不删除复制文件
   （与 0023 行为一致）。
2. `apply_sample` 在 `layer_mut()` 为空时仍 `mark_dirty()`；只影响 dirty 标记，不产生
   撤销（`commit_edit` 比较项目相等）。
3. `action_unavailable_message` 的文案改为“选 SOUND 后编辑”；第五轮再做 i18n 资源化。
4. `focus_document` 在 `Tab::Music` 下清空 `selected_library`，左侧不再高亮已载入的
   music 文件；V2 若需要“当前曲目高亮”，应引入 `Project.music` 的稳定 id，而不是
   继续用 bank 索引。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 181 passed（174 → 181，新增 7 条）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `--demo` 临时工程：manifest duration 与 WAV 帧数一致
      （`pickup_coin` 0.328 s、`explosion_burst` 1.407 s）
- [x] 导出测试断言 v3 baked `.tres` 的 `rate = 48000`
- [x] `./scripts/source_hash.sh` — `cce778f8…`
- [ ] 真实窗口手测：点击图层归位、Tab 切换、MUSIC 筛选不载入（本环境无窗口；
      由状态/可用性测试覆盖）

新增测试：

- `document_clicks_reclaim_asset_focus`
- `switching_tabs_reclaims_asset_focus`
- `filtering_by_music_selects_without_loading`
- `applying_a_sample_is_a_single_undo`
- `preset_focus_disables_project_edits_but_not_document_commands`
- `audio_focus_does_not_disable_document_commands`
- `stale_project_playback_is_detected_but_auditions_are_kept`
- `bake_manifest_duration_matches_the_written_wav`

## 残余风险与后续

- 未做：删 `LibraryMode` / `selected_preset`、稳定资产 ID、Command 表、
  `Layout` 缓存、Transport 统一。它们是 0034 的 P1–P4，应在后续独立 PR 完成。
- `focus_document` 是 P0 的临时单一入口；P1 引入 `Focus` 后应替换为真正的焦点模型，
  避免继续靠“清空 + 重设 selected_library”模拟。
- 手势级测试（tab + focus + Space、AUDIO + Delete）仍需在 P2/P3 的测试框架里补。
- `action_available` 仍是手写函数；P2 应迁移到命令元数据，避免下一次语义分叉。

## 签署

- reviewer_1: approved
- reviewer_2: approved（对抗性检查：检查了焦点归位的所有 `select_*` 调用点、
  `action_available` 的每个 Action 分类、undo 白名单与内部 push 的重复、
  manifest/`.tres` rate 的所有消费方）
