# Review 0045 — 性能：统一库查询去分配 / 渲染缓冲复用

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review)
- reviewer_2: pi (independent adversarial pass, same tool / separate run — no human reviewer available in this environment)
- scope: `src/app.rs`（LibrarySource / 计数 / 单行查询）、`src/ui/layout.rs`、`src/ui/paint.rs`、`src/app/render.rs`、`src/synth.rs`、`src/ui.rs` 布局测试
- diff_base: `142559d`
- source_hash: 3455dcc83bfec8978e4078a513e2ccf5ca1e2f9789df2c567c631a069d9e70f6
- status: approved

## 变更摘要

1. **每帧整库物化**：`paint` 之前每帧调用 `visible_library_entries()` 2–3 次
   （计数徽标、空态、选中资产、hover 提示），每次为每行分配 `name` /
   `detail` / `folder` 字符串。新增借用视图 `LibrarySource`（`name`、
   `folder` 借用，`detail` 为 `Wave / Bytes / Music` 轻量枚举），过滤/计数
   只扫描引用，不再分配字符串。
2. **单行查询**：`State::library_entry(key)` 只在命中时物化一行，hover /
   drag ghost / 状态栏不再构建全库。
3. **计数随布局缓存**：`Layout` 新增 `library_visible` / `library_total` /
   `preset_visible`；`LayoutKey` 已覆盖搜索、筛选、收藏、折叠、viewport、
   文档 revision 与各资产长度，因此缓存不会滞后。`paint` 直接读字段，
   空闲帧的库扫描降为 0。
4. **搜索串**：`search_lower()` 每次查询只做一次 lowercase；
   `visible_patches` / `visible_presets` / `visible_library_entries` /
   `visible_library_count` 共用。旧实现 `filters_match` 对每行都重新
   lowercase 搜索串。
5. **渲染 worker**：`downmix()` 与左右声道 `collect::<Vec<_>>()` 删除；
   `synth::envelope_interleaved` 在遍历中按声道平均，`peak_interleaved`
   直接扫交错缓冲。渲染 30 s 立体声时少分配约 2 份 mono 长度缓冲。
6. **状态栏**：只有 AUDIO/MUSIC 选中时才查资产行；SOUND/PRESET/文档选中
   不再触发库查询。

行为保持：`visible_library_count()` 有回归测试与
`visible_library_entries().len()` 对拍；`library_entry` 对拍物化列表的
`name/detail/kind/folder`；`envelope_interleaved` 与旧 `downmix + envelope`
对拍；`peak_interleaved` 与左右声道 peak 语义一致（max 绝对值）。

## 自审发现

- `LibraryEntry.category` 在改用借用视图后不再被读取；已删除字段，
  `LibrarySource` 内部仍保留 `category` 供筛选使用，`LibraryEntry` 的
  `#[derive(Clone, Debug)]` 与构造点同步更新。
- `preset_visible` / `all_library_entries` 失去调用点：`visible_presets`
  保留为 `#[cfg(test)]` 测试辅助；`all_library_entries` 删除（无生产调用）。
- 测试 `unified_library_lists_all_four_kinds_and_filters_them` 原先断言
  `entry.category`；改为按 `LibraryKey` 回查 SOUND/PRESET 的 category，
  验证语义不变。
- `Layout::new` 多算一次 `visible_library_count`（布局重建时），相对
  `visible_library_rows` 的字符串物化可忽略。

## 第二审查发现

### Blocker

无。

### Major

1. `Layout` 缓存计数依赖 `LayoutKey` 完整覆盖所有影响计数的输入。本次核对：
   `kind_filter` / `category_filter` / `favorites_only` / `search_len` /
   `search_hash` / `presets_len` / `audio_len` / `music_assets_len` /
   `folded_len` / `doc` / `view` 均在其中；收藏切换与预设库变更会 bump
   `view_version` 或 `doc.version`。若后续新增“不 bump revision 的库变更”，
   必须同步加到 `LayoutKey`（已记录风险）。
2. `LibrarySource` 与旧 `all_library_entries` 是两套行构造逻辑。当前由
   `all_library_entries` 删除、`LibrarySource::to_entry` 单点生成来避免漂移；
   新增字段时只需改 `LibrarySource`。审查中已核对 SOUND/AUDIO/PRESET/MUSIC
   四类的 detail/folder/favorite 与旧实现一致。
3. `visible_preset_count` 与 `visible_presets` 仍是两份过滤循环；语义由
   共享的 `filters_match_with` 保证，但新增筛选条件时两处都要更新过滤器
   （过滤器本身单点，循环只是调用点）。可接受。

### Minor

1. `library_entry` 仍会构建整个 `LibrarySource` 向量（借引用、无字符串），
   大库下约 1.5 µs/次；若以后出现每帧多次 hover 查询，可再按 kind 分派。
2. `Layout::new` 每次重建仍会重建库行；参数拖动期间 `doc.version` 变化会
   触发。这是既有行为，本次未拆分“库行缓存 / 检查器行缓存”，列入后续。
3. `audio_bridge::play` 的 MUSIC 分支仍复制整段缓冲做音量；只在按下播放
   时发生，未在本轮处理。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 204 passed
- [x] `cargo test --release` — 203 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] 性能对拍（release，512 SOUND + demo 预设，2000 次）：
  `visible_library_entries` 30.9 µs/次 → `visible_library_count` 2.3 µs/次 →
  `library_entry` 1.5 µs/次
- [ ] 真实窗口手测（本环境无窗口；布局/文本命令数由 selfcheck 守卫）

## 残余风险与后续

- `Layout` 缓存计数与库行仍随 `doc.version` 重建。下一步可将布局拆成
  `LibraryLayout`（只依赖库相关 revision）与 `InspectorLayout`（依赖文档
  revision），拖动参数时只重建检查器行。
- 若库规模继续增长，可把 `library_sources()` 改为迭代器，消除最后一个
  小向量分配；当前 1.5–2.3 µs/次不足以影响 60 fps。
- 计数缓存不影响任何导出/音频路径；DSP 与 `docs/PARITY.md` 未变化。

## 签署

- reviewer_1: acknowledged
- reviewer_2: approved（独立复核：逐项对照旧实现语义、运行门禁与性能对拍）
