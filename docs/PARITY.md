# Rust ↔ Godot 音频一致性规范

> 目标：同一份参数在 SFX LAB（Rust）与 Godot 插件里「听起来一致」。
> 本文记录已实现的一致点、已知差异、以及把差异变成可验证规格的计划。

## 1. 一致性等级

不承诺 bit-exact。不同运行时（macOS libm / Godot 内置数学、f32 / f64）不可能逐位相同。承诺：

- **参数语义一致**：同一字段含义、范围、单位相同。
- **结构一致**：层叠加、clip 起点/时长/增益/变调、自动化包络、归一化阈值相同。
- **听感一致**：在相同采样率下，RMS 差异 < -50 dBFS，峰值差异 < 0.1 dB（目标值，待 golden 测试落地后收紧）。
- **确定性**：同一实现、同一平台、同一 seed，多次渲染逐位相同。

## 2. 已对齐

| 项目 | Rust | Godot | 状态 |
|---|---|---|---|
| 层叠加 / 最长时长 | `render_patch` | `SfxPatch._build_layers` | ✅ |
| clip 起点/时长/增益 | `render_patch`（ADSR 使用 clip 时长） | `SfxPatch._build_layers` | ✅ 0017 golden 对齐 |
| clip 变调 | `Layer::with_pitch` | `SfxLayer.with_pitch` | ✅ |
| 增益/截止自动化 | `Clip::gain_at/cutoff_at` | `SfxClip` | ✅ |
| ADSR 公式 | `synth::adsr` | `SfxSynth.adsr` | ✅ |
| 音调包络 EXP | `Layer.tone_env/exp_decay` | `SfxLayer.tone_env/exp_decay` | ✅ 本轮补齐 |
| 指数噪声包络 / Sine | `render_noise` | `_build_noise` | ✅ |
| 鼓 / FM / Pluck / Arp 公式 | `synth.rs` | `sfx_layer.gd` | ✅ |
| normalize 阈值 0.95 / 0.85 | `render_patch` / `render_music` | 同名逻辑 | ✅ |
| 空 notes/chords → 空流 | `render_music` | `SfxSynth.music` | ✅ 本轮补齐 |
| 16 步 pattern / kick/hat 位掩码 | `render_music` | `SfxSynth.music` | ✅ |
| 文件元数据 | `resource_name`、`group` | `SfxPatch.group` | ✅ 本轮补齐 |
| 采样层渲染公式 | `render_sample` | `_build_sample` | ✅ Rust 单端（路径 WAV）；`.tres` 往返未支持 |
| v2 干声渲染（旧工程） | `synth::render_patch` | `SfxPatch._build_layers` | ✅ 0018 重构后 golden 不变 |
| v3 PAN / SPACE / 母带 / 立体声 | `fx::render_patch` | — | ⛔ 仅 bake（见 D8） |

## 3. 已知差异（必须知晓）

| # | 差异 | 影响 | 处理 |
|---|---|---|---|
| D1 | RNG：Rust xorshift64* vs Godot `RandomNumberGenerator`（PCG32） | **所有噪声类波形**（Noise/Pluck/Snare/Hat/Clap）同 seed 声音不同 | Phase 3：双端统一算法 + 测试向量；在此之前不要承诺「同 seed 同声音」 |
| D2 | 精度：GDScript `float` 为 f64，Rust DSP 为 f32 | 长音相位/滤波轻微漂移 | Phase 3：相位回绕（`rem_euclid(TAU)`）双端同时做；以容差测试为准 |
| D3 | 噪声低通系数与采样率无关（`lerp(low, noise, lowpass_from)`） | 22.05k / 48k 下噪声音色不同 | 有意保留以兼容 Godot；Phase 3 双端同时改为按 Hz 归一化，并升 `dsp_version` |
| D4 | Godot `SAMPLE` 使用内嵌 `AudioStreamWAV`；Rust 使用 `sample_path` + WAV 解码 | `.tres` 导入仍映射 `SAMPLE→Pluck`；Rust 样例 patch 导出时整段 bake，不做逐层采样往返 | 0016：Rust 端预览/JSON/bake 已支持；`.tres` 单端差异已标注 |
| D5 | `_s16` 截断 vs Rust `round()` | 最差 1 LSB | 不修（人耳不可闻）；若做 bit-exact 再统一 |
| D6 | 实时预览固定 48k 并线性重采样到设备率；bake 仍按 `Patch.rate` | 预览与 bake 的滤波/噪声系数仍可能不同（D3） | 0015 已完成预览侧固定 48k + 重采样；bake 统一待 D3 决策 |
| D7 | `MusicField::Rate` 仅影响 bake；实时预览已固定 48k | UI 标注为 `BAKE RATE` | 0015 起预览 48k + 设备重采样；bake 仍独立 |
| D9 | DSP v3 更换了声音模型：PolyBLEP/2× 过采样振荡器、积分方波 Triangle、v3 鼓组、按 Hz 归一的噪声低通、采样 start/reverse/循环交叉淡化 | v2 路径逐样本不变（golden 仍 9/9）；v3 与 Godot 实时 DSP 不追求一致 | 所有 v3 patch 导出 `.tres` 时自动 bake（D8），发布以 WAV 为准。 |
| D8 | DSP v3（PAN/SPACE/早期反射/混响/母带）只在 Rust 与 bake 中实现 | 外部手写 / 旧的 live `.tres` 仍是 v2；同参数会比 app 试听更干、更窄 | `dsp_version >= 3`（及含自动化/采样）的 patch 导出 `.tres` 时一律整段 bake，以 `baked` 引用 WAV，播放与 app 完全一致；`.tres` 导入固定 `dsp_version = 2`。 |

## 4. 必须遵守的改动规则

1. 改任何 DSP 公式、RNG、精度、包络，必须：
   - 提升 `Patch.dsp_version`（或新增 dispatch），保证老工程保持老声音；
   - 更新本文件的差异表；
   - 增加/更新 golden 用例。
2. 新增波形/参数必须同时改 Rust 与 Godot，或明确标注「仅单端」。
   v3 的混音/母带参数按设计「仅 bake」：带这些参数的 patch 不允许以 live
   `.tres` 导出，导出时自动 bake（见 D8）。
3. 每个版本至少保留一组 `fixtures/*.json` + 期望 WAV 指纹（Phase 3 落地）。
4. 不允许为了让测试通过而单方面修改公式而不改另一端。

## 5. Golden 测试（0017 起可运行）

```bash
./scripts/parity.sh                 # 默认 /Applications/Godot.app/.../Godot
GODOT=/path/to/godot ./scripts/parity.sh
```

脚本流程：

1. `cargo run -- --parity-fixtures target/parity`：生成 9 个确定性 `.tres` + Rust 渲染 WAV。
2. 建临时 Godot 工程，symlink `plugins/sfx_studio` 到 `addons/`，跑一次 `--editor --quit` 注册全局 class。
3. `render.gd` 逐个 `load().build().save_to_wav()`。
4. `cargo run -- --parity-check target/parity`：比较 peak / RMS（容差各 0.2 dB）与逐样本 max |Δ|（容差 0.01）。

当前覆盖：

- SINE / TRIANGLE / SAW / PULSE / SQUARE（含 ADSR、freq sweep）
- FM、ARP
- 多 clip（起点/时长/增益/变调）
- 多层 normalize

不覆盖（有意）：

- **DSP v3 混音链 / 声音模型**：fixture 以 `synth::render_patch`（v2）渲染并逐样本比较，v3 由 `fx::render_patch` + `synth::render_layer_hq` 单端负责；
- Noise / Pluck / 鼓：仍受 D1 RNG 差异影响；
- 采样层：Godot 内嵌 `AudioStreamWAV`，Rust 为路径 WAV；
- automation：导出时整段 bake，不参与逐层 golden。

0017 结果：9/9 fixture 的 peak/RMS 差异均为 0.000 dB；max |Δ| 最差 0.00674（SINE，f32/f64 数学差异），其余 ≤0.00183（Godot 4.7.2，`Patch.rate` 渲染）。

DSP 修正：clip 的 ADSR 现在使用 `min(clip.duration, layer.duration)`，与 Godot flatten 行为一致；`DSP_VERSION` 升为 2，旧工程（dsp v1）仍保留 layer 长度的包络。

