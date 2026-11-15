# Review 0042 — 独立审查：P5 安全项与 v5 边界

- date: 2026-11-16
- author: pi (independent audit pass)
- reviewer_1: pi (implementation author)
- reviewer_2: pi (adversarial cross-round audit)
- scope: 当前工作树（0035–0041）：`src/app*`, `src/ui*`, `src/{model,audio,library,presets,music_bank,export}.rs`, `docs/{PLAN,REVIEW_CHECKLIST}.md`, `README.md`
- diff_base: `142559d`
- source_hash: e563d3b6f2798ad1730c7f382dc9886c3d2296ec958a97c8bec6b7585ceb37a9
- status: approved

## 审查范围

本轮是 P0–P4 之后的额外独立审查，重点：

1. Wave 注册表是否真的减少“加波形”的分散修改；
2. 手势测试入口是否会掩盖真实输入路径；
3. AUDIO/MUSIC rename/delete 的文件安全、ID 稳定、引用语义；
4. 是否还有 P5 架构项被遗漏；
5. 全量门禁与 Release 测试。

## 发现

### Blocker

无。

### Major

1. **Sound/Arrangement 分离未完成**。`Patch` 仍同时承载音色与 `Layer.clips`
   排列。这是 v5 的核心，必须同时修改 schema、Rust 渲染/导出、Godot 插件
   的 `SfxPatch`/`SfxLayer` 与 parity golden；本轮明确没有做。
   **建议迁移步骤**：
   - 新增 `Sound { layers, dsp_version, ... }` 与
     `Arrangement { clips: Vec<ClipInstance { sound_id, start, ... }> }`；
   - `PROJECT_VERSION` 5 + 迁移旧 `Layer.clips` 为引用某个 sound 的 clip；
   - `.tres` 导出增加 arrangement 分支，旧 `.tres` 继续可读；
   - parity fixture 增加“同一 sound 多次引用”的 golden。
2. **删除被引用的音频是“软失败”**：内存中的 SAMPLE 层仍拿着 `Arc` PCM 可播放，
   但重新打开工程会 warning；确认文本已提示引用。当前是“用户明确确认后移动
   文件”，没有阻止删除或清理引用层。
3. **Wave 注册表仍非单点**：`WAVE_DEFS` 收敛了 label/desc/family/godot id，
   但 `from_godot_id`、`wave_color`、DSP 分派仍需手改；建议下一轮做
   `WaveDef` 包含 `color` 与 `from_godot` 策略。
4. **手势测试入口只覆盖同步事件**：`handle_event` 不模拟 `update` 后的
   异步预览/音频回调，拖拽和播放相关组合仍靠状态测试。
5. **AUDIO/MUSIC 删除不可恢复 UI**：`.trash/` 只有手工恢复；PRESET 有
   RESTORE LATEST，AUDIO/MUSIC 没有。

### Minor

1. AUDIO/MUSIC 菜单段始终显示；未选中时通过 `AssetEdit` 灰显。
2. `safe_file_stem` 允许 Unicode/空格，跨平台文件系统边界未做专门测试。
3. `MusicBank::rename` 后 `MusicAsset.path` 更新，但 `Music` 内容不变；
   若用户期望“文件名 = 曲目标题”，当前没有标题字段。
4. `DeleteAudio` 引用扫描不包括外部工程/游戏侧引用。
5. Wave id 8 的 Curve→Pluck 别名已显式化，但 `.tres` 的 layer 子资源仍写
   `wave = 8`；baked 路径下消费方不读该值。
6. `OverlayStack` 仍是互斥单层，不是通用 z-order 栈。

## 交叉核对 P0–P4

| 轮次 | 核对 |
|---|---|
| P0 0035 | 焦点归位、只读 scope、筛选不载入、单次 undo、导出元数据通过 |
| P1 0036 | `LibraryMode`/`selected_preset` 已删；AssetId 跨重扫稳定 |
| P2 0037/0038 | ActionMeta/undo revision；菜单/快捷键/tooltip 同源 |
| P3 0039 | Overlay 栈、Layout 缓存、输入/绘制共享布局 |
| P4 0039 | `play_focused`、`PlaybackSource`、Arc 播放缓冲 |

无发现 P0–P4 的功能回归；selfcheck 命令数保持 3937/179，说明绘制输出稳定。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 199 passed
- [x] `cargo test --release` — 198 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`
- [ ] 真实窗口手测（本环境无窗口；新增 lifecycle 由 bank/action 测试覆盖）

## 结论

P5 安全项可以合入：无 Blocker。唯一未完成的架构级 P5 项是
**Sound / Arrangement 分离（schema v5）**；它需要跨 Rust/Godot/格式的独立
迁移，不应继续塞进本轮。建议下一轮只做 v5 迁移 + parity，不要混入 UI 功能。

## 签署

- reviewer_1: acknowledged
- reviewer_2: approved（独立审查）
