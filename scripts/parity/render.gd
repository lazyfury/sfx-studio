extends SceneTree
## Render every `.tres` fixture with the Godot plugin and save 16-bit WAVs.
## Usage: Godot --headless --path <project> --script res://render.gd -- <fixtures> <out>

func _init() -> void:
	var args := OS.get_cmdline_user_args()
	if args.size() < 2:
		printerr("usage: render.gd <fixtures> <out>")
		quit(2)
		return
	var dir: String = args[0]
	var out: String = args[1]
	DirAccess.make_dir_recursive_absolute(out)
	var rendered := 0
	for file in DirAccess.get_files_at(dir):
		if not file.ends_with(".tres"):
			continue
		var path := dir.path_join(file)
		var resource = load(path)
		if resource == null or not resource.has_method("build"):
			printerr("load failed: ", path)
			quit(1)
			return
		var stream = resource.build()
		if stream == null:
			printerr("build failed: ", path)
			quit(1)
			return
		var target := out.path_join(file.get_basename() + ".wav")
		var err: int = stream.save_to_wav(target)
		if err != OK:
			printerr("save failed: ", target, " err=", err)
			quit(1)
			return
		rendered += 1
	print("rendered ", rendered, " fixtures")
	quit(0)
