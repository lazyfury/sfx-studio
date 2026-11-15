# Review 0019 — HD 声音模型：去混叠 / 鼓组 / 采样播放

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/synth.rs`, `src/fx.rs`, `src/model.rs`, `src/export.rs`, `src/cli.rs`, `src/app/commands.rs`, `src/app.rs`, `src/ui.rs`, `src/ui/paint.rs`, `docs/{PLAN,FORMAT,PARITY}.md`, `README.md`
- diff_base: `ba3b1a235f26fad01e0f348f94e57840367d99d0`
- source_hash: 740ba94234d85ef2536b7485dc719a6aa2b4d6f843190e12dbe9c6edf6b73323
- status: approved

## 变更摘要

- **去混叠**（v3）：Saw/Pulse/Square 用 PolyBLEP；Triangle 用积分方波；tone/FM/Arp
  以 2× 渲染后过 31 抽头半带 FIR（16 个非零抽头）抽取。5 kHz 锯齿的 13 kHz
  折叠杂散从 0.0359 降到 0.000004（约 −79 dB）。
- **鼓组重写**（v3）：kick（click + 指数音高包络 + sub + soft clip）、snare（双腔体
  + SVF 带通噪声 + 瞬态）、hat（6 个 PolyBLEP 方波金属分音 + 6.5 kHz 高通）、
  clap（4 段带通突发 + 短尾）。
- **采样率一致性**：噪声低通按 22.05 kHz 参考系数换算成 Hz；混响 damping 按 Hz；
  Pluck 环路滤波与增益按 Hz/参考率换算并对激励做方差归一；v3 固定 48 kHz bake。
  新增 `hq_voices_are_rate_consistent`（22k vs 44.1k 的 RMS / 过零率漂移 < 25%）。
- **采样播放**：`sample_start`（0–0.95）、`sample_reverse`、10 ms 交叉淡化循环；
  `SAMPLE` 检查器新增 `START` / `REVERSE`；加载新采样时重置这两个字段。
- **变体**：A/B/C 变体额外抖动 `space/room/pan`，游戏内随机选一个即为 round-robin。
- **示例工程**：新增 `--new DIR`（demo 工程 + 全部 bake），用于试听与回归。
- **尾音**：`tail_seconds` 随 `space` 缩放（`(0.12 + room×1.08) × (0.35 + 0.65×space)`），
  UI 短音不再带半秒近静音。
- `TrackOptions { clip_adsr, clip_fade, hq }` 显式区分 v2/v3；`synth::render_patch`
  仍是 v2 golden 路径。

## 自审发现（已修）

1. 混响 damping 是「每采样」系数，48k 预览与 22k bake 尾音亮度不同 → 改为按 Hz 求系数。
2. Pluck 的 KS 环路损耗与环路滤波都是每采样特性，22k 与 44.1k 亮度差 1.3 倍 →
   v3 独立实现：滤波截止 = 基频 × brightness，环路增益按 22.05k 参考换算。
3. 上述 Pluck 的噪声激励方差 ∝ 1/fs，导致高采样率电平低 30% → 按 `a/(2−a)` 归一。
4. v3 bake 仍用 `Patch.rate`（demo 22050），去混叠后的高频被截掉 → v3 固定 48 kHz，
   `Patch::bake_rate()` 统一 UI 文案、manifest 与导出。
5. Kick 的 click 用一阶差分噪声（带宽随采样率变化）→ 加 5 kHz 定带宽低通，
   过零率在 22k/44.1k 下一致。
6. 尾音公式不随 `space` 缩放，`coin` 0.11 s 却导出 0.55 s → 按 space 缩放。
7. 噪声低通最初用 `100×150^v` 映射，demo 会明显变暗 → 改为「按 22.05 kHz 参考系数
   反算 Hz」，保持预设音色并让预览/bake 一致。
8. clippy：`collapsible_match`、测试中的重复边界断言等已清理。

## 第二审查发现

### Blocker

无。

### Major

1. **不是真正的 round-robin**：多文件/velocity 层需要 Layer 模型与 Godot 侧扩展；
   本轮只提供 A/B/C 变体 + 随机挑选。已在 PLAN 0019「非目标」写明。
2. **鼓/噪声仍受 D1 RNG 差异影响**：v3 只在 Rust/bake 侧，Godot live 仍是 v2；
   导出已强制 bake（D8），因此不影响发布，但不能用 `.tres` live 对齐 v3 鼓组。
3. **48 kHz bake 体积**：v3 资产不再遵循 `Patch.rate`，烘焙文件约 2 倍；
   游戏侧需要接受 PCM 体积（原本就是 PCM）。
4. **采样循环交叉淡化固定 10 ms**：极短循环（< 40 ms）会被 `loop_len/4` 限制，
   可能仍有轻微接缝；未做主观听感回归。

### Minor

- `hq_voices_are_rate_consistent` 只覆盖 Sine/Saw/Pluck/Noise/Kick，未覆盖 Hat/Clap/FM/Arp。
- 去混叠只测了 5 kHz 单点；未做全频段扫频/THD+N。
- `--new` 会覆盖目标目录中的 `sfx.project.json`（无确认），CLI 语义已在 help 中写明。
- Triangle 的积分方波在极低频（< 30 Hz）可能有轻微数值漂移，靠母带 20 Hz 高通兜底。
- `.tres` 导入不恢复 `sample_start/reverse`（`.tres` 不写这些字段，v3 走 bake）。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（109 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2566 commands / 124 text runs）
- [x] `./scripts/parity.sh` → `parity ok`（9/9，v2 逐样本未变）
- [x] `--new /tmp/sfx-demo`：11 patch，48 kHz / 16-bit stereo，峰值统一 −0.45 dBFS
- [x] `--dump/--bake/--export` 端到端成功
- [x] Godot 4.7.2 加载 `explosion.tres`：`baked=true len=1.697 stereo=true rate=48000`
- [x] 5 kHz 锯齿 13 kHz 杂散：0.0359 → 0.000004
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 极端 `sample_start` + 短循环 + reverse 的组合未做听感覆盖。
- 鼓组的 `drum_tone/drum_snap` 映射与 v2 完全不同（v3 预期行为），旧工程升级后需重调。
- 变体使用 `SystemTime` 种子，结果不可复现（与既有 randomize/vary 行为一致）。
- 48k bake 下 30 s 音效 + reverb 的峰值内存约 40 MB（帧数上限已有）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
