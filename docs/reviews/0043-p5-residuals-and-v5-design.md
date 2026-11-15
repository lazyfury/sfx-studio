# Review 0043 — P5 残留项：Wave 导入映射 / trash 恢复 / 手势覆盖 / v5 设计

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass; see 0044)
- scope: `src/model.rs`, `src/library.rs`, `src/music_bank.rs`, `src/app/{input,commands}.rs`, `src/ui.rs`, `docs/design/ARRANGEMENT_V5.md`, `docs/PLAN.md`, `README.md`
- diff_base: `142559d`（统一 Library 前基线；承接 0035–0041）
- source_hash: 2a8659f5c7577ab0712d561f174a662768550efd5af0883a94618244c58108a2
- status: approved

## 变更摘要

1. **Wave 导入映射**：`WaveDef.godot_import` 显式表达 Godot 9 → Pluck 的导入语义；
   `Wave::from_godot_id` 从 `WAVE_DEFS` 派生，不再手写 match。
2. **AUDIO/MUSIC 恢复**：
   - `AudioBank::restore_latest` / `MusicBank::restore_latest` 递归扫描 `.trash`，
     取最新 `.bak`，还原原相对路径，冲突加 `_2`。
   - 新 Action `RestoreAudio` / `RestoreMusic`，菜单 AUDIO/MUSIC 段。
3. **手势覆盖**：
   - Cmd+wheel 时间线缩放；
   - clip 拖动 + 单次 undo；
   - 使用 `handle_event` 直接驱动真实输入路径。
4. **v5 设计**：新增 `docs/design/ARRANGEMENT_V5.md`，给出 Sound/SoundLayer/
   ArrangementClip schema、逐层迁移原因、7 步迁移、Godot/parity 验收。
   **本轮不实现 v5 代码迁移**。

## 自审发现（已修）

1. `WaveDef` 拆表后遗漏 `godot_import` 字段初始化；补全并用 registry 测试覆盖。
2. `from_godot_id` 改为查表后，Godot id 9 需要 `godot_import = Pluck`，否则会
   错误返回 Sample；已显式化。
3. `collect_backup_files` 与 `unique_restore_target` 最初只被
   `AudioBank::restore_latest` 使用，clippy dead_code 在接入菜单前报错；接入
   RestoreAudio/music 后消除。
4. 恢复目标冲突时使用 `_2` 后缀，避免覆盖用户刚放回的文件。
5. 删除/恢复的选中项：恢复后按文件名重新查找并选中；删除后清空 selection。

## 第二审查发现

### Blocker

无。

### Major

1. **v5 未实现**：本轮只提交设计。Sound/Arrangement 代码迁移需要同步
   Godot 插件、`.tres`、parity；不在本轮混入，避免破坏跨端。
2. **删除被引用音频仍是“软失败”**：SAMPLE 层保留内存 Arc，重新打开工程会
   warning；确认文本已提示引用，但没有阻止删除或清理引用层。
3. **Wave 仍未完全单点**：`wave_color`、DSP 分派仍需手改；`from_godot_id`
   已收敛。
4. **手势测试仍不覆盖所有拖拽**：自动化点、库滚动条、Shift 平移还没进测试。
5. **restore 只恢复最新一个**：没有 `.trash` 列表 UI；音频/音乐不能选择恢复哪一个。

### Minor

1. `restore_latest` 依赖 `.bak` 文件名格式 `<name>.<stamp>.bak`；手工改名的备份
   会被跳过。
2. AUDIO/MUSIC 菜单段始终显示；未选中时灰显。
3. `RESTORE AUDIO/MUSIC` 只要求工程目录存在，实际无备份时 toast。
4. `MusicBank::restore_latest` 返回 file stem，恢复后按名称选中；同名冲突会加
   `_2`，选中可能落到 `_2`（可接受）。
5. v5 设计中的 `ArrangementClip.layer_id` 是兼容旧逐层 clips 的过渡字段，
   不是最终形态。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 201 passed
- [x] `cargo test --release` — 200 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `2a8659f5…`

新增测试：

- registry 导入映射断言（`Wave::from_godot_id` 0..13）
- `audio_rename_and_delete_keep_ids_and_use_trash`（含 restore）
- `music_rename_and_delete_keep_ids_and_use_trash`（含 restore）
- `wheel_zoom_updates_the_timeline_scale`
- `dragging_a_clip_moves_it_and_commits_one_undo`

## 残余风险与后续

- **v5 独立迁移分支**（设计已就绪）。
- `.trash` 列表/选择性恢复。
- 删除音频的引用清理或阻止策略。
- 实时 voice 引擎。

## 签署

- reviewer_1: approved
- reviewer_2: approved（详见 0044）
