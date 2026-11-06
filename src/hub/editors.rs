//! Code editors installed on this PC, so a place, a file or an error line opens where the developer actually
//! works (VS Code, Cursor, Windsurf, Zed, Sublime Text) instead of only the built-in editor.

use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::{
	env,
	path::{Path, PathBuf},
	process::Command,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorInfo {
	pub id: &'static str,
	pub name: &'static str,
	#[serde(skip)]
	pub path: PathBuf,
	/// VS Code style CLI (`-g file:line`); otherwise `file:line` is passed as is
	#[serde(skip)]
	goto_flag: bool,
}

struct Candidate {
	id: &'static str,
	name: &'static str,
	/// Command names looked up on PATH
	commands: &'static [&'static str],
	/// Paths under %LOCALAPPDATA% / %ProgramFiles%
	local: &'static [&'static str],
	program_files: &'static [&'static str],
	goto_flag: bool,
}

const CANDIDATES: &[Candidate] = &[
	Candidate {
		id: "cursor",
		name: "Cursor",
		commands: &["cursor.cmd", "cursor"],
		local: &["Programs\\cursor\\resources\\app\\bin\\cursor.cmd"],
		program_files: &["cursor\\resources\\app\\bin\\cursor.cmd"],
		goto_flag: true,
	},
	Candidate {
		id: "vscode",
		name: "VS Code",
		commands: &["code.cmd", "code"],
		local: &["Programs\\Microsoft VS Code\\bin\\code.cmd"],
		program_files: &["Microsoft VS Code\\bin\\code.cmd"],
		goto_flag: true,
	},
	Candidate {
		id: "windsurf",
		name: "Windsurf",
		commands: &["windsurf.cmd", "windsurf"],
		local: &["Programs\\Windsurf\\bin\\windsurf.cmd"],
		program_files: &["Windsurf\\bin\\windsurf.cmd"],
		goto_flag: true,
	},
	Candidate {
		id: "zed",
		name: "Zed",
		commands: &["zed.exe", "zed"],
		local: &["Programs\\Zed\\zed.exe", "Programs\\Zed\\bin\\zed.exe"],
		program_files: &["Zed\\zed.exe"],
		goto_flag: false,
	},
	Candidate {
		id: "sublime",
		name: "Sublime Text",
		commands: &["subl.exe", "subl"],
		local: &[],
		program_files: &["Sublime Text\\subl.exe", "Sublime Text 3\\subl.exe"],
		goto_flag: false,
	},
];

fn on_path(command: &str) -> Option<PathBuf> {
	env::split_paths(&env::var_os("PATH")?)
		.map(|dir| dir.join(command))
		.find(|path| path.is_file())
}

pub fn detect() -> Vec<EditorInfo> {
	let local = env::var_os("LOCALAPPDATA").map(PathBuf::from);
	let program_files: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)"]
		.iter()
		.filter_map(|var| env::var_os(var).map(PathBuf::from))
		.collect();

	CANDIDATES
		.iter()
		.filter_map(|candidate| {
			let found = candidate
				.commands
				.iter()
				.find_map(|command| on_path(command))
				.or_else(|| {
					local.as_ref().and_then(|local| {
						candidate
							.local
							.iter()
							.map(|rel| local.join(rel))
							.find(|path| path.is_file())
					})
				})
				.or_else(|| {
					program_files.iter().find_map(|base| {
						candidate
							.program_files
							.iter()
							.map(|rel| base.join(rel))
							.find(|path| path.is_file())
					})
				})?;

			Some(EditorInfo {
				id: candidate.id,
				name: candidate.name,
				path: found,
				goto_flag: candidate.goto_flag,
			})
		})
		.collect()
}

/// Opens `folder` (the project) in the editor, and `file` at `line` inside it when given
pub fn open(editor_id: &str, folder: &Path, file: Option<&Path>, line: Option<u32>) -> Result<()> {
	let editor = detect()
		.into_iter()
		.find(|editor| editor.id == editor_id)
		.with_context(|| format!("{editor_id} isn't installed (or isn't on PATH)"))?;

	let mut args: Vec<String> = Vec::new();

	if editor.goto_flag {
		// Opening the folder too makes VS Code style editors reuse the project's window
		args.push(folder.to_string_lossy().to_string());

		if let Some(file) = file {
			args.push("-g".into());
			args.push(format!("{}:{}", file.display(), line.unwrap_or(1)));
		}
	} else if let Some(file) = file {
		args.push(format!("{}:{}", file.display(), line.unwrap_or(1)));
	} else {
		args.push(folder.to_string_lossy().to_string());
	}

	let is_script = editor
		.path
		.extension()
		.is_some_and(|ext| ext.eq_ignore_ascii_case("cmd") || ext.eq_ignore_ascii_case("bat"));

	let mut command = if is_script {
		let mut command = Command::new("cmd");

		// cmd.exe has its own quoting: /S /C ""launcher" "arg" ..."" keeps paths with spaces intact
		#[cfg(windows)]
		{
			use std::os::windows::process::CommandExt;

			let quoted: Vec<String> = std::iter::once(editor.path.to_string_lossy().to_string())
				.chain(args.iter().cloned())
				.map(|arg| format!("\"{}\"", arg.replace('"', "")))
				.collect();

			command.raw_arg(format!("/S /C \"{}\"", quoted.join(" ")));
		}

		#[cfg(not(windows))]
		command.arg("/C").arg(&editor.path).args(&args);

		command
	} else {
		let mut command = Command::new(&editor.path);
		command.args(&args);
		command
	};

	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// CREATE_NO_WINDOW: no console flashing up for the .cmd launcher
		command.creation_flags(0x0800_0000);
	}

	match command.spawn() {
		Ok(_) => Ok(()),
		Err(err) => bail!("Couldn't start {}: {err}", editor.name),
	}
}
