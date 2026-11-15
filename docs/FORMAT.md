# 工程与导出格式

## 0. 工程内预设库（不属于 `sfx.project.json`）

工程是一个目录。`PROJECT | PRESETS` 左栏的 PRESETS 读取当前工程目录下的
`presets/*.json`（每个文件一个 `Patch` JSON）。预设随工程移动/复制，不写入
`sfx.project.json`，也不影响 `--dump/--bake/--export`。

```text
<project>/
├── sfx.project.json
├── presets/
│   ├── laser.json
│   ├── coin.json
│   └── .favorites.json   # 可选，UI 收藏
└── audio/...
```

- GUI `NEW PROJECT`：写 1 个 starter patch 的工程文件 + `presets/`（从 demo 音色初始化）。
  `--new DIR` 额外 bake 音频。已存在 `sfx.project.json` 或非空 `presets/` 时拒绝覆盖。
- 预设加载时复用 `Project::validate()`：NaN/巨值 clamp、重名去重、数组上限。
- 拖入时间线：复制预设的 layers 到当前 patch，重新分配工程内 ID；落点时间
  作为预设内部 0 点（无 clip 的层生成 `start = drop + delay` 的显式 clip，
  有 clip 的层保留相对编排）。
- 拖到左栏：整份预设复制为工程 patch，名称自动去重（`name_2`…），`favorite`
  重置为 false。
- 预设收藏存 `presets/.favorites.json`（以预设名称为 key）。
- 当前 patch 的 `dsp_version/space/room` 不随拖入改变。
- 打开没有 `presets/` 的工程时 PRESETS 列表为空，不报错。

## 1. `sfx.project.json`

位于工程文件夹根部，UTF-8 JSON，`serde_json::to_string_pretty` 输出。

### 顶层

```json
{
  "version": 4,
  "next_id": 128,
  "name": "my_game",
  "patches": [],
  "music": {}
}
```

| 字段 | 说明 |
|---|---|
| `version` | 工程 schema 版本。缺失时按 1 处理，加载后迁移到当前版本。 |
| `next_id` | 稳定 ID 发号器；旧文件缺失时自动补全并顺延。 |
| `name` | 工程显示名。导出 manifest 会用到。 |
| `patches` | 音效列表，最多 512。 |
| `music` | 背景音乐参数。 |

### Patch

| 字段 | 说明 |
|---|---|
| `id` | 工程内稳定 ID（Patch/Layer/Clip 共享一个发号器），加载/编辑时补全并去重。 |
| `name` | 显示名，允许中文/空格。导出时转成安全文件名并去重。 |
| `category` | `impact/laser/ui/movement/magic/environment/combat`；导出为 `.tres` 的 `group`。 |
| `favorite` | 仅 Rust 端库筛选使用；不写入 `.tres`。 |
| `rate` | 8000–48000；v2 用于 bake 与无音频设备时的预览。v3 固定 48 kHz bake（`Patch::bake_rate()`）。 |
| `normalize` | v2：峰值 > 0.95 时压到 0.95。v3：母带后归一到 0.95（可升可降）。 |
| `space` | v3 混响/早期反射量 0–1；0 = 干声。缺省 0。 |
| `room` | v3 混响尺寸/尾音 0–1；缺省 0.5。 |
| `dsp_version` | DSP 公式代次：`2` = 干声单声道（与 Godot 插件一致），`3` = 立体声 + PAN + SPACE + 母带。新工程/预设为 3；旧文件缺失时按 2 处理，检查器提供 `UPGRADE TO HD`。 |
| `layers` | 每 patch 最多 32 层。 |

### Layer / Clip / Automation

- `sample_path` / `sample_pitch` / `sample_loop`：`Wave::Sample` 的 WAV 路径与变调/循环参数。`sample_data`（解码后的单声道样本）不写入 JSON，加载工程时重新读取；相对路径按工程根目录解析。
- `.tres` 导出的采样 patch 会整段 bake 到 `audio/baked/*.wav`（避免 Godot 侧嵌入 `AudioStreamWAV`）；`.tres` 导入仍把 `SAMPLE` 映射为 `Pluck`，不逐层往返采样。
- 所有数值在加载时 `validate()`：NaN/Inf 修复、范围 clamp、数组上限、重名去重。
- `pan`：-1（左）…0（中）…1（右），等功率声像；仅 `dsp_version >= 3` 生效。
- `sample_start`：0–0.95，采样从该比例处开始播放；`sample_reverse`：反向播放。
- v3 采样循环在末尾做 10 ms 交叉淡化（以 `sample_start` 为循环头），避免循环接缝爆音。
- `tone_env`：`adsr`（默认）或 `exp`；`exp_decay` 范围 0.5–30。
- `clip.start` 0–8s、`clip.duration` 0.005–3s、每层最多 64 个 clip。
- 自动化点按时间排序、去重，最多 512 个/轨道。

### 保存与恢复

- 写入流程：`sfx.project.json.tmp` → rename；已存在旧文件先复制为 `sfx.project.json.bak`。
- 解析失败返回错误，不会静默回退 demo。
- 加载后若发生迁移/clamp/重名，UI 状态栏会显示警告。

## 2. Godot 导出

### `audio/baked/*.wav`

- 16-bit stereo PCM，PCM 导入（`compress/mode=0`）。
- v3 patch 固定 48 kHz（保留去混叠后的完整频带）；v2 patch 仍用 `Patch.rate`。
- `Patch.rate` 对 v3 仅作历史字段保留，不再影响 bake。
- 长度 = 源时长 + 混响尾音（`space > 0` 时 `(0.12 + room × 1.08) × (0.35 + 0.65 × space)` 秒；v2 无尾音）。
- manifest 的 `sounds[]` 含 `channels: 2`、`bits: 16`、`dsp_version`。

### `audio/sfx/*.tres`

- 文件名为 `sanitize(patch.name)` 并全局去重（`coin`, `coin_2`, `sound`, `sound_2`）。
- v3 声音模型：锯齿/方波/Pulse/Triangle 使用 2× 过采样 + PolyBLEP/polyBLAMP 等价实现（Triangle 为积分方波）；FM/Arp/Pluck 以 2× 渲染后半带滤波抽取；鼓组与噪声为 v3 重写（噪声低通按 Hz 归一）。
- `.tres` 只表达 v2 参数图；`dsp_version >= 3`（以及含自动化/采样层）的 patch 会整段 bake 到 `audio/baked/*.wav` 并用 `baked = ExtResource(...)` 引用，保证导出与工程内试听逐样本一致。
- 导入 `.tres` 时 `dsp_version` 固定为 2（live 格式能表达的代次）。
- `resource_name` 保存原始显示名，用于重新导入。
- `group` 保存 category 的 label（`Impact`、`UI`、`Combat`…），导入时映射回 `Category`。
- `normalize_layers` 为 bool。
- 每层写入 `tone_env` / `exp_decay`（Godot `SfxLayer` 已有属性）。
- 有自动化（gain/cutoff points）的 patch：
  - 额外 bake 出 `audio/baked/<name>.wav`；
  - `.tres` 写 `baked = ExtResource(...)`，Godot 直接播放烘焙结果；
  - 层参数仍保留，便于编辑。

### `audio/music.tres`

- 写入完整的 `SfxMusic` 参数（bpm/bars/rate/patterns/chords/...）。

### `audio/baked/`

```
audio/
├── baked/
│   ├── <name>.wav
│   ├── <name>.wav.import   # compress/mode=0，强制 PCM
│   ├── music.wav
│   └── music.wav.import
├── sfx_manifest.json
└── sfx/
    ├── <name>.tres
    └── ...
```

`sfx_manifest.json`（v2）：

```json
{
  "version": 2,
  "schema_version": 4,
  "project": "my_game",
  "sounds": [
    {
      "name": "coin",
      "patch": "Coin",
      "category": "UI",
      "favorite": true,
      "dsp_version": 3,
      "space": 0.3,
      "room": 0.55,
      "wav": "res://audio/baked/coin.wav",
      "rate": 22050,
      "channels": 2,
      "bits": 16,
      "duration": 0.16,
      "loop": false
    }
  ],
  "music": {
    "wav": "res://audio/baked/music.wav",
    "rate": 11025,
    "duration": 7.27,
    "loop": true,
    "db": -17.0
  }
}
```

## 3. 已知限制

- `favorite` 不进入 `.tres`（Godot `SfxPatch` 没有该属性）；只存在于 JSON 与 manifest。
- 未知的 `group` 字符串导入时保持默认 `Impact`。
- `Wave.SAMPLE` 暂不支持，导入为 `Pluck`（见 `docs/PARITY.md` D4）。
- `Patch.rate` 不影响实时预览；实时预览使用音频设备采样率。

## 4. 版本迁移

| 版本 | 变更 |
|---|---|
| 1 | 无 `version` 字段的初始格式。 |
| 2 | 增加 `version`、`Patch.dsp_version`、`Layer.tone_env/exp_decay`；加载时自动迁移。 |
| 3 | 增加稳定 ID：`Project.next_id`、`Patch/Layer/Clip.id`；缺失/重复时自动分配，加载时迁移。 |
| 4 | 增加 `Wave::Sample` 的 `Layer.sample_path/sample_pitch/sample_loop`；采样数据不入 JSON，加载时按路径解码。 |

迁移函数：`model::Project::migrate`。新增字段一律 `#[serde(default)]`，保证旧文件可读。

`dsp_version` 与 schema 版本相互独立：2 → 3 不改变 JSON 结构，只改变渲染
（v3 增加 PAN/SPACE/母带与立体声导出）。旧工程加载后保持 2，需在检查器点
`UPGRADE TO HD` 才会切换到 v3。
