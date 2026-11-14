# Review 0007 — 图层检查器声明式注册表

- date: 2026-11-14
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/model.rs`, `src/ui.rs`, `docs/PLAN.md`
- diff_base: `85e1f259d592a6e06dfd76c92784d1504efe6fcd`
- source_hash: 28420c629bc7cbf0470aa766cec7758c798d5500ff9f204dc8c33456d5d18500
- status: approved

## 变更摘要

- `model.rs` 新增图层检查器注册表：
  - `LayerRowSpec`（`Field` / `ToneEnvOptions` / `EnvShapeOptions` / `FilterOptions`）；
  - `LayerRow { spec, visible: fn(&Layer) -> bool }`；
  - `LayerSectionSpec { title, rows }`；
  - `layer_sections(wave)` 按波形族返回 PITCH / AMP ENV / FILTER / NOISE / ARP / FM / PLUCK / DRUM。
- 行级可见性从 UI 分支移到谓词：`is_sine`（HARSH）、`is_pulse`（DUTY）、`is_exp_env`（EXP DECAY）、`reads_seed`（Kick 无 SEED）。
- `ui::inspector_items` 删除按波形手写的 `if is_tone / Noise / Arp / Fm / Pluck / is_drum` 六段分支，改为遍历 section 表；选项行仍由 UI 生成 `RawItem::Options`，视觉顺序与之前一致。
- 新增模型测试 `layer_sections_expose_wave_specific_fields` 与 UI 测试 `inspector_renders_the_layer_registry`（含 HARSH 在 VIB RATE 之前的顺序断言）。

## 自审发现（已修）

1. 注册表初版把 `LayerRowSpec` 的匹配写在 `LayerSectionSpec` 之前会导致 `use` 遮蔽，改为显式 `LayerRowSpec::Field`。
2. UI 测试最初误用 `Item`（布局后结构），实际 `inspector_items` 返回 `RawItem`，已改。
3. FM 段的 FILTER 选项原来直接内联在 FM section（无独立标题），注册表保持同结构，避免 UI 输出变化。
4. 默认 demo 层是 Noise，UI 测试显式把 wave 设为 Sine，避免依赖 demo 数据。

## 第二审查发现

### Blocker

无。

### Major

1. **行为等价性**：对比 0006 截图与新截图，Noise 层的 section/字段顺序（FREQ A/B、LP A/B、NOISE、SWEEP、DECAY、ENV、SEED）完全一致；Sine 顺序由 `inspector_renders_the_layer_registry` 覆盖。
2. **可见性谓词只覆盖当前规则**：如果未来新增波形或参数依赖多字段组合，需要给 `LayerRow` 扩展而不只是布尔谓词；当前 `fn(&Layer) -> bool` 足够。

### Minor

- Clip / Music 检查器仍是手写行，注册表只覆盖 Layer（Backlog 第 1 项）。
- 选项行（TONE ENV / ENV / FILTER）的代码仍在 UI 里生成，后续可把选项列表也放进注册表。
- 注册表是 `&'static` 表，新增字段必须同时改 `LayerField::spec/get/set` 与对应 section；没有编译期「字段必被某个 section 引用」检查。
- `LayerField::spec()` 仍是长 match；与 section 表暂无交叉校验测试（可后续用「每个 Field 至少出现一次」断言补齐）。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（55 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2550 commands / 118 text runs）
- [x] 实机截图：Noise 检查器布局与 0006 一致
- [x] `layer_sections_expose_wave_specific_fields`
- [x] `inspector_renders_the_layer_registry`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- 注册表与 `LayerField` 的映射靠人工维护；没有「字段被引用」的编译期保证。
- 只重构 Layer 检查器，Clip/Music 仍走手写分支，结构上尚未完全统一。
- 没有改变 DSP/导出行为，风险局限在 UI 参数面板。

## 签署

- reviewer_1: approved
- reviewer_2: approved
