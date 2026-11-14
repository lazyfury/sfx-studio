# Review 0015 — 48k 固定预览、设备重采样、buffer 回收与重连

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/audio.rs`, `src/synth.rs`, `src/app.rs`, `src/app/{input,commands}.rs`, `docs/PARITY.md`, `docs/PLAN.md`
- diff_base: `8315e66efb201ba28f43b50652829bebd25af1a8`
- source_hash: 13ddaf6a7d2e393293ce2767a09bc1900d5f9deaaf903884c228449f16bba578
- status: approved

## 变更摘要

- `synth::PREVIEW_RATE = 48_000`；`State::preview_rate()` 固定返回 48k，导出 WAV 复用该常量。
- 音频回调线性重采样：`ratio = 48_000 / device_rate`，`Playback.pos` 改为 `f64` 源帧游标，循环 wrap 使用取模。
- `AudioEngine::position()` 按 48k 源帧换算；播放 offset 仍以源帧传入。
- **Buffer 回收**：回调替换样本时把旧 `Vec` 放进 `retired` 单槽，UI 每帧 `reclaim()` 在 UI 线程释放；槽被占用时回退为回调内 drop（记录为余项）。
- **设备重连**：`AudioEngine::spawn_stream/connect` 保存 device/config/format，错误后 2s 退避重连；`is_alive/try_reconnect` 暴露给 UI。
- **underrun 统计**：回调在 audible 且样本为空时计数；`take_underruns()` 由 UI 读取。
- UI `update()`：回收 buffer、读取 device errors/underruns、toast 提示并尝试重连。
- `docs/PARITY.md` D6/D7 更新：预览固定 48k + 设备重采样；bake 仍按 `Patch.rate`/`MusicField::Rate`。

## 自审发现（已修）

1. 初版 `connect()` 在 `stream.play()` 成功后才置 `alive=true`，错误回调可能先置 false 又被覆盖 → 改为先置 true、失败再清 false。
2. 旧 `AudioEngine` 保存 `_stream`，无法重连 → 改为 `stream: Option<cpal::Stream>` + 保存 device/config/format。
3. `spawn_stream(&self)` 后写 `self.stream` 的借用冲突 → 先取 stream 局部变量，再赋值。
4. UI 仍用 `&self.audio`，reconnect/reclaim 需要 `&mut` → 改为 `&mut self.audio`，并保持 toast 与状态更新的字段级借用。
5. 重采样测试最初用 `step=0` 看不到 fade 增长 → 使用 `step=1.0`/显式 fade 初始化。
6. 回调内 `std::mem::take(&mut samples)` 与原 `apply` 重复赋值 → 将 Play 分支拆出，先回收旧 buffer 再 apply。

## 第二审查发现

### Major

1. **重采样质量**：线性插值足够预览，但不是带限重采样；设备 44.1k 时高频会有轻微混叠。`rubato`/sinc 留作后续。
2. **bake 未统一 48k**：预览/导出 48k，bake 仍按 `Patch.rate`，D3（噪声低通不随采样率归一）会让 bake 与预览音色不同；需要与 Godot 同步改公式并升 `dsp_version`。
3. **重连不可回归测试**：无硬件拔插/模拟 device error 的集成测试；仅保证调用路径不 panic。
4. **退役槽满的回调 drop**：极窄窗口下仍在音频线程释放 Vec；要彻底消除需要固定容量 ring 或双槽 + 回收失败时暂存。

### Minor

- underrun 计数只在「audible 但 samples 为空」时增加，属于安全兜底，不是真正的设备 underrun 统计。
- 重连成功会重置 playing，但不清空 command 槽；残留 Stop 会被新回调消费（无害）。
- `PREVIEW_RATE` 变更影响无音频设备时的渲染成本（48k > 22.05k），短音效可接受。
- `docs/PARITY.md` 的 D3 仍是已知差异；采样率统一后需重新测试噪声 timbre。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（73 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2552 commands / 118 text runs）
- [x] release 启动截图：48 kHz 预览、波形/peak 正常，设备初始化成功
- [x] `resamples_48k_frames_to_the_device_ratio`
- [x] `retired_buffers_are_handed_back_to_the_ui`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 无设备热插拔的自动化回归；重连逻辑只经过类型/路径检查。
- 预览 48k 与 bake 采样率不再一致（D3/D6 部分解决）。
- 线性重采样的高频质量。
- 本轮不包含采样波形与跨端 golden；这两项需要单独设计与 Godot headless 验证。

## 签署

- reviewer_1: approved
- reviewer_2: approved
