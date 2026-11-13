extends Node
## 程序化音效播放器（Autoload 名：Sfx）。
##
## 音效参数从 `res://audio/sfx/*.tres`（SfxPatch）自动发现，
## 音乐从 `res://audio/music.tres`（SfxMusic）加载。
##
## 多路复用：预先建好 polyphony 个 AudioStreamPlayer 轮转，连续触发可叠加。
## 静音状态存到 user://，下次启动保持。
##
## 可选的 project.godot 配置（都有默认值，不配也能跑）：
##   [sfx_studio]
##   patch_dir="res://audio/sfx"
##   music_path="res://audio/music.tres"
##   polyphony=10
##   config_path="user://sfx_audio.cfg"

const DEFAULT_PATCH_DIR := "res://audio/sfx"
const DEFAULT_MUSIC_PATH := "res://audio/music.tres"
const DEFAULT_POLYPHONY := 10
const DEFAULT_CONFIG_PATH := "user://sfx_audio.cfg"

var patch_dir := DEFAULT_PATCH_DIR
var music_path := DEFAULT_MUSIC_PATH
var polyphony := DEFAULT_POLYPHONY
var config_path := DEFAULT_CONFIG_PATH

var _streams: Dictionary = {}
var _players: Array[AudioStreamPlayer] = []
var _next := 0

var _music: SfxMusic
var _music_stream: AudioStreamWAV
var _music_player: AudioStreamPlayer
var _muted := false


func _ready() -> void:
	patch_dir = str(ProjectSettings.get_setting("sfx_studio/patch_dir", DEFAULT_PATCH_DIR))
	music_path = str(ProjectSettings.get_setting("sfx_studio/music_path", DEFAULT_MUSIC_PATH))
	polyphony = int(ProjectSettings.get_setting("sfx_studio/polyphony", DEFAULT_POLYPHONY))
	config_path = str(ProjectSettings.get_setting("sfx_studio/config_path", DEFAULT_CONFIG_PATH))

	_build_players()
	reload()
	_load_muted()


## 重新扫描音效目录并重建音频流。编辑器里改完 .tres 后可手动调用。
func reload() -> void:
	_build_streams()
	_load_music()


## 播放音效。pitch 用于随机微调，避免连续触发时过于机械。
func play(sound: String, pitch: float = 1.0, volume_db: float = 0.0) -> void:
	if not _streams.has(sound):
		return
	var player := _players[_next]
	_next = (_next + 1) % _players.size()
	player.stream = _streams[sound]
	player.pitch_scale = pitch
	player.volume_db = volume_db
	player.play()


func has_sound(sound: String) -> bool:
	return _streams.has(sound)


func sound_names() -> Array:
	var names := _streams.keys()
	names.sort()
	return names


## 返回已合成的流（测试 / 外部播放器用）。
func get_stream(sound: String) -> AudioStream:
	return _streams.get(sound)


# =====================================================================
# 背景音乐
# =====================================================================

## 开始（或继续）播放背景音乐。第一次调用时才生成波形，避免拖慢启动。
func play_music() -> void:
	if _music_player == null:
		return
	if _music_stream == null:
		_build_music()
	if _music_stream == null:
		return
	if not _music_player.playing:
		_music_player.stream = _music_stream
		_music_player.play()


## 暂停/恢复音乐（暂停菜单用）。stream_paused 会保留播放位置。
func set_music_paused(paused: bool) -> void:
	if _music_player:
		_music_player.stream_paused = paused


func is_music_playing() -> bool:
	return _music_player != null and _music_player.playing


func music_length() -> float:
	return _music_stream.get_length() if _music_stream else 0.0


## 静音开关（会把状态存到 user://，下次启动保持）。
func toggle_mute() -> bool:
	set_muted(not _muted)
	return _muted


func set_muted(value: bool) -> void:
	_muted = value
	var index := AudioServer.get_bus_index("Master")
	if index >= 0:
		AudioServer.set_bus_mute(index, _muted)
	_save_muted()


func is_muted() -> bool:
	return _muted


func _load_muted() -> void:
	var config := ConfigFile.new()
	if config.load(config_path) != OK:
		return
	_muted = bool(config.get_value("audio", "muted", false))
	var index := AudioServer.get_bus_index("Master")
	if index >= 0:
		AudioServer.set_bus_mute(index, _muted)


func _save_muted() -> void:
	var config := ConfigFile.new()
	config.set_value("audio", "muted", _muted)
	config.save(config_path)


func _build_players() -> void:
	for i in polyphony:
		var p := AudioStreamPlayer.new()
		p.bus = &"Master"
		add_child(p)
		_players.append(p)

	_music_player = AudioStreamPlayer.new()
	_music_player.bus = &"Master"
	_music_player.volume_db = -17.0
	add_child(_music_player)


## 扫描 patch_dir，按文件名（不含扩展名）注册音效。
func _build_streams() -> void:
	_streams.clear()
	var dir := DirAccess.open(patch_dir)
	if dir != null:
		for file in dir.get_files():
			if not (file.ends_with(".tres") or file.ends_with(".res")):
				continue
			var path := patch_dir.path_join(file)
			var patch := load(path)
			if patch is SfxPatch:
				_streams[file.get_basename()] = patch.build()
			else:
				push_warning("[Sfx] 不是 SfxPatch，已跳过：%s" % path)
	else:
		push_warning("[Sfx] 音效目录不存在：%s" % patch_dir)

func _load_music() -> void:
	_music = null
	_music_stream = null
	if not FileAccess.file_exists(music_path):
		return
	_music = load(music_path) as SfxMusic
	if _music == null:
		push_warning("[Sfx] 不是 SfxMusic，已跳过：%s" % music_path)
		return
	if _music_player != null:
		_music_player.volume_db = _music.volume_db
		if _music_player.playing:
			_build_music()
			_music_player.stream = _music_stream
			_music_player.play()


func _build_music() -> void:
	if _music == null:
		return
	_music_stream = _music.build()
