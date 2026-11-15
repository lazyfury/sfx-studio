# Review 0044 — 独立审查：P5 收尾与 v5 设计边界

- date: 2026-11-16
- author: pi (independent audit pass)
- reviewer_1: pi (implementation author)
- reviewer_2: pi (adversarial cross-round audit)
- scope: 当前工作树（0035–0043）：`src/app*`, `src/ui*`, `src/{model,audio,library,presets,music_bank,export}.rs`, `docs/design/ARRANGEMENT_V5.md`, `docs/{PLAN,REVIEW_CHECKLIST}.md`
- diff_base: `142559d`
- source_hash: 2a8659f5c7577ab0712d561f174a662768550efd5af0883a94618244c58108a2
- status: approved

## 审查范围

本轮额外独立审查覆盖 P0–P5 全部改动，重点：

1. Wave 注册表 / Godot id 导入导出是否自洽；
2. AUDIO/MUSIC rename/delete/restore 的文件安全与 ID；
3. 手势测试是否走真实 `handle_event` 路径；
4. v5 设计是否足以指导独立迁移；
5. 全量门禁、Release 测试、selfcheck。

## 发现

### Blocker

无。

### Major

1. **v5 仍是设计，不是实现**。`Patch` 继续承载 `Layer.clips`；本轮没有改
   schema/Godot/parity。设计文档已给出可执行步骤，但必须单独迁移分支。
   这不是代码缺陷，而是明确的范围边界。
2. **删除音频不清理 SAMPLE 引用**：确认文本提示引用，但内存 Arc 仍可播放，
   重新打开工程会 warning。后续应选择“阻止删除 / 同时清空引用 / 标记失效”
   三种策略之一。
3. **Wave 元数据仍未完全单点**：`WAVE_DEFS` 覆盖 label/desc/family/godot id/
   import/alias，但 `wave_color` 与 DSP 分派仍需手改；新增波形仍要改 3–4 处。
4. **恢复只恢复最新备份**：`.trash` 无列表/选择 UI；音频/音乐不能像预设那样
   浏览多个备份。
5. **手势测试覆盖仍有限**：新增 clip 拖动/缩放，但自动化点拖动、库滚动条、
   Shift 平移、拖入预设等仍未覆盖。

### Minor

1. `restore_latest` 依赖 `<name>.<stamp>.bak` 命名；手工改名备份跳过。
2. `safe_file_stem` 保留 Unicode/空格；Windows 保留名（CON 等）未专门处理。
3. AUDIO/MUSIC 菜单段常驻，未选中灰显。
4. `MusicBank::restore_latest` 用文件 stem 作为显示名，恢复后同名冲突 `_2`
   可能不是用户预期选中项。
5. `arrangement` 设计中的 `layer_id` 是兼容字段；最终 sound group clip
   仍需后续设计。

## 交叉核对

| 轮次 | 核对 |
|---|---|
| P0 0035 | 焦点/只读/筛选/undo/导出元数据通过 |
| P1 0036 | 单一焦点 + AssetId 跨重扫稳定 |
| P2 0037/0038 | ActionMeta、revision undo、菜单/快捷键/tooltip 同源 |
| P3 0039 | Overlay 栈、Layout 缓存、手势入口 |
| P4 0039 | play_focused、PlaybackSource、Arc 缓冲 |
| P5 0041/0043 | Wave 注册表、AUDIO/MUSIC 生命周期、restore、v5 设计 |

无 P0–P4 功能回归；selfcheck 维持 3937 commands / 179 text runs。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 201 passed
- [x] `cargo test --release` — 200 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`
- [ ] 真实窗口手测（本环境无窗口）

## 结论

P5 安全项与设计可以合入：无 Blocker。**唯一未完成的架构项是 v5 代码迁移**，
且它必须作为一个只包含 schema/Godot/parity 的独立 PR，不能继续混入 UI 功能。
下一步建议：开 `arrangement-v5` 分支，按设计文档 7 步执行。

## 签署

- reviewer_1: acknowledged
- reviewer_2: approved（独立审查）
