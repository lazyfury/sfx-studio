# 工程与导出格式

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
| `rate` | 8000–48000；用于 bake 与无音频设备时的预览。 |
| `normalize` | 峰值 > 0.95 时压到 0.95。 |
| `dsp_version` | DSP 公式代次（当前 2；0017 修正 clip ADSR 使用 clip 时长）。 |
| `layers` | 每 patch 最多 32 层。 |

### Layer / Clip / Automation

- `sample_path` / `sample_pitch` / `sample_loop`：`Wave::Sample` 的 WAV 路径与变调/循环参数。`sample_data`（解码后的单声道样本）不写入 JSON，加载工程时重新读取；相对路径按工程根目录解析。
- `.tres` 导出的采样 patch 会整段 bake 到 `audio/baked/*.wav`（避免 Godot 侧嵌入 `AudioStreamWAV`）；`.tres` 导入仍把 `SAMPLE` 映射为 `Pluck`，不逐层往返采样。
- 所有数值在加载时 `validate()`：NaN/Inf 修复、范围 clamp、数组上限、重名去重。
- `tone_env`：`adsr`（默认）或 `exp`；`exp_decay` 范围 0.5–30。
- `clip.start` 0–8s、`clip.duration` 0.005–3s、每层最多 64 个 clip。
- 自动化点按时间排序、去重，最多 512 个/轨道。

### 保存与恢复

- 写入流程：`sfx.project.json.tmp` → rename；已存在旧文件先复制为 `sfx.project.json.bak`。
- 解析失败返回错误，不会静默回退 demo。
- 加载后若发生迁移/clamp/重名，UI 状态栏会显示警告。

## 2. Godot 导出

### `audio/sfx/*.tres`

- 文件名为 `sanitize(patch.name)` 并全局去重（`coin`, `coin_2`, `sound`, `sound_2`）。
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
  "schema_version": 3,
  "project": "my_game",
  "sounds": [
    {
      "name": "coin",
      "patch": "Coin",
      "category": "UI",
      "favorite": true,
      "dsp_version": 2,
      "wav": "res://audio/baked/coin.wav",
      "rate": 22050,
      "duration": 0.11,
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
