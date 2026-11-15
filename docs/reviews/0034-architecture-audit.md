# Review 0034 — 架构审查：交互一致性、功能裁剪与扩展基线

- date: 2026-11-16
- author: pi (architecture audit)
- reviewer_1: pi (self review)
- reviewer_2: pending（本记录是重构输入，不是 approved 的代码评审）
- scope: `src/app*`, `src/ui*`, `src/model.rs`, `src/library.rs`, `src/export.rs`, `src/audio.rs`, `src/presets.rs`, `src/music_bank.rs`, `docs/*`
- diff_base: `142559d` (add unified library with audio presets and music files)
- source_hash: `eedd5a34961c1f5180949164d1adc5565dcad819ce17609de470c7201f132249`
- status: draft

## 0. 结论（TL;DR）

工程基线是健康的：`cargo test` 174 passed，`--selfcheck` 2 frames / 3949 commands /
191 text runs，导出/parity/审查门禁都在。**问题不在“有没有测试”，而在
“新增能力时没有统一的状态与命令边界”**。

本轮审查得到四个结论：

1. **交互问题的根因是焦点（focus）有 3–4 个真相源**：
   `selected_library`、`selected_preset`、`library_mode`、`tab`、以及
   `selected_patch/selected_layer/selected_clip` 各自独立变化，导致右侧检查器、
   Space、菜单可用性、导出目标互相矛盾。可稳定复现 5 个 P0 级交互缺陷。
2. **命令语义分散在 5 个地方**：`Action` 枚举、`activate` 的 match、
   `action_available` 的 `matches!`、`input.rs` 的快捷键 match、`activate` 里的
   `snapshot` 白名单。每加一个命令要改 5 处，且已经出现 undo 双快照、空快照、
   只读判定错杀导出等偏差。
3. **模型里“文档、资产、播放、排列”混在一起**：`Patch` 同时是音色和排列，
   `Project.music` 是“当前唯一音乐”，资产库用索引身份，播放是“整段离线渲染 +
   单缓冲播放”。这决定了下一步扩展（多轨编排、音色复用、实时试听、资产生命周期）
   会持续撞墙。
4. **功能裁剪的空间明确**：`LibraryMode`/`selected_preset`/`patch_scroll`/
   `preset_scroll` 是统一 Library 之前的遗留；`DEMO TRACKS` 与 `music/*.json`、
   `builtin` 预设与 demo 预设、CLI demo 与 GUI 是重复入口；`Layer.enabled`、
   `Patch.normalize`、ARP 16 槽等能力存在但 UI 没有接线。

同时发现一个**独立的导出元数据 bug**：v3 工程以 48 kHz 烘焙，但 manifest
`duration` 仍除以 legacy `patch.rate`（通常 22050），实测 `pickup_coin`
manifest 0.715 s，WAV 实际 0.328 s。

建议按“先修交互不变量，再抽状态/命令，再做性能与功能裁剪，最后才谈 v5 排列模型”
的顺序推进；不要先大改 schema。

---

## 1. 现状测量

| 指标 | 现状 |
|---|---|
| Rust 源码 | ~21.4k 行 |
| `ui::Action` | 46 个变体 |
| `ui::Hit` | 25 个变体 |
| `app::Drag` | 13 个变体 |
| `LibraryKey::` 使用点 | 55 处，分布在 7 个文件 |
| `LibraryMode` 使用点 | 54 处 |
| `selected_preset` 使用点 | 30 处 |
| 每帧 `Layout::new` | 至少 2 次（输入 1 次 + 绘制 1 次）；每次都会重建库行、检查器行 |
| 每帧 `all_library_entries` | 2–4 次（计数、空态、`selected_library_entry`） |

当前数据流：

```
InputEvent
  └─ SfxLogic::event
       ├─ Layout::new(state)            // 全量：库行 + inspector RawItem
       ├─ 手写 if/else 处理 overlay（menu/kind/category）
       ├─ hit() 线性扫描
       └─ State 方法直接改 doc/view/playback
            └─ mark_dirty / begin_edit / commit_edit
                 └─ 整 Project clone + 深比较（含 SampleData PCM）
paint
  └─ Layout::new(state) 再算一次
       └─ visible_library_entries()/all_library_entries() 再分配一遍
```

这套结构在 500 行规模时是高效的；在 46 个命令、4 类资产、3 套播放来源、
170 个测试时，边界开始互相污染。

---

## 2. 交互缺陷（带复现与证据）

### P0-1 焦点多真相源：右栏、菜单、Space 各看各的

**证据**：

- `inspector_items` 的优先级：Audio → `library_mode == Presets` → `tab`
  （`src/ui/paint.rs:1764-1770`）。
- 点击图层/时间线只调用 `select_layer` / `select_clip`，不会清掉
  `selected_library` / `library_mode`（`src/app/input.rs:414-460`，
  `src/app.rs:833-853`）。
- `toggle_play` 先看 Audio，再看 Music，再看 `library_mode == Presets`，
  最后才看 tab（`src/app/audio_bridge.rs:9-45`）。

**复现**：

1. 打开有 AUDIO 的工程 → 点一条 AUDIO 行 → 点时间线里的某一层。
   - 层被选中、时间线可继续拖拽；
   - 右侧仍显示 `AUDIO ASSET`，不是该层参数；
   - `MENU → BAKE GODOT` / `EXPORT .TRES` / `EXPORT WAV` 全被禁用。
2. 选一个 PRESET → 点 `MUSIC` tab。
   - 中间变成 pattern grid；
   - 右侧仍是 `PRESET LIBRARY`；
   - Space 仍试听预设，不播放音乐。
3. 选一个 PRESET → 在搜索框输入把它过滤掉。
   - `refresh_filter` 清掉 `selected_library`/`selected_preset`，但**不清
     `library_mode`**（`src/app/commands.rs:1308-1326`）；
   - 右侧变成空的 `PRESET LIBRARY · SELECT A PRESET`；
   - Bake/Export 仍按 PRESETS 只读规则被禁用，形成“幽灵只读模式”。

**根因**：把“当前列表选中项”“当前文档对象”“当前面板”和“只读边界”混成了
四份可变状态。正确模型是**一个 `Focus`**，其余全部派生。

---

### P0-2 只读边界按“资产选择”判定，误杀了文档级命令

**证据**：`action_available`（`src/ui.rs:425-472`）。

```rust
let focused_read_only = matches!(
    state.selected_library_kind(),
    Some(LibraryKind::Audio | LibraryKind::Music)
);
if state.view.library_mode != LibraryMode::Presets && !focused_read_only {
    return true;
}
// 以下命令全部被禁用：
// Bake / Tres / ExportWav / SavePreset / Randomize / Vary / Favorite /
// AddLayer / DuplicateLayer / DeleteLayer / AddClip / DeleteClip /
// LoadSample / UpgradeDsp / NewPatch / RenamePatch / ...
```

**问题**：

- `BAKE GODOT`、`EXPORT .TRES` 是**工程级**命令，不该因为左栏选了一个 WAV 就禁用。
- `SAVE AS PRESET`、`RANDOMIZE`、`VARY` 作用于当前 SOUND patch，也不该禁用。
- 但同一时刻，鼠标仍可直接拖参数、拖 Clip、点层级（这些路径不查
  `action_available`），于是出现“键盘/菜单说只读，鼠标却能改工程”的分裂。
- `Key::Delete` 在“已选中自动化点”时会直接删点并 `push_undo`，完全绕过
  `can_run`（`src/app/input.rs:100-119`）；ASSET 焦点下按 Delete 仍可能改工程。

**修复方向**：命令声明 `Scope`（Document / FocusedAsset / Transport），
只对 `FocusedAsset` 类命令做只读判定；鼠标手势和菜单走同一份 `can_run`。

---

### P0-3 Tab、Space、PLAY 的语义互不一致

**证据**：

- `A::TabPatches` / `A::TabMusic` 只改 `tab` 和滚动，不清焦点、不停止/重定向播放
  （`src/app/commands.rs:88-101`）。
- `toggle_play` 无视 tab，只看资产焦点（`src/app/audio_bridge.rs:9-45`）。
- 头部 `PLAY` 按钮直接 `self.play()`，按当前 tab 播放（`src/app/commands.rs:117`），
  与 Space 的“先看资产焦点”不是同一套逻辑。

**后果**：

- 选中 AUDIO 后，Space 试听 WAV，而头部 PLAY 播放工程 patch；
- 选中 MUSIC 行后，Space 播放 `Project.music`（不一定是你选的那首），
  即使你已经切到 SOUND tab；
- 播放中切换 patch/tab/undo，旧 buffer 继续出声，UI 播放头却按新文档走
  （`select_patch` 只重置 playhead，不停止 transport，`src/app.rs:805-831`）。

**修复方向**：引入单一 `Transport { source, position, state }`；
选择变化不隐式改 source；切文档时显式 `stop` 或显式 reload；
Space/PLAY 都调用同一个 `play_source(TransportTarget)`。

---

### P0-4 “改变筛选”会修改文档

**证据**：`pick_kind(Some(Music))` 在选中项不可见时会对首个可见项调用
`activate_library_row`（`src/app/commands.rs:1263-1274`），而 Music 的 activate
是 `load_music`（`src/app/commands.rs:892-942`），会替换 `Project.music` 并入 undo。

**复现**：打开有 `music/*.json` 的工程 → 类型下拉选 MUSIC → 当前音乐被自动替换。
虽然是可撤销的，但“筛选”是纯视图操作，不应该产生文档修改。

**修复方向**：筛选只改可见集并清空/保留选中；激活必须显式（点击、Enter 或
`OPEN` 命令）。同理，方向键浏览不应切 tab / 载入 music。

---

### P1-5 Undo 语义已出现分叉

**证据**：`activate` 用一份手写白名单决定是否 `push_undo`
（`src/app/commands.rs:40-70`）。

已确认的异常：

1. **LoadSample 双快照**：`A::LoadSample` 在白名单里，`activate` 先
   `push_undo()`；成功载入后 `load_sample` 又调用一次 `push_undo()`
   （`src/app/commands.rs:662-704`）。撤销一次回到载入前，再撤销一次是空操作。
2. **LoadSample 空快照**：对话框取消时，白名单里的 `push_undo` 已经发生，
   产生一条什么都不会撤销的记录。
3. **Favorite 两条路径不一致**：菜单/快捷键 `A::Favorite` 不在白名单，
   不可撤销（`src/app/commands.rs:787-803`）；点星标走 `toggle_patch_favorite`，
   `begin_edit` + `commit_edit`，可撤销（`src/app/commands.rs:1183-1204`）。
4. **标签退化**：`DemoTrack`、`Randomize` 等都落到通用 `"edit"`，
   而 `load_music` 用 `"load music"`，同类操作标签不同。

**根因**：undo 策略是命令调用点的局部知识，不是命令元数据。

**修复方向**：`UndoPolicy` 进入命令表；提供 `App::edit(label, closure)` 事务，
仅在文档 revision 真正变化时压栈；删除手写 `snapshot` 白名单。

---

### P1-6 文档对象用稳定 ID，资产库却用索引身份

**证据**：

- Patch/Layer/Clip 已有 `id`（review 0012/0013）。
- `LibraryKey::{Audio,Preset,Music}(usize)` 是索引（`src/library.rs:71-80`）。
- `PresetBank::load_project` / `MusicBank::load_project` / `AudioBank::load_project`
  都会整体重建 Vec（`src/presets.rs:46`、`src/music_bank.rs:42`、
  `src/library.rs:166`）。

**风险**：

- 删除/重命名/外部新增文件后重扫，索引会指向另一条资产；
- `clamp_selection` 只校验 `get(index).is_some()`，无法发现“同 index 不同文件”
  （`src/app.rs:970-1008`）；
- 只有 `load_sample` 手工按 path 找回 AUDIO 选中项，其他路径没有。

**修复方向**：资产也引入稳定 `AssetId`；重扫按相对路径保留 ID，
选中项保存 ID；索引只作为 `Vec` 的临时位置。

---

### P1-7 手势生命周期有悬挂路径

- `PointerLeave` 只在 `Drag::LibraryItem` 时清 drag；Clip/参数/自动化拖动会保留
  `Drag` 与 `timeline_pin`（`src/app/input.rs:546-555`）。
  如果 PointerUp 发生在窗口外，时间线比例会一直冻结，下一次 PointerMove
  还会继续拖动。
- `begin_edit` 在 pointer-down 时 clone 整个工程；若窗口失焦/关闭而没有
  PointerLeave，`pending_edit` 会跨手势残留，后续 `push_undo` 才被一起提交。
- 整 Project 的 `commit_edit` 用 `before != project`（`src/app.rs:906`），
  而 `Layer.sample_data: Option<Arc<SampleData>>` 的 `PartialEq` 会逐样本比较
  PCM（`src/wav.rs:15`）；载入采样后每次松手都可能扫描最多 288 万样本。

**修复方向**：统一的 gesture/transaction 状态机；用 `doc.revision` 判定变化；
窗口失焦时取消 pending gesture；`Arc` 只比较 `ptr_eq` 或忽略 PCM。

---

### Major-8 导出元数据与烘焙采样率不一致

**证据**：`src/export.rs:42-96`

```rust
let rate = patch.bake_rate();                 // v3 = 48000
let stereo = fx::render_patch(patch, rate);
...
"rate": rate,
"duration": stereo.frames() as f32 / patch.rate as f32,  // 旧字段，可能 22050
```

`patch_tres_inner` 也写 `rate = patch.rate`（`src/export.rs:199`），
即使 `baked` 指向 48 kHz WAV。

**实测**（`--demo` 临时工程）：

| patch | manifest rate | manifest duration | WAV 实际 |
|---|---|---|---|
| pickup_coin | 48000 | 0.715 s | 0.328 s |
| explosion_burst | 48000 | 3.064 s | 1.407 s |
| power_up | 48000 | 1.878 s | 0.863 s |

根因是 `Patch.rate`（legacy）与 `Patch::bake_rate()`（派生）双真相。
消费方若按 manifest duration 安排循环/裁剪，会得到 2.18× 的错误长度。

**修复方向**：所有导出元数据只从“实际写入 WAV 的 rate”派生；`Patch.rate`
降级为 v2 迁移字段，不再参与 v3 导出计算。`.tres` 也写 `bake_rate()`。

---

### P2-9 每帧/每指针事件全量重建

`Layout::new` 在 `event()`（`src/app/input.rs:57`）和 `paint()`
（`src/ui/paint.rs:7`）各调用一次，内部：

- `state.visible_library_rows()` → `all_library_entries()`：为每条资产 clone
  name、format detail、建 HashMap、排序；
- `inspector_items(state)`：构造整份 `Vec<RawItem>`，含大量 `String`；
- `paint` 内又多次调用 `visible_library_entries()`（计数、空态）和
  `selected_library_entry()`。

在 2000 条 AUDIO/PRESET 时，每个鼠标移动帧会做数千次分配。功能上还能用，
但它把“资产规模”与“交互延迟”直接绑死，是下一步接资产监听/大批量导入的硬限制。

**修复方向**：`Layout` 每帧只算一次，按
`(viewport, view_version, doc_version, asset_epoch)` 缓存；库行/检查器行
拆成 `present` 层，按版本缓存。

---

## 3. 扩展瓶颈（不是 bug，但决定下一步能走多远）

1. **`Patch` = 音色 + 排列**。`Layer.clips` 让“一个音色”同时携带时间线位置；
   复制音色会复制编排，音色复用只能复制。想支持“同一 sound 在多处引用”、
   多轨、多 music 同时编排，需要 v5 的 `Sound` / `Arrangement` 分离。
2. **`Project.music` 是单份当前音乐**。music 文件库只是载入器，
   没有“工程级曲目列表/多曲编排”。
3. **Wave 没有注册表**。加一个波形要改
   `Wave::ALL`/`label`/`desc`/`is_tone`/`is_drum`/`godot_id`/`from_godot_id`/
   `layer_sections`/`paint::wave_color`/`synth` 两套 render/`.tres`/测试。
   `Wave::Curve` 与 `Wave::Pluck` 的 Godot id 都是 8，更是隐式别名。
4. **`Action` 是全局大枚举**，没有命令元数据（作用域、undo、快捷键、可用原因），
   菜单、按钮、快捷键、tooltip 各自解释同一份语义。
5. **Overlay 没有栈**。main menu、kind menu、category menu 在 `input.rs` 里
   三段近乎重复的打开/关闭/穿透逻辑；main menu 外部点击会吞掉点击，
   两个下拉的外部点击会继续落到下层控件——行为不一致。
6. **没有手势级集成测试**。现有 170+ 测试主要是状态函数和单控件命中，
   所以 P0-1/P0-3/P0-4 这类“组合语义”无法被门禁发现。

---

## 4. 目标架构（务实版，不要求一次性重写）

### 4.1 分层

```
domain/       纯数据 + 规则：Project/Sound/Layer/Clip/Music、FieldSpec、WaveDef
assets/       资产索引与文件生命周期：Audio/Preset/Music，稳定 AssetId
app/          Focus、Command、Transaction、Transport、Effects
ui/           Geometry（纯矩形）+ Present（行数据）+ Overlay + Paint
platform/     cpal、rfd、文件 I/O（由 Effects 驱动）
```

依赖方向只允许 `ui -> app -> domain/assets`，`platform` 只被 app 调用。
`ui` 不直接改文档，只发 `Command`。

### 4.2 单一 Focus

```rust
pub enum Focus {
    Sound { patch: u32, layer: Option<u32>, clip: Option<u32>, point: Option<u32> },
    Audio { asset: AssetId },
    Preset { asset: AssetId },
    Music { asset: AssetId },
}

impl Focus {
    fn kind(&self) -> LibraryKind;
    fn is_document_object(&self) -> bool;
}
```

规则：

- 点击库行设置对应 `Focus`；
- 点击时间线/图层/Clip/自动化/参数把 `Focus` 切回 `Focus::Sound`；
- `tab` 是用户视图偏好，但 `Tab` 切换必须把不兼容的 `Focus` 归位
  （例如切到 MUSIC 时 `Focus::Music(current)` 或清空资产焦点）；
- 右侧面板标题、只读提示、导出目标、Space 目标全部从 `Focus` 派生；
- 删除 `LibraryMode`、`selected_preset`、`patch_scroll`、`preset_scroll`、
  `visible_patches`、`visible_presets` 的语义重复。

### 4.3 Command + Transaction

```rust
pub enum Scope { Document, FocusedAsset, Transport, FileSystem }

pub struct CommandMeta {
    pub label: &'static str,
    pub group: Group,
    pub scope: Scope,
    pub undo: UndoPolicy,          // None | Edit(&'static str) | External
    pub shortcut: Option<Shortcut>,
}

pub fn meta(cmd: &Cmd) -> CommandMeta;
pub fn can_run(app: &App, cmd: &Cmd) -> Result<(), DisabledReason>;
pub fn run(app: &mut App, cmd: Cmd, io: &mut Effects);
```

- 菜单、按钮、快捷键、tooltip、`--selfcheck` 都消费同一张表；
- 只读只作用于 `Scope::FocusedAsset`；
- 所有文档变更走 `app.edit(label, |project| Changed/Unchanged)`；
  `UndoPolicy` 不再由调用方临时决定；
- I/O（打开对话框、保存、bake）变成 `Effects`，便于 CLI/GUI 复用和测试。

### 4.4 Geometry / Present / Overlay

```rust
struct FrameKey { viewport: Size, view: u64, doc: u64, assets: u64 }
struct Layout { geometry: Geometry, rows: LibraryRows, inspector: InspectorRows }
struct OverlayStack(Vec<Overlay>);
struct HitLayer(Vec<(Rect, Hit)>); // 按绘制顺序，hit 反向
```

- `Layout` 每帧算一次，输入与绘制共用；
- `Present` 只负责“把状态变成行”，按版本缓存；
- overlay 统一入栈，Esc/外部点击只关闭栈顶，不穿透；
- `hit()` 从顶层向下走，不再手写菜单优先级。

### 4.5 资产索引与稳定身份

```rust
pub struct AssetId(u32);

pub struct AssetIndex<T> {
    by_id: HashMap<AssetId, T>,
    order: Vec<AssetId>,
    path_to_id: HashMap<PathBuf, AssetId>,
    epoch: u64,
}
```

重扫规则：同路径复用 ID；新路径发新 ID；消失的 ID 标记 retired，
`clamp_focus` 在下一帧把焦点清空并 toast 原因。`LibraryKey` 改为
`Sound(PatchId) | Audio(AssetId) | Preset(AssetId) | Music(AssetId)`。

### 4.6 Transport

```rust
pub enum Source { Patch(u32), Music(AssetId), Audio(AssetId) }

pub struct Transport {
    source: Option<Source>,
    state: TransportState,
    position: f32,
    looped: bool,
}
```

- 只有显式 `play_source` 改 `source`；
- 选择/筛选/切 tab 不改 source；
- 文档变更时按策略 `stop`（默认）或标记 stale（未来实时引擎）；
- 头部 PLAY、Space、菜单共用同一入口；
- 音频引擎接收 `Arc<[f32]>` 或 `Arc<Vec<f32>>`，避免每次播放 clone 整段 buffer。

### 4.7 领域 v5（目标，不在本阶段）

```
Sound       { layers, dsp_version, space, room, ... }      // 只有音色
Arrangement { clips: Vec<ClipInstance { sound: SoundId, start, gain, pitch }> }
Scene       { sounds: Vec<Sound>, arrangements, music_tracks }
```

先分离“音色”和“排列”，才能自然支持：一个 sound 多处引用、多轨、多曲、
按轨道 solo/mute、把 preset 当成真正的声音资产而不是一整段 Patch。

### 4.8 Wave 注册表

```rust
pub struct WaveDef {
    pub wave: Wave,
    pub label: &'static str,
    pub desc: &'static str,
    pub family: Family,
    pub godot_id: Option<u32>,     // Curve 显式 None 或 documented alias
    pub color: Color,
    pub sections: &'static [LayerSectionSpec],
}
```

`Wave::ALL`、`label/desc/is_tone/is_drum/godot_id/layer_sections/wave_color`
都从表查；新增波形只改“表 + DSP + 一个测试”。Godot id 唯一性测试会把
Curve/Pluck 的隐式别名变成显式契约。

---

## 5. 功能裁剪 / 孤立能力对接

### 5.1 裁员（删除或合并）

| 对象 | 现状 | 决策 | 理由 |
|---|---|---|---|
| `LibraryMode` | Project/Presets 双模式遗留 | **删** | 统一 Library 之后只剩只读判定与状态污染 |
| `selected_preset` | 与 `selected_library` 重复 | **删** | 焦点统一后由 `Focus::Preset` 派生 |
| `patch_scroll` / `preset_scroll` | 统一列表后的死滚动 | **删** | 只保留 `library_scroll` |
| `visible_patches` / `visible_presets` | 与 `library_entry_matches` 重复过滤 | **删/合并** | 两套搜索/筛选逻辑已开始分叉 |
| `AudioBank.root` | 只写不读 | **删** | 用 `AssetIndex` 的根路径 |
| `DEMO TRACKS` inspector 段 | 与 `music/*.json` 重复 | **合并** | 改成一次性 `IMPORT DEMO PACK`，播放源只认 music 文件 |
| builtin 11 预设 + hand-tuned 11 预设 | 两套 seed 语义 | **合并** | 新工程只保留一套 starter pack；builtin 变成“生成器命令” |
| `--demo-reel` / `--demo-track` / `--list-presets` | GUI 不可见的工具面 | **保留但工具化** | 移到 `scripts/` 或文档的 tooling 段，不再当作产品能力 |
| `Patch.rate` 作为 UI/导出概念 | 与 `bake_rate()` 双真相 | **内部化** | 只在 v2 迁移与 legacy `.tres` 使用 |

### 5.2 接线（能力已存在，但 UI/流程没接）

| 能力 | 现状 | 建议 |
|---|---|---|
| `Layer.enabled` | DSP 已尊重，`paint` 只做灰显 | TIMING/MIX 增加 `MUTE`；时间线行点击可静音；后续再加 `SOLO` |
| `Patch.normalize` | 影响 v3 导出，但无 UI | SPACE 段增加 `NORMALIZE` 开关，或明确固定并删除字段语义 |
| `Music.normalize` | 同上 | MIX 段增加开关或固定 |
| `arp_freqs` 最多 16 槽 | inspector 只有 ARP 0–3 | 要么收敛模型到 4，要么提供动态列表编辑器；不要保持“数据能存、UI 只能看 4 个” |
| 自动化点 | 只能 Delete 键删除，无数值编辑 | 增加 `DEL POINT`、`VALUE` 显示/输入；选中点与 clip 焦点统一 |
| AUDIO 文件 | 只能试听/拖入 | 明确只读，或补 rename/delete/import（进 `.trash`） |
| MUSIC 文件 | 只能载入 | 补 `SAVE AS MUSIC` / rename / delete；否则在 inspector 明确“文件由外部管理” |
| PRESET `.tres` | 不支持 | 明确不在范围，或补导出 |
| `Wave::Sample` 按钮 | 可直接切换成空采样（静音） | 切换时强制走 `LOAD WAV…`，取消则回退原波形 |
| `Audio` 试听 | UI 线程同步解码/重采样 | 走后台 worker（与 preview 同一套 generation/cancel） |

### 5.3 明确不做（本阶段非目标）

- 不重写 DSP，不改变 v2 golden。
- 不一次上 v5 排列模型；先把 focus/command/transport 边界做对。
- 不引入完整 retained UI 框架；只做 overlay 栈 + 命中层 + 缓存。
- 不做 plugin 化/动态加载；当前 Rust 单二进制是优势。

---

## 6. 迁移路线（每一步都可独立合入）

### P0 — 交互不变量修复（1–2 天，不改 schema）

目标：让现有功能自洽，先止血。

1. **单一焦点归位**：点击 layer/clip/automation/param → 清 AUDIO/PRESET 焦点；
   点击 AUDIO/PRESET/MUSIC 行 → 设置对应焦点；tab 切换清不兼容焦点。
   涉及：`input.rs` 的 `Hit::LayerRow/Timeline/AutomationPoint/LayerField/...`、
   `Action::TabPatches/TabMusic`、`select_layer/select_clip`。
2. **只读 scope 化**：`Bake/Tres/Save/SavePreset/Randomize/Vary/AddLayer/...`
   按命令 scope 判定；`AUDIO` 选中只禁用“作用于该 AUDIO 的命令”。
3. **Space/PLAY 统一**：抽 `play_focused()` / `play_tab()`；明确 Space 在
   资产焦点与文档焦点下的语义；切 patch/tab/undo 时停止旧 transport。
4. **筛选不改文档**：移除 `pick_kind` 中的 `activate_library_row`；
   music 只有显式点击/Enter 才 load。
5. **undo 止血**：去掉 `LoadSample` 的白名单 push（交给 `edit` 事务）；
   取消对话框不产生快照；favorite 两条路径统一。
6. **导出元数据**：manifest duration 用实际 `rate`；`.tres` 写 `bake_rate()`；
   加 round-trip 测试。

验收测试（新增，纯状态级即可）：

- `selecting_audio_then_clicking_a_layer_restores_the_layer_inspector`
- `tabs_reclaim_focus_from_preset_and_audio`
- `asset_focus_does_not_disable_document_export`
- `filtering_by_music_does_not_load_a_track`
- `cancelled_sample_picker_pushes_no_undo`
- `successful_sample_load_is_a_single_undo`
- `bake_manifest_duration_matches_written_wav`

### P1 — 删除遗留状态

- 删 `LibraryMode`/`selected_preset`/`patch_scroll`/`preset_scroll`；
- `LibraryKey` 引入稳定 `AssetId`；
- `clamp_selection` 只保留一套；
- `--selfcheck` 更新期望标签。

验收：`cargo test` 全绿；无 `LibraryMode` 引用；重扫后选中项仍指向同一文件。

### P2 — Command + Transaction

- `Cmd` 携带 `meta/can_run/run`；
- 菜单/快捷键/tooltip/selfcheck 统一；
- `App::edit(label, closure)` + `doc.revision` 替代白名单与深比较；
- 删除 `activate` 里的 snapshot 列表。

验收：新增命令只改命令表 + handler 两处；`can_run` 覆盖所有菜单项；
`no_op_commands_do_not_push_undo` 覆盖全部命令。

### P3 — Layout/Present/Overlay

- `Layout` 缓存 + `HitLayer`；
- `OverlayStack` 统一菜单/下拉/模态；
- 库行/inspector 行按版本缓存；
- 大库（1000+ 条）指针移动分配数下降一个数量级。

验收：`Layout::new` 每帧最多一次；输入路径不再构造 `RawItem`；
overlay 穿透测试。

### P4 — Transport + 音频

- `Transport` 单一来源；
- `Arc` 播放 buffer，避免整段 clone；
- patch/tab/undo 与 transport 的联动测试；
- 后续再做实时 voice 引擎（可选）。

### P5 — 模型 v5 / Wave 注册表 / 资产生命周期

- Sound 与 Arrangement 分离；
- 多 music、多轨、引用式 clip；
- `WaveDef` 表；
- AUDIO/MUSIC 文件生命周期。

---

## 7. 架构不变量（建议加入 `docs/REVIEW_CHECKLIST.md`）

1. **单一焦点**：任一时刻只有一个 `Focus`；右侧面板、Space、导出目标、
   只读提示都从它派生。
2. **命令作用域**：只读只禁用 `Scope::FocusedAsset`；Document/Transport
   命令不受资产选择影响。
3. **无副作用筛选**：筛选、搜索、滚动、方向键浏览不得修改 `Project`。
4. **身份稳定**：跨重扫/重排的选择必须用稳定 ID/路径，不用 Vec 索引。
5. **一次手势一次 undo**：取消、no-op、失败不压栈；成功操作只压一次。
6. **播放源显式**：只有 `play_*` 命令改 transport source；文档/焦点变化
   默认 stop，不允许“UI 换了、声音还是旧的”。
7. **单一派生**：导出 rate/duration 从实际写入 WAV 的参数派生；
   `Patch.rate` 不再参与 v3 计算。
8. **布局一次**：每帧最多一次完整 layout；输入与绘制共用；
   overlay 走栈，不允许穿透。
9. **测试手势语义**：跨控件组合（tab + focus + Space、筛选 + 文档、
   只读 + Delete）必须有状态级回归测试。

---

## 8. 验证基线

本记录的审查基线：

- `cargo test` — 174 passed
- `./target/release/sfx-studio --selfcheck` — 2 frames / 3949 commands / 191 text runs
- `--new` / `--demo` / `--bake` 临时工程可复现
- 导出实测：`--demo` 的 manifest duration 与 WAV 帧数不一致（见 Major-8）

本轮**没有修改源码**；本记录是重构输入，`status: draft`，不参与
`review_gate` 的 approved 判定。实际代码改动应按 P0→P5 分 PR，并各自补
`docs/reviews/NNNN-*.md`。

---

## 9. 残余风险

- P0 的焦点修复会改动多处 `select_*` 调用，容易引入“点时间线跳回 SOUND 面板”
  之类的回归；需要手势级测试。
- 删除 `LibraryMode` 会触碰 `ui.rs` 的可用性测试、`project_io` 的打开重置、
  `paint` 的状态栏；建议单独 PR，不做混合提交。
- `AssetId` 重扫保留策略要明确“文件消失”时的 UX（清空焦点 + toast），
  否则会出现悬空 id。
- v5 模型是下一阶段的事；在此之前不要为了“未来多轨”在 `Layer.clips` 上
  继续叠加语义，否则迁移成本更高。

## 签署

- reviewer_1: draft（作者自审）
- reviewer_2: pending
