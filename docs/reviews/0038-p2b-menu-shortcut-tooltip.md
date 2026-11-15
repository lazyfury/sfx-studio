# Review 0038 — 架构 P2b：菜单 / 快捷键 / tooltip 单一来源

- date: 2026-11-16
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (adversarial pass on the current tree)
- scope: `src/ui.rs`, `src/ui/{layout,paint}.rs`, `src/app/input.rs`, `docs/PLAN.md`
- diff_base: `142559d`（统一 Library 前的基线；本轮基于 0037）
- source_hash: 905b7379da13f7fdfe042c0b42fe0535a3933cf2877419146e96ae0b4355a2ea
- status: approved

## 变更摘要

对应 0034 的 P2b：把命令的展示/快捷键元数据收敛。

1. **`ActionMeta` 扩展**：增加 `label`、`tip`、`icon`。`action_meta()` 同时
   提供 scope/undo/label/tip/icon；`action_label()` 是 label 的单一实现。
2. **菜单去重**：`MenuEntry::Item(Action)` 不再携带字符串；`MENU_ENTRIES`
   只声明分组与顺序，layout 从 `action_meta` 取 label，paint 不再自己找图标。
3. **顶栏 / 标签页按钮**：label 与 icon 从 `action_meta` 派生，删除 layout 中
   的重复硬编码文案。
4. **快捷键表**：新增 `Shortcut { key, cmd, shift: Option<bool> }`、
   `SHORTCUTS` 与 `shortcut_action()`。
   - Cmd+S/O/E/B、Cmd+Z/Shift+Cmd+Z/Cmd+Y、R/V/C/L 走表；
   - 字符匹配大小写不敏感（`Shift+R` 也能触发 Randomize）；
   - Space/Tab/Delete/方向键/搜索 `/`/1–8/F 保留显式手势逻辑。
5. **tooltip / icon**：`paint_hover_tip`、`hover_info`、菜单图标改用
   `action_meta(...).tip` / `.icon`。

## 自审发现（已修）

1. `Cmd+S/O` 原来直接调用 `save()` / `open()`，绕过 `activate`；现在统一走
   `activate`，会先 `commit_edit` 并检查可用性。行为等价，但撤销事务更一致。
2. `Cmd+Z` 现在通过 `Action::Undo`，会显示 `undo <label>` toast；之前直接
   调用 `state.undo()` 无提示。属于可接受的 UX 改进。
3. `L` 从直接翻转 `loop_play` 改为 `Action::Loop`；行为相同，但统一经过命令层。
4. 快捷键表使用 `shift: Option<bool>`：`None` 忽略 Shift，`Some` 用于
   Undo/Redo 共享 Cmd+Z。
5. `MenuEntry::Item` 的测试与 layout 高度计算同步更新；`--selfcheck` 命令数
   保持 3937 / 179 text runs，确认绘制输出未意外变化。

## 第二审查发现

### Blocker

无。

### Major

1. **快捷键表尚未覆盖全部操作**：Space/Tab/Delete/方向键/搜索/1–8/F 仍是
   显式分支，因为它们带焦点/手势/上下文语义；这是有意保留，不是遗漏。
2. **`action_label` / `action_tip` / `action_icon` 仍是三个 match**：通过
   `ActionMeta` 统一暴露，调用点不再分叉；但新增 Action 需要同时补三处。
   P2c 可合并为单一 `ActionDef` 声明。
3. **顶级菜单顺序仍手写**：`MENU_ENTRIES` 只存 Action，顺序/分组是显式策略；
   若要根据 scope 自动分组，需要额外设计，不在本轮。
4. **Inspector 小按钮文案仍本地**：例如 `DUP` / `DEL` / `LOAD WAV…` 由
   presenter 决定；它们是控件标签而非命令菜单标签，保留更灵活。
5. **`Cmd+Z` 的 toast 行为变化**：已在摘要中标注；如有自动化依赖状态行文本，
   需检查（现有测试没有）。

### Minor

1. `shortcut_action` 对字符做 `eq_ignore_ascii_case`，对非 ASCII 字符退化为
   精确匹配；当前快捷键都是 ASCII。
2. `Shortcut` 表没有冲突检测；测试至少覆盖了 Cmd+Z 的 shift 分支。
3. `ActionMeta` 每次调用会执行 label/tip/icon 三个 match；命令调用频率很低，
   不构成性能问题。
4. 菜单 label 使用 `&'static str`，`layout.menu_items` 保持
   `(Action, &'static str, Rect)`，paint 无需改变。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test` — 188 passed（187 → 188）
- [x] `cargo run -- --selfcheck` — 2 frames / 3937 commands / 179 text runs
- [x] `./scripts/preflight.sh`
- [x] `./scripts/source_hash.sh` — `905b7379…`

新增测试：

- `shortcuts_and_menu_labels_are_centralized`

## 残余风险与后续

- P2c：`App::edit` 封装字段修改，彻底消除“忘记 mark_dirty”的 undo 漏洞。
- P3：Layout/Present 缓存 + Overlay 栈。
- P4：Transport 统一 + `Arc` 播放 buffer。
- 若后续增加快捷键，只需在 `SHORTCUTS` 表加一行；但带上下文语义的键仍需
  在 `input.rs` 显式处理。

## 签署

- reviewer_1: approved
- reviewer_2: approved（对抗性检查：菜单 label/icon/tooltip 来源、
  Shift+Cmd+Z、大小写字符、Cmd+S/O 事务、L 命令化、selfcheck 输出稳定）
