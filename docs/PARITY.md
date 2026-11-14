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
| clip 起点/时长/增益 | `render_patch` | `SfxPatch._build_layers` | ✅ |
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

## 4. 必须遵守的改动规则

1. 改任何 DSP 公式、RNG、精度、包络，必须：
   - 提升 `Patch.dsp_version`（或新增 dispatch），保证老工程保持老声音；
   - 更新本文件的差异表；
   - 增加/更新 golden 用例。
2. 新增波形/参数必须同时改 Rust 与 Godot，或明确标注「仅单端」。
3. 每个版本至少保留一组 `fixtures/*.json` + 期望 WAV 指纹（Phase 3 落地）。
4. 不允许为了让测试通过而单方面修改公式而不改另一端。

## 5. Golden 测试计划（Phase 3）

1. `fixtures/patches/*.json`：覆盖每种波形、clip、自动化、tone_env、normalize。
2. Rust：`cargo run -- --golden fixtures/` 输出 WAV + `sha256`/RMS/peak 报告。
3. Godot：扩展 `plugins/sfx_studio/tools/self_test.gd`，读取同一 JSON，渲染后比较 RMS/峰值（容差）。
4. CI：两端都跑；差异超过阈值即失败。

在 golden 落地前，Rust 侧的单测只保证「确定性、有限、长度正确」，不保证跨端一致。
