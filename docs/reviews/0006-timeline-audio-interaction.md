# Review 0006 — 时间线比例冻结、收藏星标、无锁播放头、自适应高度

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app.rs`, `src/ui.rs`, `src/audio.rs`, `src/icons.rs`, `README.md`, `docs/PLAN.md`
- diff_base: `e54c92d73c3aa360f3f4f046e922bda7bce223d9`
- source_hash: 8d1c0e56d152d7faaf980d5df7783afc8e549a02e5e672ec71bba5b97f0d1063
- status: approved

## 变更摘要

- **拖拽比例冻结**：`State::timeline_pin` 在手势开始时保存 `timeline_span()`，`Layout` 与 `Drag::Clip` 换算都使用冻结比例；松开指针后清除并重新 fit。`Layout::timeline_span()` 成为唯一的可见时长来源。
- **共享吸附**：新增 `ui::CLIP_SNAP`（5ms）与 `ui::snap_time()`；Clip Move/Resize/Trim 与自动化点共用；拖拽状态栏显示 `SNAP 5ms`。
- **收藏星标可点击**：新增 `Hit::PatchFavorite(usize)`、`ui::patch_star_rect()`；点击只切换对应 patch 的 favorite，不改变选中；hover 高亮为 `ACCENT`；走 `begin_edit/commit_edit` 撤销事务。
- **播放头无锁**：音频回调每 buffer 将帧位置写入 `Arc<AtomicU64>`，`AudioEngine::position()` 只读原子；`Mutex<Playback>` 仅用于换 buffer，UI 每帧不再抢音频锁。
- **自适应布局**：时间线高度按图层数伸缩（3 行下限，音乐页按 pattern 行数），剩余高度给波形；时间线增加与标尺对齐的淡竖网格；时间线内容裁剪在卡片内，拖出边界不会画到相邻卡片。
- **图标表健壮性**：`document()` 改用 `get()`，新增 `icon_indices_match_the_all_table` 测试，防止漏改 `ALL` / `index()` 时静默画错或越界。
- 测试从 48 增至 53：星标命中、pin 比例稳定、自适应高度（含音乐页）、星标点击撤销。

## 自审发现（已修）

1. 自适应高度最初把音乐页 pattern 网格压到 92px（图层数决定高度）→ 音乐页改为按 `pattern_rows` 行数计算。
2. Clip 拖出右边界时会被绘制到自动化/波形卡片上 → `paint_timeline` 增加 `ctx.clip_rect(rect)`。
3. `timeline_pin` 若因焦点丢失没有 PointerUp 会残留 → `update()` 在 `Drag::None` 时兜底清除。
4. 新增测试 `timeline_grows...` 起初只覆盖 Patches 页 → 补 Music 页断言。

## 第二审查发现

### Blocker

无。

### Major

1. **松手后会重新 fit**：拖动过程中比例冻结，指针松开后按新的 `patch.end()` 重算，会看到一次跳变。这是本轮的约定行为（避免拖动中漂移），真正的固定缩放/滚动需要后续的 view span + zoom。
2. **收藏星标命中区只有 20px 宽**：覆盖整行高度，鼠标可用；触控/高 DPI 下偏小，后续可扩到 24–28px 或整块右侧留白。
3. **播放头每 buffer 更新一次**：位置最多滞后一个音频 buffer（约 5–10ms），比之前锁竞争的抖动更可预测；逐帧播放头精度需求不高，可接受。
4. **波形高度无上限**：图层很少、窗口很高时波形会占据大部分中心区。当前视为「波形是视觉锚点」的设计选择；若后续觉得过大，可把余量分给自动化卡。

### Minor

- 音乐页滚轮仍走 `layer_scroll` 分支（不可见）；应在 Music tab 忽略或改成分页。
- 自动化卡仍固定 62px，未参与自适应分配。
- `timeline_pin` 是 State 上的临时字段，长期更适合放进 `Drag::Clip`；当前兜底清理已覆盖丢失 PointerUp 的情况。
- 拖拽吸附固定 5ms，暂不支持 Shift 关闭；已记入 Backlog #11。
- 合成鼠标点击在无焦点窗口下会被 macOS 吞掉，收藏星标的**端到端手测**只确认了 hover/hit；切换动作由 `star_click_toggles_favorite_without_changing_selection` + `favorite_star_hit_is_split_from_the_row` 覆盖。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（53 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2550 commands / 118 text runs）
- [x] 实机截图：时间线 3 行、波形占满剩余空间、竖网格与标尺对齐
- [x] `favorite_star_hit_is_split_from_the_row`
- [x] `pinned_timeline_span_keeps_the_scale_stable`
- [x] `timeline_grows_with_layers_and_gives_space_to_the_waveform`
- [x] `star_click_toggles_favorite_without_changing_selection`
- [x] `icon_indices_match_the_all_table`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 时间线缩放/滚动仍是后续工作；当前只保证拖动过程稳定、松手重新 fit。
- 波形高度在超大窗口上可能过大，需根据实际使用反馈再定上限。
- 音频无锁位置只覆盖播放头读取；`play()` / `stop()` 仍会短暂持锁换 buffer。
- 图标表仍是四张手写表（enum/ALL/svg/index），一致性测试只能防止当前组合漂移。

## 签署

- reviewer_1: approved
- reviewer_2: approved
