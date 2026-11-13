# 强制开发流程（含二次审查）

本流程适用于所有 Rust 端、Godot 插件与导出格式改动。**没有通过二次审查的提交不得合入、不得推送共享分支。**

## 角色

| 角色 | 职责 |
|---|---|
| 作者 | 写计划、实现、自测、写自审记录 |
| 第二审查者 | 独立于作者的一次完整审查（人或其他 AI 代理均可），必须实际阅读 diff、运行门禁、提出 blocker/major 问题 |
| 门禁脚本 | 机械检查：格式、clippy、测试、自检、审查记录与代码哈希匹配 |

作者不能给自己签 `approved`。若团队只有一人，第二审查必须由**另一个独立会话/代理**完成，并在记录中注明工具与会话标识。

## 六步流程

### 1. 计划

- 在 `docs/PLAN.md` 增加或更新条目（P0/P1/P2、验收命令）。
- 涉及格式/导出/一致性时，同步更新 `docs/FORMAT.md` 或 `docs/PARITY.md`。

### 2. 实现

- 小步提交；每个提交保持可编译。
- 不修无关代码；发现旁支问题记入 PLAN Backlog。

### 3. 自审（作者）

```bash
./scripts/preflight.sh
git diff --stat
git diff            # 逐文件读一遍
```

对照 `docs/REVIEW_CHECKLIST.md` 自查，把「改了什么 / 为什么 / 风险 / 验证」写进 PR 描述。

### 4. 二次审查（必须）

第二审查者执行：

```bash
git fetch origin
git diff <base>...HEAD
./scripts/preflight.sh
```

并按 `docs/REVIEW_CHECKLIST.md` 给出：

- **Blocker**：必须修，否则不可合入（数据丢失、崩溃、RT 安全、错误导出、测试造假）。
- **Major**：应在本 PR 修，或给出明确后续 issue 与时间。
- **Minor**：可后续处理。

审查结论写入 `docs/reviews/NNNN-<slug>.md`（模板见下），只有 `status: approved` 才进入下一步。

### 5. 门禁

```bash
./scripts/review_gate.sh    # source_hash 必须与 approved 记录匹配
```

- 本地：`git push` 会触发 `.githooks/pre-push`（preflight + review_gate）。
- 共享 CI：在任意 runner 上执行 `./scripts/ci.sh`（需同级 `../igui` checkout）；本仓库不使用 GitHub Actions。
- 安装 hook：`./scripts/install_hooks.sh`。

### 6. 合入

- 合并后更新 `docs/PLAN.md` 状态。
- 若实现与计划不符，先改计划再合入，禁止「代码先行、文档后补」的静默偏差。

## 审查记录模板

文件路径：`docs/reviews/NNNN-<slug>.md`

```markdown
# Review NNNN — <title>

- date: 2026-02-01
- author: <name/agent>
- reviewer_1: <self-review>
- reviewer_2: <independent reviewer>
- scope: <files / areas>
- diff_base: <commit>
- source_hash: <scripts/source_hash.sh 输出>
- status: approved            # draft | changes_requested | approved

## 变更摘要

## 自审发现

## 第二审查发现

### Blocker
### Major
### Minor

## 验证记录

- [ ] cargo fmt --check
- [ ] cargo clippy --all-targets -- -D warnings
- [ ] cargo test
- [ ] cargo run -- --selfcheck
- [ ] 导出/格式回归
- [ ] 手测项（列出）

## 残余风险与后续

## 签署

- reviewer_2: approved / changes_requested
```

## 门禁原理

`scripts/source_hash.sh` 对以下内容做 SHA-256：

- `src/**/*.rs`
- `scripts/**/*.sh`、`.githooks/*`
- `Cargo.toml`、`Cargo.lock`
- `plugins/**/*.gd`

`docs/reviews/` 不在哈希内，因此可以先写审查记录再提交。代码任何变化都会改变哈希，旧审查记录自动失效，必须重新审查。这保证「审查的是当前代码」，而不是历史快照。

## 紧急修复

- 允许先修线上问题，但必须在 24 小时内补做二次审查并提交记录；未补记录前不得继续推送。
- 禁止用 `git push --no-verify` 绕过门禁；如因基础设施故障必须绕过，需在 PR 中说明原因、附手工验证输出，并在恢复后立即重跑门禁。

## 禁止事项

- 作者自审代替第二审查。
- 修改测试以通过门禁（除非测试本身错误，并在记录中说明）。
- 关闭 clippy 规则而不写理由。
- 在 `docs/reviews/` 中伪造 `source_hash`。
