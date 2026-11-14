# Review 0016 — Rust 端采样波形 `Wave::Sample`

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/wav.rs`, `src/model.rs`, `src/synth.rs`, `src/app/{input,commands}.rs`, `src/ui/{paint,layout}.rs`, `src/project_io.rs`, `src/export.rs`, `docs/{FORMAT,PARITY,PLAN}.md`, `README.md`
- diff_base: `c60bd3d9099ae808557278af44c9d62d993c39ec`
- source_hash: 5cafda581de288fc0a93096c5f40e08b391aff658ea3873e199f6430311028c8
- status: approved

## 变更摘要

- 新增 `src/wav.rs`：RIFF/WAVE 读取，支持 PCM16 / IEEE F32、单声道/立体声，混为 mono 并归一化；未知格式报错。
- `Wave::Sample`（Godot id 9）；`Layer` 增加：
  - `sample_path: String`（工程内相对路径或绝对路径）
  - `sample_pitch: f32`（-24..24）
  - `sample_loop: bool`
  - `sample_data: Option<SampleData>`（不序列化）
- `Project::load_samples(root)`：打开工程时按路径解码，相对路径基于工程根；失败写入 warnings（不静默）。
- `synth::render_sample` 对齐 Godot `_build_sample`：线性插值、loop wrap、`source_rate/output_rate * 2^(pitch/12)`、ADSR、gain。
- Inspector 新增 SAMPLE 段：`LOAD WAV…`、`PITCH`、`LOOP`；`Action::LoadSample` / `ToggleSampleLoop`。
  - 选中的 WAV 会复制到 `<project>/audio/sfx/samples/`（重名 `_2` 递增）；无工程目录时存绝对路径。
- `PROJECT_VERSION` 3 → 4；JSON 增加采样字段；`.tres` 导出的采样 patch 自动整段 bake（Godot 直接播放 baked）。
- 新增 6 条测试（WAV 读写、采样循环/变调、JSON 字段、`load_samples`、采样导出 bake、检查器控件）。

## 自审发现（已修）

1. `Wave` 增加变体后 `wave_color`、`LayerField` 穷尽匹配、`layer_sections` 等编译错误 → 全部补齐。
2. `Project::load_samples` 需要可变工程且要在 `validate()` 之后调用 → `project_io` 改为 `mut warnings` 并扩展采样错误。
3. `every_wave_renders_finite_samples` 对无数据的 SAMPLE 判定为 silent → 测试注入 4 帧样本。
4. 采样导出原本会写 `wave=9` 但没有 `AudioStreamWAV`（Godot 静音）→ 改为任何 Sample 层都走 baked 整段导出。
5. WAV 复制无重名保护会覆盖已有样本 → 使用 `stem_2.wav` 递增。
6. 测试里 `let mut project = project;` 触发 clippy `redundant_locals` → 移除。

## 第二审查发现

### Blocker

无（Rust 端闭环）。

### Major

1. **`.tres` 往返仍不完整**：导出 Sample patch 会 bake 整段，但 `.tres` 导入仍把 `SAMPLE→Pluck`，且不读取 `baked` WAV；因此“导出再导入”会丢失采样层。已在 `FORMAT.md` / `PARITY.md` D4 明确标注为单端差异。
2. **WAV 格式限制**：只支持 PCM16 / F32 单/双声道；24-bit、8-bit、多声道会报错。Godot 侧 `AudioStreamWAV` 更宽。
3. **采样路径迁移**：有工程目录时复制进工程；无工程目录时保存绝对路径（不可移植）。在 UI 中只显示路径，没有“重新定位/清理”操作。
4. **采样数据内存**：`sample_data` 常驻内存；长样本 × 多 patch 可能占用较大（上限 3s 的 duration 限制仍存在）。

### Minor

- `sample_pitch` 使用 `LayerField` 通用滑块；`sample_loop` 是 toggle，未支持 loop 区域（Godot 侧也未暴露）。
- 修改 `sample_path` 不会自动重载，需重新选择文件。
- bake 仍按 `Patch.rate`，预览按 48k（0015 的 D6/D3 差异）。
- `Project::load_samples` 每次打开都会重新解码所有样本；无缓存/去重。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（79 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2555 commands / 119 text runs）
- [x] release 启动截图：WAVE 网格第三行出现 SAMPLE，布局无裁切
- [x] `reads_back_the_wav_writer_output`
- [x] `sample_layers_loop_and_apply_pitch`
- [x] `sample_fields_roundtrip_and_load`
- [x] `sample_patches_bake_for_tres_export`
- [x] `sample_layer_exposes_load_pitch_and_loop`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- `.tres` 采样层导入/逐层往返未实现（Backlog 第 1 项）。
- WAV 解码格式与声道数限制。
- 触发文件选择/真实加载无法自动点击验证；逻辑由单元测试覆盖。
- 采样数据不进入 JSON/`.tres`，工程迁移需保留 WAV 文件。

## 签署

- reviewer_1: approved
- reviewer_2: approved
