# SFX Studio · read-only Godot plugin

程序化音效插件的 **Godot 运行时 + 只读检查器**。声音设计在独立客户端
`~/Documents/sfx-studio`（Rust + igui）里完成；Godot 侧不再提供编辑面板。

## 目录

```
plugins/sfx_studio/
├── plugin.cfg
├── plugin.gd                       # EditorPlugin：只注册 Inspector 扩展
├── core/                           # 运行时（无 Editor* 依赖）
│   ├── sfx_synth.gd
│   ├── sfx_patch.gd
│   ├── sfx_layer.gd
│   ├── sfx_clip.gd
│   ├── sfx_automation_point.gd
│   ├── sfx_music.gd
│   └── sfx_player.gd               # Autoload Sfx
├── editor/
│   ├── sfx_inspector_plugin.gd     # EditorInspectorPlugin
│   ├── sfx_inspector_view.gd       # 只读摘要 + 波形
│   └── waveform_view.gd
└── tools/
    └── self_test.gd
```

## 接入 Godot 工程

本目录 `~/Documents/sfx-studio/plugins/sfx_studio` 是唯一源，和 Rust 客户端
在同一个 Git 仓库。各 Godot 工程用相对软链接加载：

```bash
# 在游戏仓库根目录
tools/sync_sfx_addon.sh
# 每个工程得到：
#   addons/sfx_studio -> ../../../Documents/sfx-studio/plugins/sfx_studio
```

`project.godot` 保持：

```ini
[autoload]
Sfx="*res://addons/sfx_studio/core/sfx_player.gd"

[editor_plugins]
enabled=PackedStringArray("res://addons/sfx_studio/plugin.cfg")
```

## 检查器

选中以下资源时，Inspector 显示只读信息（默认属性编辑器被隐藏）：

- `SfxPatch`：文件、分组、采样率、时长、图层、baked、波形、每层摘要
- `SfxLayer`：波形类型、延迟/时长/增益、频率/包络/滤波、clip 数、波形
- `SfxClip`：起点、时长、增益、变调、自动化点数
- `SfxAutomationPoint`：时间、值
- `SfxMusic`：BPM、小节、时长、音量、和声数、波形

这是**只读**扩展：没有输入框、没有保存/烘焙/时间线，也没有新的 dock 或工具栏按钮。
需要修改参数时使用 SFX LAB 客户端，或直接改 `.tres` 文本。

## 运行时

与旧工程 API 一致：

```gdscript
Sfx.play("eat")
Sfx.play_music()
Sfx.set_music_paused(true)
Sfx.toggle_mute()
```

## 自检

```bash
godot --headless --path <工程> res://addons/sfx_studio/tools/self_test.tscn
# 或
tools/run_test.sh sfx
```
