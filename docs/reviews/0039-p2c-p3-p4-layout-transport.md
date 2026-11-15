# Review 0039 — P2c/P3/P4：revision 守卫 / Overlay + Layout 缓存 / Transport

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass; see 0040 for the cross-round audit)
- scope: `src/app.rs`, `src/app/{input,commands,audio_bridge,render}.rs`, `src/ui.rs`, `src/ui/{layout,paint}.rs`, `src/audio.rs`, `src/library.rs`, `src/presets.rs`, `src/music_bank.rs`, `docs/PLAN.md`, `docs/REVIEW_CHECKLIST.md`
- diff_base: `142559d`（统一 Library 前基线；本轮承接 0035–0038）
- source_hash: 27a2304475881143c852ac626b3a0106fea75bdccf4f35d6b06367b275602380
- status: approved

## 变更摘要

对应 0034 的 P2c / P3 / P4。

### P2c — revision 不变量守卫

- `DocState.pending_edit` 已是 `PendingEdit { project, version, music_version }`；
  `commit_edit` 只在 revision 变化时压栈。
- 新增 test + debug 守卫：revision 未变但持久化工程 JSON 不同（`sample_data`
  被 `serde(skip)`）时 `debug_assert!`，捕获“忘记 mark_dirty”的路径。
- 守卫只在 `cfg(all(test, debug_assertions))` 启用，开发/发布不做序列化。

### P3 — Overlay 栈 + Layout 缓存

- 新增 `Overlay::{Menu,KindFilter,CategoryFilter}` 与 `OverlayStack`
  （互斥打开、LIFO 关闭）。删除 `menu_open` / `kind_menu_open` /
  `category_menu_open` 三个布尔值。
- 外部点击统一为“关闭栈顶并消费”；打开另一个下拉会替换栈顶。
- `SfxLogic::layout()` 返回 `Rc<ui::Layout>`；输入命中与
  `ui::paint_with_layout` 共用同一份布局。
- `LayoutKey` 覆盖 viewport、view/doc/music revision、tab、选中项、筛选、
  搜索 hash、滚动、缩放/偏移、pin、overlay、loop、资产数量、root 存在性。
- 测试用的 `ui::paint` 保留为包装（`#[cfg(test)]`）。

### P4 — Transport / 共享播放缓冲

- 新增 `PlaybackSource { Patch, Music, Preset, Audio }`；`play` / `audition`
  设置，`stop` / `seek` / 自然结束清空。
- 新增 `SfxLogic::play_focused()`：AUDIO/PRESET 焦点试听资产，否则播放当前
  tab 文档；Space 与头部 PLAY 共用。
- `AudioEngine::play_shared(Arc<Vec<f32>>, ...)`；`Playback` / `Command` /
  retired 槽改用 `Arc<Vec<f32>>`。
- `PlaybackState.cache` / `music_cache` 改为 `Arc<Vec<f32>>`；patch / preset /
  audio 播放不再复制整段缓冲。music 音量仍在播放时复制应用（见残余风险）。

## 自审发现（已修）

1. `paint_with_layout` 改造时把 `&layout.field` 批量替换成 `layout.field`，
   导致库行/菜单迭代语义变化；clippy 发现 `&layout` needless borrow，
   已恢复字段引用并只对函数实参去借用。
2. `LayoutKey` 最初依赖 `view_version` 过多；改为显式包含滚动、缩放、
   overlay、选中项等字段，降低“某处忘记 bump”导致 UI 不刷新的风险。
3. `PlaybackSource` 在设备自然结束/错误时可能残留；`update` 重置
   `was_playing` 时一并清空 source。
4. Overlay 外部点击最初会关闭 Category 后无法一次切到 Kind；补充
   Kind/Category/Menu 之间的显式切换。
5. `commit_edit` 守卫最初使用 `return`，clippy `needless_return`；改为
   `changed` + 条件块，并在 move 前执行 test-only 比较。
6. 删除三个 Bank 上已经不再读取的 `next_id` 字段（ID 每次重扫从
   `path_ids` 推导）。

## 第二审查发现

### Blocker

无。

### Major

1. **`LayoutKey` 是维护点**：新增 `Layout::new` 输入（例如新的面板/列表字段）
   必须同步加 key；测试只覆盖了 kind filter 与 scroll。已在
   `REVIEW_CHECKLIST` 增加“新增输入必须更新 key”。
2. **test-only revision 守卫**：发布路径仍依赖 `mark_dirty` 约定；完整的
   `App::edit` 字段封装留到 v5 / 后续重构。
3. **`PlaybackSource` 不是完整 Transport 状态机**：停止仍由 cache version、
   `was_playing` 和 `update` 共同驱动；source 主要提供显式来源与自然结束清理。
4. **music 播放仍复制**：为应用 `volume_db` 保留 `Vec` 复制；patch/preset/audio
   已共享。
5. **Arc 退役槽满时仍在音频线程释放**：与 0015 相同，PCM 释放可能发生在回调；
   UI `reclaim()` 正常路径会接走。

### Minor

1. `OverlayStack` 名为栈，但当前语义是“互斥单层”；LIFO 只用于 Esc 关闭。
2. `paint` 测试包装是 `#[cfg(test)]`，外部无法复用；生产统一走
   `paint_with_layout`。
3. `AssetId` 仍是 `u32` 类型别名，不是 newtype。
4. `ActionMeta` 的 label/tip/icon 仍是三个 match，通过统一 API 暴露。
5. 菜单分组/顺序仍由 `MENU_ENTRIES` 手写。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 191 passed
- [x] `cargo test --release` — 190 passed（debug-only 守卫测试排除）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `--demo`：manifest duration 与 WAV 一致；selfcheck 同上
- [x] `./scripts/source_hash.sh` — `27a23044…`

新增测试：

- `forgotten_revision_bump_is_caught_in_debug`
- `overlay_stack_is_exclusive_and_lifo`
- `layout_is_reused_until_relevant_state_changes`
- 音频测试适配 `Arc<Vec<f32>>`

## 残余风险与后续

- P5：Sound / Arrangement 分离、多 music、Wave 注册表、资产文件生命周期。
- 实时 voice 引擎（边拖边听）仍不在范围。
- `App::edit` 字段封装、命令表合并 label/shortcut/tooltip 的进一步收尾。
- 手势级测试框架仍建议在 P5 前补。

## 签署

- reviewer_1: approved
- reviewer_2: approved（详见 `0040-independent-audit.md`）
