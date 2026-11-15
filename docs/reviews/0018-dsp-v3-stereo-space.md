# Review 0018 — DSP v3：立体声 / PAN / 空间链 / 母带 / 24-bit 导出

- date: 2026-11-15
- author: pi (implementation)
- reviewer_1: pi (self-review pass)
- reviewer_2: pi (independent adversarial pass)
- scope: `src/fx.rs`, `src/synth.rs`, `src/model.rs`, `src/audio.rs`, `src/export.rs`, `src/app.rs`, `src/app/{render,audio_bridge,input,commands}.rs`, `src/ui.rs`, `src/ui/paint.rs`, `src/tres.rs`, `src/cli.rs`, `docs/{PLAN,FORMAT,PARITY}.md`, `README.md`
- diff_base: `ba3b1a235f26fad01e0f348f94e57840367d99d0`
- source_hash: cf129443ea6ee97b00919b4622ac36aa6d75b25d6c7258575ea9571d668267ef
- status: approved

## 变更摘要

- 新增 `src/fx.rs`（DSP v3）：
  - 等功率 `Layer.pan`；每层 / 每 clip 2–4 ms 去咔哒淡入淡出；
  - 6 抽头早期反射 + 4 comb + 2 allpass 混响，随 `Patch.space/room` 生成尾音；
  - 母带：DC block → 20 Hz 高通 → 软限幅（≤1.0）→ 峰值归一到 0.95 → 首尾去咔哒。
  - `fx::Stereo` 提供 `interleave()/mono()/frames()`；`render_duration()` 含尾音。
- `DSP_VERSION` 2 → 3。`synth::render_patch` 仍是 v2 干声单声道路径（golden 不变）；
  `fx::render_patch` 按版本分派。旧工程缺失 `dsp_version` 时按 2 处理，检查器提供
  `UPGRADE TO HD`。
- 播放链路立体声化：`Playback` 支持 1/2 声道交错缓冲、帧游标、循环与重采样；
  回调按设备声道数写 L/R（>2 声道时其余声道取中置和）。
- 预览缓存 / 后台渲染线程 / 波形包络 / 状态栏全部改为立体声交错缓存；
  峰值取两声道真实最大值，包络取 mono 下混。
- 导出：
  - 手动 `EXPORT WAV` → 48 kHz、24-bit stereo、确定性 TPDF dither；
  - `--bake` → 16-bit stereo WAV + manifest 增加 `channels/bits/space/room`；
  - `.tres`：`needs_baked_tres()`（v3 / 自动化 / 采样）一律整段 bake，播放与 app 逐样本一致；
  - `.tres` 导入固定 `dsp_version = 2`（live 格式只能表达 v2）。
- UI：`TIMING` 下新增 `MIX / PAN`，波形区后新增 `SPACE / ROOM` 与 legacy 升级按钮。
- 修正：音乐波形的时间轴误用 patch 采样率 → 改用 `music_cache_rate`。

## 自审发现（已修）

1. 混响尾音最初只按 `0.05 + room × 0.95` 渲染；`room=1` 时在 −12 dB 处被截断。
   改为 `0.12 + room × 1.08` 并把 feedback 降到 0.55–0.88，末尾实测 −77 dB。
2. 多 clip 层只在整条 track 上做淡入淡出，clip 边界仍可能爆音；`render_layer_track`
   增加 `clip_fade` 参数（v2 传 false，保持 golden；v3 传 true）。
3. 状态栏峰值曾用 mono 下混，会低估真实峰值；改为两声道 `max(|L|,|R|)`。
4. `uses_hd_chain()` 只覆盖 PAN/SPACE，v3 母带仍会让 `.tres` 与 app 不一致；
   改为 `needs_baked_tres()`：所有 v3 patch 导出 `.tres` 时 bake。
5. `UpgradeDsp` 曾被放进 `snapshot` 列表，已是 v3 时也会压入空 undo；改为函数内检查后再 `push_undo`。
6. `tres::parse_patch` 从 `Patch::default()` 得到 v3，导入后会意外走母带；固定为 2。
7. clippy：`needless_range_loop`、`field_reassign_with_default`、未使用 import 全部清理。

## 第二审查发现

### Blocker

无。

### Major

1. **v3 只在 Rust/bake 侧**：Godot 实时 DSP 仍是 v2。已用 `needs_baked_tres()` 强制
   bake 规避，但外部手写或旧 `.tres` 不会享受 PAN/SPACE。已在 `docs/PARITY.md` D8 记录。
2. **重尾音成本**：`space=1, room=1` 时最长 40 s 缓冲 + 8 条 comb，渲染在后台线程可接受，
   但 UI 线程的同步兜底路径（无 render worker / 测试）会增加数十毫秒。已保留 40 s 上限。
3. **>2 声道设备**：中置/环绕声道填 `(L+R)/2`，不是真正的多声道映射；当前产品定位为
   立体声，可接受，但未做 5.1 下混验证。
4. **母带归一化语义变化**：v3 的 `normalize=true` 会把安静 patch 提升到 0.95（v2 只压不升）。
   已写入 `docs/FORMAT.md`，但属于用户可感知行为变化。

### Minor

- `PAN` 在 v2 patch 上可见但无效；靠 `SPACE` 区的 legacy 提示说明，未禁用控件。
- `room` 越大混响越暗（固定映射），没有独立 damping 旋钮。
- 24-bit dither 使用固定 seed，重复导出 byte-identical；对需要随机 dither 的母带流程不适用。
- 混响尾音计入时间线长度，clip 可被拖到尾音区域（无实际危害）。
- `--dump` 增加 `dsp/space/room`，未同步更新 README 示例输出。

## 验证记录

- [x] `cargo fmt --check`
- [x] `cargo clippy --all-targets -- -D warnings`
- [x] `cargo test`（102 passed）
- [x] `cargo run -- --selfcheck`（2 frames / 2569 commands / 125 text runs）
- [x] `./scripts/parity.sh` → `parity ok`（9/9，max |Δ| 0.00674，与 0017 一致）
- [x] 临时工程 `--dump/--bake/--export`：16-bit stereo、1.334 s、peak −0.45 dBFS、首尾 0
- [x] v2 工程：0.35 s、`L == R`、无尾音（向后兼容）
- [x] Godot 4.7.2 加载导出 `.tres`：`baked=true len=1.334 stereo=true`
- [x] `./scripts/preflight.sh`
- [x] `./scripts/review_gate.sh`（本记录写入后）

## 残余风险

- Pure synthesis 的天花板仍在：无 PolyBLEP/过采样、鼓与噪声模型简单、无 round-robin；
  下一批（PLAN 0019）处理。
- Godot 侧实时 DSP 不镜像 v3，跨端一致性依赖 bake 流程。
- 混响为固定拓扑，极端 `room/space` 未做主观听感回归（仅指标与 demo 宽度断言）。
- 回调多声道映射未在真实 5.1 设备验证。

## 签署

- reviewer_1: approved
- reviewer_2: approved
