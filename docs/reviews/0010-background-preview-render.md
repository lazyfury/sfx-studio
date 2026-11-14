# Review 0010 — 后台预览渲染

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/app/render.rs`, `src/app/{input,commands,audio_bridge}.rs`, `src/app.rs`, `src/cli.rs`, `src/synth.rs`, `docs/PLAN.md`
- diff_base: `8f98fd4c8e2bc4ec0f6c41e9f3fa3db7c6fcb13b`
- source_hash: c87bbe5eeb99003c63f70cf6424b31a6282e6bae683807a01f42c6b986519162
- status: approved

## 变更摘要

- 新增 `app/render.rs`：独立 `sfx-render` 线程 + `RenderWorker`。
  - `RenderJob { seq, key, payload }`，payload 为 Patch/Music 的 clone。
  - `RenderKey { tab, rate, version, music_version, patch }` 标识文档状态。
  - 共享 `Arc<AtomicU64>` 保存最新 seq，worker 在渲染前/发送前各检查一次，过期任务直接跳过。
  - `RenderOutcome { seq, key, samples, envelope, peak }`。
- UI 侧 `SfxLogic::request_preview(rate)`：仅当 `preview_pending()`、距上次请求 ≥80ms、且同一 key 不在飞行中才入队；`poll_render()` 每帧排空结果，只有 key 与当前状态匹配才写入缓存并 `bump_view`。
- `play()` 先 `cancel_preview()` 再同步 `ensure_cache()`，保证播放样本不被旧预览覆盖；打开/撤销/切 patch 通过 version / selected_patch 让旧结果自动失效。
- 兜底：worker 不可用或任务超过 5s 未回传时取消并退回同步 `ensure_cache_soon()`。
- headless/selfcheck 使用 `SfxLogic::new(state, background_preview=false)`，保持确定性输出。
- `synth::ENVELOPE_BINS` 抽为共享常量。
- 新增 worker 测试：同 key 去重、真实渲染回传序列/包络/峰值。

## 自审发现（已修）

1. `SfxLogic::new` 在 `commands.rs` 中构造，首次只改了 `app.rs` 导致缺字段 → 同步补 `render`。
2. 初版删除 `ensure_cache_soon` 后，worker 启动失败会让 `needs_frame` 永远为真但缓存不更新 → 恢复同步节流兜底。
3. worker 线程若 panic，UI 会一直等待 → 增加 `stalled()`（5s）取消 + 同步兜底。
4. selfcheck 首次因异步结果未在 2 帧内到达，命令数从 2552 掉到 1727 → headless 改走同步路径，计数恢复且不依赖线程调度。
5. 同一 key 重复请求会重复渲染 → `pending_key` 去重。

## 第二审查发现

### Blocker

无。

### Major

1. **渲染线程 panic 无 catch_unwind**：目前依赖 5s watchdog + 同步兜底，UI 不会永久卡住，但会白等一次。后续可在 worker 内 `catch_unwind` 并回传带错误标记的结果。
2. **队列上限**：拖动时每 80ms 可排入新任务，正常渲染远快于该速率；若遇到极端大工程渲染 >80ms，队列会积累过期任务（会被快速跳过，但仍占内存）。可改为容量 1 的 `sync_channel` + try_send。
3. **clone 成本**：每次请求 clone 整个 Patch/Music。当前工程规模很小；稳定 ID / 采样波形后再评估共享 `Arc`。
4. **播放仍同步渲染**：`play()` 会阻塞到缓存生成，这是为了音频拿不到样本时的正确性；后台化播放缓冲留给音频 RT 化。

### Minor

- 结果只在 `pending_key == outcome.key == 当前 key` 时应用，`pending` 被 cancel 后到达的结果会直接丢弃（符合预期但无日志）。
- 同步兜底与后台路径的包络规格已共享，但节流参数（80ms / 5s）是两处常量。
- worker 线程名固定，多个 SfxLogic 实例会创建多个同名线程（实际只有一个实例）。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（64 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] 实机启动 + 2s 后截图：异步结果到达后波形、peak、状态栏正常
- [x] `identical_keys_are_not_requested_twice`
- [x] `worker_renders_the_requested_patch`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 音频播放前的缓存生成仍在 UI 线程；彻底消除需要音频 RT 化（Backlog）。
- 渲染线程未做 panic 隔离，watchdog 是最后的保护。
- 无跨平台线程优先级/QoS 设置。
- 本次不改变 DSP/导出/格式。

## 签署

- reviewer_1: approved
- reviewer_2: approved
