# Arrangement v5 — Sound / Arrangement 分离设计

- status: design only；本轮**未实现**代码迁移
- date: 2026-11-16
- related: `docs/reviews/0034-architecture-audit.md`, `0042-independent-audit.md`

## 1. 为什么不能直接改

`Patch` 现在是“音色 + 排列”的混合体：

```text
Patch
  layers[]
    Layer
      ...synth params...
      clips[]   // start / duration / gain / pitch / automation
```

`Layer.clips` 让同一个音色不能在时间线上复用；复制音色会复制编排。与此同时：

- `.tres` 导出把每个 `Patch` 当作一个 `SfxPatch` 资源；
- Godot 插件 `sfx_patch.gd` / `sfx_layer.gd` 直接消费 `layers` 与 `clips`；
- parity golden 以 Patch 为单位逐样本比较。

因此 v5 不是一次 Rust 内部重构，而是**格式 + 跨端资源 + golden** 的同步迁移。

## 2. 目标模型

```rust
// schema v5
struct Project {
    version: 5,
    next_id: u64,
    name: String,
    sounds: Vec<Sound>,
    arrangement: Arrangement,
    music: Music,          // 仍是当前唯一音乐；多 music 留到 v6
}

struct Sound {
    id: u32,
    name: String,
    category: Category,
    favorite: bool,
    dsp_version: u32,
    space: f32,
    room: f32,
    layers: Vec<SoundLayer>,   // 不含 clips
}

struct SoundLayer {
    id: u32,
    // 现有的 synth 参数（wave/freq/ADSR/filter/sample/...）
    delay: f32,                // 相对 clip 起点
    duration: f32,
    gain: f32,
    pan: f32,
    // ...其余字段保持不变
}

struct Arrangement {
    clips: Vec<ArrangementClip>,
}

struct ArrangementClip {
    id: u32,
    /// 引用 Sound。首版保留 layer_id，以无损迁移旧 Patch 的逐层 clips。
    sound_id: u32,
    layer_id: Option<u32>,
    start: f32,
    duration: f32,
    gain: f32,
    pitch: f32,
    gain_points: Vec<AutomationPoint>,
    cutoff_points: Vec<AutomationPoint>,
}
```

### 为什么首版保留 `layer_id`

旧 `Patch` 的不同 layer 可以有完全不同的 clip 起点：

```text
patch "explosion"
  layer 0 clips: [0.00, 0.30]
  layer 1 clips: [0.05]
  layer 2 clips: [0.12, 0.40]
```

如果 `ArrangementClip` 只引用 Sound，那么一个 clip 会同时播放所有 layer，无法无损迁移。
首版用 `ArrangementClip { sound_id, layer_id }` 逐层迁移，等 UI/DSP 支持“sound group
clip”后再增加 `layer_id = None` 的语义。

## 3. 迁移步骤（必须独立分支）

1. **schema 类型 + serde**
   - 增加 `Sound` / `SoundLayer` / `Arrangement` / `ArrangementClip`；
   - `Project` 增加 `sounds` / `arrangement`，保留旧 `patches` 的只读解析；
   - `PROJECT_VERSION = 5`，`migrate_v4_to_v5()`。
2. **迁移算法**
   - 每个旧 `Patch` → 一个 `Sound`，layers 清空 `clips`；
   - 对每个 `layer.clips[i]` → `ArrangementClip { sound_id, layer_id, start, ... }`；
   - `layer.delay` 保留为 sound 内偏移；旧 implicit clip 变为一个显式 clip；
   - ID 发号器统一到 `next_id`。
3. **DSP / 预览**
   - `render_patch` 的输入改为“Sound + 选中的 arrangement clips”；
   - 时间线渲染按 `ArrangementClip` 分组，不再遍历 `Layer.clips`；
   - `fx::render_duration` 取 arrangement 的最远 end。
4. **UI**
   - 时间线行 = arrangement 中的 clip 组；
   - 左栏 SOUND = `Project.sounds`；
   - 拖动 clip 时改 `ArrangementClip`，不再改 Layer。
5. **导出**
   - `SfxSound.tres`：等价于今天的 `SfxPatch.tres`，但 layers 不含 clips；
   - 新增 `SfxArrangement.tres`（或由游戏侧组合）；旧 `.tres` 继续可读；
   - manifest 增加 `sounds[]` 与 `arrangement[]`，保留旧 `sounds` 字段兼容。
6. **Godot 插件**
   - `sfx_patch.gd` 拆为 `sfx_sound.gd` + `sfx_arrangement.gd`；
   - `sfx_layer.gd` 保留，去掉 clips 路径；
   - 旧 `SfxPatch` 作为兼容壳，内部转成 sound+arrangement。
7. **Parity**
   - 新 fixture：同一 sound 被 arrangement 引用两次；
   - 旧 fixture 全部保持 0.000 dB peak/RMS 通过；
   - 增加 `arrangement.tres` 的 Godot 渲染对照。

## 4. 验收条件

- v4 → v5 → 再打开，音色/时间线/导出逐样本一致（除新增引用语义）。
- `./scripts/parity.sh` 旧 9 个 fixture 不变，新增 2 个 fixture 通过。
- Godot 4.x 能同时加载 `SfxSound` 与 `SfxArrangement`；旧 `SfxPatch` 不报错。
- `--bake` / `--export` / `--dump` 在 v4/v5 临时工程都成功。
- 撤销/重做、复制 sound、repeat clip 均有回归测试。

## 5. 明确非目标

- 多 music 同时播放/编排（v6）。
- 实时 voice 引擎（需要在音频回调内做 voice mixing，和当前“整段渲染”架构不同）。
- 把 arrangement 做成完整 DAW 多轨混音。

## 6. 结论

v5 是**格式迁移**，不是“再叠一层抽象”。在没有 Godot 侧同步与 parity 基线的情况下
直接改 Rust 模型，会破坏现有 `.tres` 消费方。建议下一轮单独开迁移分支，只做 v5，
不混 UI 功能。
