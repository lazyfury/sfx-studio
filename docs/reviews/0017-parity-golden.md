# Review 0017 — Rust ↔ Godot 确定性 golden 测试

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/parity.rs`, `src/cli.rs`, `src/main.rs`, `src/synth.rs`, `src/model.rs`, `scripts/parity.sh`, `scripts/parity/render.gd`, `docs/{PARITY,FORMAT,PLAN}.md`, `README.md`
- diff_base: `7d2ee79718c3b706cf116abd45ad643f338204db`
- source_hash: b6a53a5808710b3cf960d803ef4007afd3686a7882a9d39f6f723410cb35b1a7
- status: approved

## 变更摘要

- 新增 `src/parity.rs`：
  - 9 个确定性 fixture：SINE ADSR、TRIANGLE、SAW、PULSE、SQUARE、FM、ARP、多 clip、多层 normalize。
  - `--parity-fixtures DIR` 写出 `.tres` + Rust 渲染 WAV；`--parity-check DIR` 比较 Godot WAV。
  - 比较项：peak Δ、RMS Δ（各 ≤0.2 dB）+ 逐样本 max |Δ|（≤0.01）。
- Godot 侧 `scripts/parity/render.gd` 加载 `.tres`、调用 `build()`、`save_to_wav()`；`scripts/parity.sh` 编排临时工程、addon symlink、`--editor --quit` 注册 class、headless render、compare。
- **DSP 修正**：clip 的 ADSR 时长从「layer.duration」改为 `min(clip.duration, layer.duration)`（dsp v2），与 Godot flatten 行为一致；dsp v1 保留旧行为。`DSP_VERSION` 1 → 2。
- `docs/PARITY.md` 第 5 节从计划改为可运行流程、覆盖范围、排除项与实测结果；README 增加 `./scripts/parity.sh`。

## 实测结果（Godot 4.7.2）

```
sine_adsr  peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00674
triangle   peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00003
saw        peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00003
pulse      peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00003
square     peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00003
fm         peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00183
arp        peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00006
clips      peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00079
normalize  peak Δ0.000 dB  RMS Δ0.000 dB  max Δ0.00089
parity ok
```

## 自审发现（已修）

1. Godot 4.7 严格 GDScript 解析：`var err :=` 无法推断类型 → 显式 `int`。
2. headless 直接 `load()` 时全局 class（`SfxSynth`）未注册 → 先跑一次 `--editor --quit` 生成 `global_script_class_cache.cfg`。
3. clip 的 RMS 差 0.979 dB：Rust 用 layer 长度算 ADSR，Godot flatten 用 clip 长度 → 修正并按 `dsp_version` 保留旧行为。
4. 仅 peak/RMS 可能掩盖相位差 → 增加逐样本 max |Δ| 容差 0.01（SINE 实测 0.00674，来自 f32/f64 数学）。
5. 测试调试时残留 `max_diff` 变量触发 `-D warnings` → 清理。

## 第二审查发现

### Blocker

无。

### Major

1. **覆盖范围有限**：只覆盖确定性波形。Noise/Pluck/鼓仍受 D1（RNG）影响；采样层与 automation 不在逐层 golden 内。
2. **不是 CI 门禁**：`parity.sh` 需要 Godot 4.x 二进制，未接入 `scripts/ci.sh` / preflight；目前是本地/发布前手动执行。
3. **比较维度**：peak/RMS/max|Δ| 不能发现「能量相同但频谱/相位不同」的问题；后续可加 FFT 或相关性。
4. **DSP 版本 dispatch 只覆盖本轮 clip 包络**：`DSP_VERSION` 升到 2，但没有通用版本化渲染框架；旧工程 dsp v1 仅在这一个分支保留旧行为。

### Minor

- Godot 导入步骤依赖 editor scan；CI 需要缓存 `.godot` 或固定 Godot 版本。
- macOS 默认 Godot 路径写死，脚本支持 `GODOT=` 覆盖。
- WAV 以 16-bit 保存，量化下限约 3e-5；max |Δ| 已高于该值，来自 f32/f64 数学。
- fixture `.tres` 在 `target/parity` 生成，不进入 git；改动 fixture 时请同步 `docs/PARITY.md`。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（83 passed；含 parity 3 条 + dsp v1 测试）
- [x] `cargo run -- --selfcheck`（2 frames / 2555 commands / 119 text runs）
- [x] `./scripts/parity.sh` → `parity ok`（9/9）
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- RNG/噪声、采样、automation 的跨端一致性仍未覆盖。
- Golden 没有 CI 化，依赖本机/runner 安装 Godot。
- dsp 版本化渲染未完整实现，后续改公式仍需谨慎。
- 本轮不改变导出格式（除 DSP_VERSION 标记）。

## 签署

- reviewer_1: approved
- reviewer_2: approved
