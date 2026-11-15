# Review 0040 — 独立审查：P0–P4 架构收尾

- date: 2026-11-16
- author: pi (independent audit pass)
- reviewer_1: pi (implementation author)
- reviewer_2: pi (adversarial cross-round audit)
- scope: 当前工作树（0035–0039）：`src/app*`, `src/ui*`, `src/{audio,library,presets,music_bank,export}.rs`, `docs/{PLAN,REVIEW_CHECKLIST}.md`
- diff_base: `142559d`（统一 Library 前基线）
- source_hash: 27a2304475881143c852ac626b3a0106fea75bdccf4f35d6b06367b275602380
- status: approved

## 审查范围与方法

本轮是 0034 之后的额外独立审查，覆盖 P0–P4 的全部改动，不只看最后一个 diff：

1. 焦点/选择/只读边界是否仍有第二真相源；
2. 命令元数据、undo、revision 事务是否自洽；
3. `AssetId` 在重扫/重命名/删除/恢复后的稳定性；
4. Overlay 栈的点击优先级与关闭语义；
5. `LayoutKey` 是否覆盖所有布局输入；
6. `Transport` / `Arc` 播放缓冲的 UI↔回调一致性；
7. 导出元数据与文档/测试是否一致。

验证基线：

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                 # 191
cargo test --release       # 190（debug-only 守卫排除）
cargo run -- --selfcheck   # 2 frames / 3937 commands / 179 text runs
./scripts/preflight.sh
./scripts/review_gate.sh
```

## 跨轮核对

| 轮次 | 目标 | 核对结果 |
|---|---|---|
| 0035 | P0 焦点/作用域/导出 | 通过；`focus_document` 覆盖文档点击，`ActionMeta` 只读边界生效 |
| 0036 | P1 单一焦点/稳定 ID | 通过；无 `LibraryMode`/`selected_preset`；重扫选择测试覆盖 |
| 0037 | P2a 命令元数据/revision undo | 通过；`ActionMeta` 穷尽，undo 单快照 |
| 0038 | P2b 菜单/快捷键/tooltip | 通过；菜单 label、快捷键表、tooltip 同源 |
| 0039 | P2c/P3/P4 | 通过；Layout 共享、Overlay 栈、Arc 播放缓冲、source 清理 |

## 发现

### Blocker

无。

### Major

1. **`LayoutKey` 是新的维护点**：布局缓存避免了每帧重建，但新增
   `Layout::new` 输入时必须同步扩展 `LayoutKey`。当前测试只覆盖
   kind filter 与 scroll；建议在 code review 清单中作为强制项（已加入）。
2. **revision 守卫只在 test+debug 生效**：发布构建仍依赖 `mark_dirty` 约定；
   完整字段级 `App::edit` 仍未实现。作为风险记录，不构成当前 Blocker。
3. **`PlaybackSource` 不是完整状态机**：它记录来源并在 stop/seek/自然结束时
   清空，但“何时停止”仍由 cache version、`was_playing`、`update` 共同决定。
   这是从“整段渲染播放”过渡到 Transport 的中间态。
4. **music 播放仍复制缓冲**：`volume_db` 在播放时应用，因此 music 仍
   `Vec` 复制；patch/preset/audio 已 `Arc` 共享。把 volume 纳入渲染缓存可
   后续消除。
5. **音频线程可能释放退役 PCM**：`retired` 槽满时 Arc 在回调 drop，和 0015
   的残余风险相同；正常路径 UI `reclaim()` 会接走。

### Minor

1. `OverlayStack` 是“互斥单层 + LIFO 关闭”，不是通用 z-order 栈；满足当前
   三个弹出层。
2. 外部点击下拉现在统一关闭并消费；这是有意的行为变化，已在 0039 记录。
3. `paint` 测试包装是 `#[cfg(test)]`；生产只暴露 `paint_with_layout`。
4. `AssetId` 是 `u32` 别名而非 newtype；variant 已提供类型区分。
5. `ActionMeta` 的 label/tip/icon 仍是三个 match；调用点统一。
6. 菜单分组/顺序仍手写；快捷键表只覆盖 1:1 命令，上下文键保留在 `input.rs`。
7. `commit_edit` 的 debug 守卫只序列化 test 构建；开发 app 不承担开销。

## 已修复的审查项

在 0039 实现过程中发现并修复：

- `paint_with_layout` 批量替换导致 `layout.field` 迭代语义错误 / clippy
  needless borrow。
- `PlaybackSource` 在自然结束时残留。
- Category→Kind 下拉一次切换失效。
- `commit_edit` 守卫的 `needless_return` 与 move 后借用。
- 三个 Bank 的死 `next_id` 字段。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 191 passed
- [x] `cargo test --release` — 190 passed
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `--demo` 临时工程：manifest duration == WAV 时长（48 kHz）
- [x] `./scripts/review_gate.sh` — approved
- [ ] 真实窗口手测（本环境无窗口；由状态/布局/绘制测试覆盖）

## 残余风险与后续

- **P5 模型**：Sound 与 Arrangement 分离、多 music、引用式 Clip、Wave 注册表。
- **实时 voice 引擎**：当前仍是“整段渲染 + 播放”，不是 DAW 级实时混音。
- **手势级测试框架**：组合键/焦点/拖拽的回归仍主要靠状态测试。
- **资产生命周期**：AUDIO/MUSIC 的 rename/delete/import 仍不完整。
- **i18n**：中文副标题/提示仍内联在代码。

## 结论

P0–P4 可以合入：无 Blocker；5 个 Major 均为“下一步架构边界”或已记录的性能
折衷，不阻塞当前功能。建议下一轮先补手势级测试，再进入 v5 模型。

## 签署

- reviewer_1: acknowledged
- reviewer_2: approved（独立审查）
