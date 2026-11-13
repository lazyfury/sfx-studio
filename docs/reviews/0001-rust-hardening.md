# Review 0001 — Rust 端 P0/P1 加固与强制审查流程

- date: 2026-11-13
- author: pi (implementation)
- reviewer_1: pi (self-review pass, checklist §1–§9)
- reviewer_2: pi (independent adversarial pass, separate review context)
- scope: `src/audio.rs`, `src/app.rs`, `src/model.rs`, `src/synth.rs`, `src/tres.rs`, `src/export.rs`, `src/ui.rs`, `src/theme.rs`, `README.md`, `docs/`, `scripts/`, `.githooks/`, `.github/workflows/ci.yml`
- diff_base: `154853e3ce3dba6210cc53d3834341a13c426900`
- source_hash: f9b7f77642422f2bb629747c905970d27616af99364edb94783f6d3dfe48edfd
- status: approved

## 变更摘要

按 `docs/PLAN.md` 执行 P0 + P1 全部条目，并落地强制二次审查流程。

- **播放正确性**：循环回绕不再重复首样本；seek 使用 `start_frame`，seek 时停流；5 ms fade-in/out；`is_playing` 原子化；空 buffer 防御。
- **数据安全**：加载错误不再静默回退 demo；保存改为 temp + rename + `.bak`；`Project.version = 2` + `migrate/validate`；所有数值 clamp、去重名、数组上限、DSP buffer 上限。
- **导出**：唯一文件名（`coin`/`coin_2`/`sound`/`sound_2`）；`.tres` 写入 `resource_name`、`group`、`tone_env`、`exp_decay`；manifest v2 含 category/favorite/dsp_version/schema_version；WAV/music/`.import` 全部原子写。
- **缓存与性能**：拆分 `version` / `music_version` / `view_version`；按 tab 渲染；80 ms 节流；peak 与 2048-bucket 包络缓存；拖动不再每帧重渲染整段音频。
- **撤销**：`begin_edit/commit_edit` 手势事务，空操作不入栈；`(label, Project)` 栈；Vary 不再双快照；新编辑清 redo；打开工程清历史。
- **UI**：中文显示名保留；1024 px 紧凑顶栏；布局容量单源；Cutoff 提示修正；Kick 隐藏无效 SEED；ToneEnv 选项 + EXP DECAY 参数。
- **DSP**：补齐 Godot `tone_env/exp_decay`；空 notes/chords 与 Godot 一致返回空流。
- **流程**：`preflight.sh`、`source_hash.sh`、`review_gate.sh`、git hooks、CI workflow、开发流程/清单/格式/一致性文档。

## 自审发现（已修）

1. `Vary` 在 `activate` 快照列表和函数内部各推一次 undo → 删除内部快照。
2. 拆分 revision 后 `select_patch` 不再自动使 patch 缓存失效 → 显式 `cache_version = 0`。
3. `mark_dirty` 清 redo 会破坏 undo/redo → `invalidate_document` 单独实现，不复用 `mark_dirty`。
4. `save` 中 validate 可能 clamp 数据但缓存仍旧 → 保存后置 `cache_version/music_cache_version = 0`。
5. `MusicField` 拖动仍走 patch 的 `mark_dirty` → 改 `mark_music_dirty`。
6. `Seed` 对 Kick 无效却显示 → Kick 隐藏。
7. `_keep` 死代码与未用 import → 删除。
8. clippy `items_after_test_module`、`too_many_arguments`、`ptr_arg` 等 → 清零（`-D warnings`）。

## 第二审查发现

### Blocker（均已修复）

1. **中文名带数字被 sanitize 成纯数字**：`sanitize("金币 2")` 原结果为 `"2"`，与「空名→sound」策略冲突，导出可能直接以 `2.wav` 命名。改为：结果不含 ASCII 字母则回退 `sound`；新增测试 `sanitize("金币 2") == "sound"`。
2. **music.wav 缺少 `.import` sidecar**：bake 只为每个 patch 写 sidecar，`music.wav` 在 Godot 4.7 下仍可能走 QOA。已补 `music.wav.import`，端到端测试断言其存在。
3. **`Project::default()` 的 `version` 为 0**：`#[derive(Default)]` 绕过 serde 默认函数，空工程保存后会写成 v0。改为手写 `Default`，`version = PROJECT_VERSION`。
4. **同名 patch 的 `.tres` 往返歧义**：两个都叫 `coin` 的 patch 会写出 `coin`/`coin_2` 文件，但 `resource_name` 都是 `coin`，重新导入后无法区分。处理：应用加载走 `validate()` 自动重命名为 `coin`/`coin_2`；导出测试改用不同的显示名验证元数据，唯一名由独立测试覆盖。

### Major（已修复）

1. **`.tres` 名称含控制字符**会破坏资源文件。`validate` 现在把控制字符替换为空格并 trim。
2. **music-only 工程无法导入**：`import_project` 在无 patch 时返回 `None`，会丢掉只有 `music.tres` 的工程。现在检测到 music 即返回 `Some`。
3. **超长名称/文件名**：JSON 中任意长 name 可能超出文件系统限制。`validate` 截断到 64 字符，`sanitize` 截断到 48 字符。
4. **自动化点落在 clip 末尾**：先 sort 后 `min(duration)` 可能产生重复点。改为在 `sanitize_points` 中先 clamp 时间再排序/去重。
5. **空 buffer 播放**：`AudioEngine::play` 对空 `samples` 会保持 `playing=true`。已加空 buffer → `stop()`。

### Minor / 接受

- 撤销标签目前统一为 `edit`，后续可按动作细化。
- `Favorite` 仍走 `mark_dirty`，会触发一次无效音频重渲染（非正确性问题）。
- `DeletePatch` 在只剩一个 patch 时仍会入一条空快照。
- Godot `.tres` 的 `favorite` 无属性可写，仅保留在 JSON/manifest。
- Windows 下 `std::fs::rename` 覆盖已存在文件的行为与 Unix 不同；跨平台发布前需改为 ReplaceFile/先删后改名。
- 退出时没有 dirty guard：igui 当前不向 `AppLogic` 派发窗口关闭事件，列入 Backlog。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`（0 warning）
- [x] `cargo test`（43 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 3010 commands / 148 text runs）
- [x] `./scripts/preflight.sh`
- [x] 导出端到端：bake + export_tres + `import_project`，断言唯一名/元数据/sidecar
- [x] 音频回绕/停止回归测试
- [x] 布局 1024 px 不越界测试
- [x] 保存/加载/坏 JSON/迁移/校验/撤销事务测试
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险与后续

| 风险 | 影响 | 计划 |
|---|---|---|
| Rust xorshift vs Godot PCG32；f32 vs f64 | 噪声类波形同 seed 不同声 | Phase 3 `docs/PARITY.md` D1/D2 |
| 噪声低通系数与采样率无关 | 22.05k/48k 音色不同 | Phase 3 双端同步改 + `dsp_version` |
| 无后台渲染线程 | 极长音乐拖动仍可能掉帧（已节流到 80 ms） | Phase 2 |
| `app.rs`/`ui.rs` 仍偏大 | 维护成本 | Phase 2 模块拆分 |
| `Wave.SAMPLE` 未支持 | 采样层往返丢失 | Phase 3 |
| 无退出 dirty guard | 关闭窗口可能丢未保存修改 | 待 igui 提供 close 事件或平台插件扩展 |

## 签署

- reviewer_1: approved
- reviewer_2: approved
- 结论：**P0/P1 与流程门禁可合入；Phase 2/3 按 `docs/PLAN.md` Backlog 推进。**
