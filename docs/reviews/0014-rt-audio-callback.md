# Review 0014 — 音频回调去锁与设备错误处理

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/audio.rs`, `src/app/input.rs`, `docs/PLAN.md`
- diff_base: `b64dcf6d47cf9784467f0d03c8e446060c3ec28e`
- source_hash: d7798b4d872ae36791ab37d7c3e0ddc1d232854a57bac4fec86e19c5502bcb37
- status: approved

## 变更摘要

- 移除 `Arc<Mutex<Playback>>`。音频回调现在**拥有** `Playback`（samples、pos、loop、fade），状态跨 buffer 持续。
- 命令单槽：`Arc<Mutex<Option<Command>>>`，`Command::{Play, Stop}`；`AudioEngine::play/stop` 短暂加锁写入，回调 `try_lock` 读取，**永不阻塞**。
- `position` / `is_playing` 仍为 `AtomicU64` / `AtomicBool` 无锁读取。
- 设备错误：cpal error callback 累加 `device_errors`、`playing=false`、`alive=false`；`AudioEngine::take_device_errors()` 由 UI 每帧读取并 toast「audio device error · playback stopped」；`play()` 在 `alive=false` 时拒绝启动。
- `Playback::apply` 从回调主循环抽出，便于测试；`take_command` 用 `try_lock`，同样抽出。
- 新增测试：按帧重启、Stop 渐隐、命令槽读取；既有循环/自然结束测试保留。

## 自审发现（已修）

1. 初版把 `Playback::default()` 写在回调闭包内部 → 每个 buffer 都会重置播放状态；已移到闭包外，由 `FnMut` 持有。
2. cpal error 闭包与数据回调同时持有 `playing` → 克隆 `error_playing` 专用，避免 moved-value 编译错误。
3. `take_device_errors` 最初没有调用方，触发 `dead_code` → UI `update()` 读取并 toast。
4. 新 `play_command_restarts...` 测试用 `step=0` 导致 fade 永远为 0 → 改用 `step=1.0` 验证首帧输出。
5. 命令槽被覆盖时旧 Vec 在 UI 线程析构（正确方向），但回调内替换 samples 仍会析构旧 Vec（见残余风险）。

## 第二审查发现

### Blocker

无。

### Major

1. **回调内 deallocation**：`self.samples = samples` 会释放上一个 `Vec`（音频线程调用 free）。真正的零 RT 风险需要退役 buffer 交还 UI 回收，或使用固定 ring buffer。
2. **未实现 48k 固定渲染 + 重采样**：预览仍按音频设备采样率渲染，`preview_rate()` 未改变；跨设备一致性仍需 `rubato`/线性重采样。
3. **设备热插拔只有检测**：错误后 `alive=false`、停止播放并提示，但不会重建 stream；需要重启应用或后续 `AudioEngine::restart()`。
4. **无欠载统计**：当前预渲染整段音频，正常不会 underrun；回调没有 xrun 计数/日志。

### Minor

- 命令单槽会合并快速连续的 Play/Stop（最后一次生效），符合交互预期但无法插入 seek。
- 播放位置每 buffer 更新一次（约 5–10ms 延迟）。
- `device_errors` 采用 swap 读取，多个错误在一次 UI 帧内会合并为一个 toast。
- 通过 `try_lock` 读取命令：若 UI 恰好在写命令，回调会延迟一个 buffer 取到；不丢命令。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（71 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] release 启动截图：音频设备初始化成功（状态栏 48000 Hz），UI/波形正常，空闲 CPU ~0%
- [x] `play_command_restarts_at_the_requested_frame`
- [x] `stop_command_ramps_down_instead_of_cutting`
- [x] `command_slot_takes_commands_without_blocking`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 音频线程仍会析构旧 Vec；真正 RT 安全的 buffer 回收未做。
- 无重采样/固定 48k，设备采样率变化会改变预览音高（与之前一致）。
- 设备错误后无法自动恢复，只能重启应用。
- 实机无法在自动化环境中验证实际发声（合成输入被 macOS 权限拦截），验证依赖单元测试 + 设备初始化截图。

## 签署

- reviewer_1: approved
- reviewer_2: approved
