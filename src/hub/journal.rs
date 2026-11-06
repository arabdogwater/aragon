//! What happened to each place, for the dashboard (and for AI agents working in the project folder):
//! - activity: every file synced, in which direction, live
//! - output: Studio's Output window (edit mode and playtests), with errors linked to the files
//! - problems: sync warnings turned into plain language, each with a one-click fix
//! - history: the safety net, a copy of every file Studio overwrote or deleted on disk

use crossbeam_channel::{Receiver, Sender};
use log::{debug, Level};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
	collections::{HashMap, VecDeque},
	fs::{self, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, LazyLock, Mutex, Weak,
	},
	thread,
	time::{Duration, Instant},
};

use super::Hub;
use crate::{
	lock,
	vfs::{JournalKind, VfsJournal},
};

const ACTIVITY_LIMIT: usize = 300;
const OUTPUT_LIMIT: usize = 2000;
const PROBLEM_LIMIT: usize = 40;
/// Problems that stop happening drop off after this long
const PROBLEM_TTL_MS: i64 = 2 * 3600 * 1000;
/// Events closer than this belong to the same batch (one Studio write, one restore)
const BATCH_GAP_MS: i64 = 2500;
const HISTORY_DAYS: i64 = 14;
const HISTORY_MAX_BYTES: u64 = 300 * 1024 * 1024;
const HISTORY_MAX_FILE: u64 = 10 * 1024 * 1024;
const OUTPUT_LOG_MAX: u64 = 2 * 1024 * 1024;

pub fn now_ms() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

fn relative(workspace: &Path, path: &Path) -> String {
	let path = path_clean::clean(path);
	let workspace = path_clean::clean(workspace);

	path.strip_prefix(&workspace)
		.unwrap_or(&path)
		.to_string_lossy()
		.replace('\\', "/")
}

fn short_hash(bytes: &[u8]) -> String {
	let digest = Sha256::digest(bytes);
	digest[..8].iter().map(|b| format!("{b:02x}")).collect()
}

// Activity ///////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Activity {
	pub t: i64,
	/// `toStudio` (a change on disk reached Studio) or `toDisk` (Studio wrote a file)
	pub dir: &'static str,
	pub kind: &'static str,
	pub path: String,
	pub count: u32,
}

// Output /////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLink {
	/// Range of the instance reference inside `text`, in UTF-16 code units
	pub start: usize,
	pub end: usize,
	pub file: String,
	pub line: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputLine {
	pub id: u64,
	pub t: i64,
	pub kind: String,
	pub context: String,
	pub text: String,
	pub links: Vec<OutputLink>,
}

#[derive(Debug, Deserialize)]
pub struct OutputMessage {
	#[serde(default)]
	pub t: Option<f64>,
	pub kind: String,
	pub text: String,
}

// Problems ///////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
	pub action: &'static str,
	pub label: &'static str,
	pub arg: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Problem {
	pub id: String,
	pub severity: &'static str,
	pub title: String,
	pub detail: String,
	pub file: Option<String>,
	pub count: u32,
	pub first: i64,
	pub last: i64,
	pub fixes: Vec<Fix>,
	/// The log line it came from, for "copy for AI" and the curious
	pub raw: String,
}

// History ////////////////////////////////////////////////////////////////////////////////////////

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
	pub id: String,
	pub t: i64,
	pub batch: i64,
	pub path: String,
	/// `overwrite` or `delete`
	pub action: String,
	pub size: u64,
	pub hash: String,
}

#[derive(Default)]
struct PlaceJournal {
	activity: VecDeque<Activity>,
	output: VecDeque<OutputLine>,
	problems: Vec<Problem>,
	history: Option<Vec<HistoryEntry>>,
	last_history_t: i64,
	batch: i64,
}

struct LogRecord {
	level: Level,
	message: String,
	thread: Option<String>,
}

pub struct Journal {
	root: PathBuf,
	places: Mutex<HashMap<String, PlaceJournal>>,
	next_output_id: Mutex<u64>,
	dirty: AtomicBool,
	logs: (Sender<LogRecord>, Receiver<LogRecord>),
}

impl Journal {
	pub fn new() -> Arc<Self> {
		let root = crate::util::get_aragon_dir()
			.map(|dir| dir.join("history"))
			.unwrap_or_else(|_| std::env::temp_dir().join("aragon-history"));

		Arc::new(Self {
			root,
			places: Mutex::new(HashMap::new()),
			next_output_id: Mutex::new(1),
			dirty: AtomicBool::new(false),
			logs: crossbeam_channel::unbounded(),
		})
	}

	/// Hooks the logger and starts the thread that turns log lines into problems, wakes the dashboard
	/// (at most a few times a second, an import can write thousands of files) and prunes old history
	pub fn start(self: &Arc<Self>, hub: Weak<Hub>) {
		let sender = self.logs.0.clone();

		crate::logger::set_listener(Box::new(move |level, message, thread| {
			sender.send(LogRecord { level, message, thread }).ok();
		}));

		let journal = self.clone();

		thread::Builder::new()
			.name("hub-journal".into())
			.spawn(move || {
				let mut last_prune = Instant::now() - Duration::from_secs(3600);

				loop {
					let deadline = Instant::now() + Duration::from_millis(350);

					while let Ok(record) = journal.logs.1.recv_deadline(deadline) {
						if let Some(hub) = hub.upgrade() {
							journal.on_log(&hub, record);
						}
					}

					let Some(hub) = hub.upgrade() else {
						break;
					};

					if journal.dirty.swap(false, Ordering::Relaxed) {
						hub.bump();
					}

					if last_prune.elapsed() > Duration::from_secs(3600) {
						last_prune = Instant::now();
						journal.prune_history();
					}
				}
			})
			.unwrap();
	}

	fn touch(&self) {
		self.dirty.store(true, Ordering::Relaxed);
	}

	fn with<R>(&self, key: &str, f: impl FnOnce(&mut PlaceJournal) -> R) -> R {
		let mut places = lock!(self.places);
		f(places.entry(key.to_owned()).or_default())
	}

	// Activity ///////////////////////////////////////////////////////////////////////////////////

	pub fn record_activity(&self, key: &str, dir: &'static str, kind: &'static str, path: String) {
		let t = now_ms();

		self.with(key, |place| {
			if let Some(last) = place.activity.back_mut() {
				if last.path == path && last.dir == dir && t - last.t < 3000 {
					last.t = t;
					last.count += 1;
					last.kind = kind;
					return;
				}
			}

			place.activity.push_back(Activity {
				t,
				dir,
				kind,
				path,
				count: 1,
			});

			while place.activity.len() > ACTIVITY_LIMIT {
				place.activity.pop_front();
			}
		});

		self.touch();
	}

	pub fn activity(&self, key: &str) -> Vec<Activity> {
		self.with(key, |place| place.activity.iter().rev().cloned().collect())
	}

	pub fn last_activity(&self, key: &str) -> Option<Activity> {
		lock!(self.places)
			.get(key)
			.and_then(|place| place.activity.back().cloned())
	}

	// Output /////////////////////////////////////////////////////////////////////////////////////

	/// Stores Studio output lines, links `Script:line` references to files and mirrors everything into
	/// `<project>/.aragon/studio-output.log` so an AI agent can read runtime errors
	pub fn add_output(
		&self,
		key: &str,
		context: &str,
		messages: Vec<OutputMessage>,
		workspace: Option<&Path>,
		resolve: &dyn Fn(&str) -> Option<PathBuf>,
	) {
		static LINE_REF: LazyLock<Regex> =
			LazyLock::new(|| Regex::new(r"([A-Za-z_][\w]*(?:\.[A-Za-z_][\w]*)+):(\d+)").unwrap());
		static OLD_REF: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Script '([^']+)', Line (\d+)").unwrap());

		let mut lines = Vec::with_capacity(messages.len());

		for message in messages {
			let mut links = Vec::new();

			for regex in [&*LINE_REF, &*OLD_REF] {
				for captures in regex.captures_iter(&message.text) {
					let (Some(whole), Some(name), Some(line)) = (captures.get(0), captures.get(1), captures.get(2))
					else {
						continue;
					};

					let Some(file) = resolve(name.as_str()) else {
						continue;
					};

					let file = workspace.map_or_else(|| file.to_string_lossy().to_string(), |w| relative(w, &file));

					// UTF-16 offsets: what JavaScript strings index by
					let utf16 = |byte: usize| message.text[..byte].encode_utf16().count();

					links.push(OutputLink {
						start: utf16(whole.start()),
						end: utf16(whole.end()),
						file,
						line: line.as_str().parse().unwrap_or(1),
					});
				}
			}

			let id = {
				let mut next = lock!(self.next_output_id);
				*next += 1;
				*next
			};

			lines.push(OutputLine {
				id,
				t: message.t.map_or_else(now_ms, |t| (t * 1000.0) as i64),
				kind: message.kind,
				context: context.to_owned(),
				text: message.text,
				links,
			});
		}

		if let Some(workspace) = workspace {
			write_output_log(workspace, &lines);
		}

		self.with(key, |place| {
			place.output.extend(lines);

			while place.output.len() > OUTPUT_LIMIT {
				place.output.pop_front();
			}
		});

		self.touch();
	}

	pub fn output(&self, key: &str, since: u64) -> Vec<OutputLine> {
		self.with(key, |place| {
			place.output.iter().filter(|line| line.id > since).cloned().collect()
		})
	}

	pub fn clear_output(&self, key: &str) {
		self.with(key, |place| place.output.clear());
		self.touch();
	}

	/// Errors Studio printed in the last 10 minutes
	pub fn recent_errors(&self, key: &str) -> usize {
		let since = now_ms() - 10 * 60 * 1000;

		lock!(self.places).get(key).map_or(0, |place| {
			place
				.output
				.iter()
				.filter(|line| line.kind == "error" && line.t >= since)
				.count()
		})
	}

	// Problems ///////////////////////////////////////////////////////////////////////////////////

	fn on_log(&self, hub: &Hub, record: LogRecord) {
		// Which place is this about? The sync thread of a place names itself after its project file,
		// anything else is matched by the project folder showing up in the message
		let places: Vec<(String, PathBuf, PathBuf)> = hub
			.store()
			.places
			.values()
			.filter_map(|place| {
				let project = place.project.clone()?;
				let workspace = project.parent()?.to_owned();
				Some((place.key.clone(), project, workspace))
			})
			.collect();

		let from_thread = record
			.thread
			.as_deref()
			.and_then(|name| name.strip_prefix("processor|"))
			.and_then(|project| places.iter().find(|(_, p, _)| Path::new(project) == p));

		let lower = record.message.to_lowercase();
		let target = from_thread.or_else(|| {
			places
				.iter()
				.find(|(_, _, workspace)| lower.contains(&workspace.to_string_lossy().to_lowercase()))
		});

		let Some((key, _, workspace)) = target else {
			return;
		};

		let Some(mut problem) = classify(record.level, &record.message, workspace) else {
			return;
		};

		debug!("Sync problem for {key}: {}", problem.title);

		let now = now_ms();

		self.with(key, |place| {
			place.problems.retain(|p| now - p.last < PROBLEM_TTL_MS);

			if let Some(existing) = place.problems.iter_mut().find(|p| p.id == problem.id) {
				existing.count += 1;
				existing.last = now;
				existing.raw = problem.raw;
				return;
			}

			problem.first = now;
			problem.last = now;
			place.problems.push(problem);

			if place.problems.len() > PROBLEM_LIMIT {
				place.problems.remove(0);
			}
		});

		self.write_problems_file(key, workspace);
		self.touch();
	}

	pub fn problems(&self, key: &str) -> Vec<Problem> {
		let now = now_ms();

		lock!(self.places).get(key).map_or_else(Vec::new, |place| {
			let mut problems: Vec<Problem> = place
				.problems
				.iter()
				.filter(|p| now - p.last < PROBLEM_TTL_MS)
				.cloned()
				.collect();

			problems.sort_by(|a, b| {
				(b.severity == "error")
					.cmp(&(a.severity == "error"))
					.then(b.last.cmp(&a.last))
			});
			problems
		})
	}

	pub fn dismiss_problem(&self, key: &str, id: Option<&str>) {
		self.with(key, |place| match id {
			Some(id) => place.problems.retain(|p| p.id != id),
			None => place.problems.clear(),
		});

		self.touch();
	}

	/// `<project>/.aragon/sync-problems.md`, the same list in a form an AI agent can read
	fn write_problems_file(&self, key: &str, workspace: &Path) {
		let problems = self.problems(key);
		let dir = ensure_agent_dir(workspace);

		let mut text = String::from("# Aragon sync problems\n\nCurrent problems syncing this project with Roblox Studio, newest first. Written by Aragon.\n\n");

		if problems.is_empty() {
			text.push_str("None right now.\n");
		}

		for problem in problems {
			text.push_str(&format!("## {}\n\n{}\n", problem.title, problem.detail));

			if let Some(file) = &problem.file {
				text.push_str(&format!("\nFile: `{file}`\n"));
			}

			text.push_str(&format!(
				"\nSeen {} time(s). Log line: `{}`\n\n",
				problem.count,
				problem.raw.trim()
			));
		}

		fs::write(dir.join("sync-problems.md"), text).ok();
	}

	// History (safety net) ///////////////////////////////////////////////////////////////////////

	fn history_dir(&self, key: &str) -> PathBuf {
		let safe: String = key
			.chars()
			.map(|c| {
				if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
					c
				} else {
					'_'
				}
			})
			.collect();

		self.root.join(safe)
	}

	fn load_history(&self, key: &str) -> Vec<HistoryEntry> {
		let path = self.history_dir(key).join("index.jsonl");

		fs::read_to_string(path)
			.unwrap_or_default()
			.lines()
			.filter_map(|line| serde_json::from_str(line).ok())
			.collect()
	}

	fn history_mut<R>(&self, key: &str, f: impl FnOnce(&mut Vec<HistoryEntry>, &mut PlaceJournal) -> R) -> R {
		let loaded = lock!(self.places).get(key).is_some_and(|p| p.history.is_some());
		let fresh = if loaded { None } else { Some(self.load_history(key)) };

		self.with(key, |place| {
			if let Some(fresh) = fresh {
				if place.history.is_none() {
					place.history = Some(fresh);
				}
			}

			let mut history = place.history.take().unwrap_or_default();
			let result = f(&mut history, place);
			place.history = Some(history);
			result
		})
	}

	/// Keeps a copy of `path` before Studio overwrites or deletes it
	pub fn backup(&self, key: &str, workspace: &Path, path: &Path, action: &str) {
		let Ok(meta) = fs::metadata(path) else {
			return;
		};

		if meta.is_dir() {
			for entry in fs::read_dir(path).into_iter().flatten().flatten() {
				self.backup(key, workspace, &entry.path(), action);
			}

			return;
		}

		if meta.len() > HISTORY_MAX_FILE {
			return;
		}

		let Ok(bytes) = fs::read(path) else {
			return;
		};

		let rel = relative(workspace, path);
		let hash = short_hash(&bytes);
		let dir = self.history_dir(key);

		self.history_mut(key, |history, place| {
			// The same content is already kept for this file
			if history
				.iter()
				.rev()
				.find(|e| e.path == rel)
				.is_some_and(|e| e.hash == hash)
			{
				return;
			}

			let t = now_ms();

			if t - place.last_history_t > BATCH_GAP_MS {
				place.batch = t;
			}

			place.last_history_t = t;

			let id = format!("{t}-{hash}");
			let blobs = dir.join("blobs");

			if fs::create_dir_all(&blobs).is_err() || fs::write(blobs.join(&id), &bytes).is_err() {
				return;
			}

			let entry = HistoryEntry {
				id,
				t,
				batch: place.batch,
				path: rel,
				action: action.to_owned(),
				size: bytes.len() as u64,
				hash,
			};

			if let Ok(mut file) = OpenOptions::new()
				.create(true)
				.append(true)
				.open(dir.join("index.jsonl"))
			{
				if let Ok(line) = serde_json::to_string(&entry) {
					writeln!(file, "{line}").ok();
				}
			}

			history.push(entry);
		});

		self.touch();
	}

	pub fn history(&self, key: &str) -> Vec<HistoryEntry> {
		self.history_mut(key, |history, _| history.iter().rev().take(2000).cloned().collect())
	}

	pub fn history_count(&self, key: &str) -> usize {
		lock!(self.places)
			.get(key)
			.and_then(|p| p.history.as_ref())
			.map_or(0, Vec::len)
	}

	pub fn history_blob(&self, key: &str, id: &str) -> Option<(HistoryEntry, Vec<u8>)> {
		let entry = self.history_mut(key, |history, _| history.iter().find(|e| e.id == id).cloned())?;
		let bytes = fs::read(self.history_dir(key).join("blobs").join(&entry.id)).ok()?;

		Some((entry, bytes))
	}

	/// Drops copies older than two weeks, then the oldest ones while a place keeps more than its budget
	fn prune_history(&self) {
		let Ok(dirs) = fs::read_dir(&self.root) else {
			return;
		};

		let cutoff = now_ms() - HISTORY_DAYS * 24 * 3600 * 1000;

		for dir in dirs.flatten() {
			let index = dir.path().join("index.jsonl");
			let Ok(text) = fs::read_to_string(&index) else {
				continue;
			};

			let mut entries: Vec<HistoryEntry> = text.lines().filter_map(|l| serde_json::from_str(l).ok()).collect();
			let before = entries.len();
			let mut total: u64 = entries.iter().map(|e| e.size).sum();

			entries.retain(|entry| {
				let keep = entry.t >= cutoff;

				if !keep {
					total -= entry.size;
				}

				keep
			});

			while total > HISTORY_MAX_BYTES && !entries.is_empty() {
				total -= entries.remove(0).size;
			}

			if entries.len() == before {
				continue;
			}

			let kept: std::collections::HashSet<&str> = entries.iter().map(|e| e.id.as_str()).collect();

			for blob in fs::read_dir(dir.path().join("blobs")).into_iter().flatten().flatten() {
				if !kept.contains(blob.file_name().to_string_lossy().as_ref()) {
					fs::remove_file(blob.path()).ok();
				}
			}

			let text: String = entries
				.iter()
				.filter_map(|e| serde_json::to_string(e).ok())
				.map(|line| line + "\n")
				.collect();

			fs::write(&index, text).ok();

			// Reloaded from disk next time it's needed
			let key = dir.file_name().to_string_lossy().to_string();
			for (place_key, place) in lock!(self.places).iter_mut() {
				if self.history_dir(place_key).ends_with(&key) {
					place.history = None;
				}
			}
		}
	}
}

/// `<project>/.aragon/`, ignored by git, with a README telling AI agents what's inside
pub fn ensure_agent_dir(workspace: &Path) -> PathBuf {
	let dir = workspace.join(".aragon");

	if !dir.exists() {
		fs::create_dir_all(&dir).ok();
		fs::write(dir.join(".gitignore"), "*\n").ok();
		fs::write(
			dir.join("README.md"),
			"# .aragon\n\nLive files Aragon keeps for you and your AI agents. Ignored by git.\n\n\
			- `studio-output.log`: Roblox Studio's Output window (edit mode and playtests), newest at the bottom. \
			Errors end with `-> path/to/file.luau:line`, the file in this project that raised them.\n\
			- `sync-problems.md`: current problems syncing this project with Studio, in plain language.\n",
		)
		.ok();
	}

	dir
}

fn write_output_log(workspace: &Path, lines: &[OutputLine]) {
	if lines.is_empty() {
		return;
	}

	let path = ensure_agent_dir(workspace).join("studio-output.log");

	// Keep the newest half once it grows too big
	if fs::metadata(&path).is_ok_and(|m| m.len() > OUTPUT_LOG_MAX) {
		if let Ok(text) = fs::read_to_string(&path) {
			let cut = text.len() / 2;
			let start = text[cut..].find('\n').map_or(cut, |i| cut + i + 1);
			fs::write(&path, &text[start..]).ok();
		}
	}

	let mut text = String::new();

	for line in lines {
		let time = chrono::DateTime::from_timestamp_millis(line.t)
			.map(|t| t.with_timezone(&chrono::Local).format("%H:%M:%S").to_string())
			.unwrap_or_default();

		let kind = match line.kind.as_str() {
			"error" => "ERROR ",
			"warn" => "WARN  ",
			"info" => "INFO  ",
			_ => "",
		};

		let mut row = format!(
			"{time} [{}] {kind}{}",
			line.context.to_lowercase(),
			line.text.trim_end()
		);

		if let Some(link) = line.links.first() {
			row.push_str(&format!("  -> {}:{}", link.file, link.line));
		}

		text.push_str(&row.replace('\n', "\n    "));
		text.push('\n');
	}

	if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
		file.write_all(text.as_bytes()).ok();
	}
}

/// Turns a hub log line into a problem a person can act on, or `None` when it isn't worth showing
fn classify(level: Level, message: &str, workspace: &Path) -> Option<Problem> {
	static EXISTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Instance with path: (.+?) already exists").unwrap());
	static MISSING_PATH: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"Path specified in the project does not exist: (.+?)\. Please").unwrap());
	static LOCKED: LazyLock<Regex> = LazyLock::new(|| {
		Regex::new(r"Failed to \w+ \w+ at (.+?): .*(os error (1224|32|33|5)\b|used by another)").unwrap()
	});
	static PARTIAL: LazyLock<Regex> =
		LazyLock::new(|| Regex::new(r"(\d+) of (\d+) changes from Studio could not be written").unwrap());
	static REFS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"Ref\(Some\(\d+\)\)|\d{6,}").unwrap());
	static UNREADABLE: LazyLock<Regex> = LazyLock::new(|| {
		Regex::new(r"Failed to process changes: (?:.*? at )?(.+?\.\w+): (.+?)(?:, source: .*)?$").unwrap()
	});

	let file_of = |path: &str| relative(workspace, Path::new(path.trim()));
	let raw = message.to_owned();

	let problem = |id: String, severity, title: &str, detail: String, file: Option<String>, fixes: Vec<Fix>| Problem {
		id,
		severity,
		title: title.to_owned(),
		detail,
		file,
		count: 1,
		first: 0,
		last: 0,
		fixes,
		raw: raw.clone(),
	};

	let reconnect = || Fix {
		action: "reconnect",
		label: "Reconnect Studio",
		arg: None,
	};

	if let Some(captures) = EXISTS.captures(message) {
		let file = file_of(&captures[1]);
		let name = file.rsplit('/').next().unwrap_or(&file).to_owned();

		return Some(problem(
			format!("duplicate:{file}"),
			"warn",
			"Two Studio instances share one file",
			format!(
				"Studio has two instances called \u{201c}{name}\u{201d} in the same place. Only one of them can be saved to \
				{file}, so the other one isn't synced and any edit made to it is lost. Delete or rename the extra copy in Studio."
			),
			Some(file.clone()),
			vec![
				Fix {
					action: "selectInStudio",
					label: "Select them in Studio",
					arg: Some(file),
				},
				reconnect(),
			],
		));
	}

	if message.contains("instance that doesn't exist") || message.contains("whose parent doesn't exist") {
		return Some(problem(
			"drift".into(),
			"warn",
			"Studio and your files are out of step",
			"Studio sent changes for instances the hub doesn't know: usually a duplicate, or something that was moved \
			out of the synced tree and back. Those edits weren't saved to disk. Reconnecting resyncs everything."
				.into(),
			None,
			vec![reconnect()],
		));
	}

	if let Some(captures) = LOCKED.captures(message) {
		let file = file_of(&captures[1]);

		return Some(problem(
			format!("locked:{file}"),
			"error",
			"A file was locked by another program",
			format!(
				"Another program (an editor, OneDrive, an antivirus) had {file} open, so Studio's change to it didn't reach \
				the disk. Close it there and reconnect to write it again."
			),
			Some(file.clone()),
			vec![
				Fix {
					action: "openFile",
					label: "Open the file",
					arg: Some(file),
				},
				reconnect(),
			],
		));
	}

	if let Some(captures) = MISSING_PATH.captures(message) {
		let file = file_of(&captures[1]);
		let packages = ["Packages", "ServerPackages", "DevPackages"]
			.iter()
			.any(|name| file.ends_with(name));

		return Some(if packages {
			problem(
				format!("packages:{file}"),
				"warn",
				"Wally packages aren't installed",
				format!("The project syncs {file}, but that folder doesn't exist yet, so your packages are missing in Studio."),
				Some(file),
				vec![Fix {
					action: "wally",
					label: "Run wally install",
					arg: None,
				}],
			)
		} else {
			problem(
				format!("missing:{file}"),
				"warn",
				"A folder from the project file is missing",
				format!(
					"The project file maps {file}, but it doesn't exist. Create it or remove it from the project file."
				),
				Some(file),
				vec![Fix {
					action: "openFolder",
					label: "Open the project folder",
					arg: None,
				}],
			)
		});
	}

	if let Some(captures) = PARTIAL.captures(message) {
		return Some(problem(
			"partial".into(),
			"error",
			"Some of Studio's changes didn't reach the disk",
			format!(
				"{} of {} changes from Studio couldn't be written. The problems above say which files and why.",
				&captures[1], &captures[2]
			),
			None,
			vec![reconnect()],
		));
	}

	if message.contains("Top level project file was deleted") {
		return Some(problem(
			"project-deleted".into(),
			"warn",
			"The project file was deleted or replaced",
			"Something deleted the project file (an editor or a tool rewriting it). Changes are paused until it's back."
				.into(),
			None,
			vec![reconnect()],
		));
	}

	if let Some(captures) = UNREADABLE.captures(message) {
		let file = file_of(&captures[1]);
		let error = captures[2].trim();

		return Some(problem(
			format!("read:{file}"),
			"error",
			"A file has an error and can't sync",
			format!("{file}: {error}. Fix it and save; it syncs to Studio as soon as it's valid."),
			Some(file.clone()),
			vec![Fix {
				action: "openFile",
				label: "Open the file",
				arg: Some(file),
			}],
		));
	}

	if message.contains("Failed to process changes") {
		let detail = message
			.split("Failed to process changes:")
			.nth(1)
			.unwrap_or(message)
			.trim();

		return Some(problem(
			format!("read:{}", REFS.replace_all(detail, "#")),
			"error",
			"A file couldn't be read",
			format!("A change on disk couldn't be synced to Studio: {detail}"),
			None,
			vec![],
		));
	}

	if level == Level::Error {
		let normalized = REFS.replace_all(message, "#").to_string();

		return Some(problem(
			format!("error:{normalized}"),
			"error",
			"Sync error",
			message.trim().to_owned(),
			None,
			vec![reconnect()],
		));
	}

	None
}

/// The journal of one loaded project, given to its sync engine
pub struct CoreJournal {
	pub journal: Arc<Journal>,
	pub key: String,
	pub workspace: PathBuf,
}

impl VfsJournal for CoreJournal {
	fn before_write(&self, path: &Path, contents: &[u8]) {
		// Only when the content really changes
		if fs::read(path).is_ok_and(|old| old != contents) {
			self.journal.backup(&self.key, &self.workspace, path, "overwrite");
		}
	}

	fn before_remove(&self, path: &Path) {
		self.journal.backup(&self.key, &self.workspace, path, "delete");
	}

	fn written(&self, path: &Path, kind: JournalKind) {
		let kind = match kind {
			JournalKind::Created => "created",
			JournalKind::Updated => "updated",
			JournalKind::Removed => "removed",
			JournalKind::Renamed => "renamed",
		};

		self.journal
			.record_activity(&self.key, "toDisk", kind, relative(&self.workspace, path));
	}

	fn incoming(&self, path: &Path, _changes: usize) {
		let kind = if path.exists() { "changed" } else { "removed" };

		self.journal
			.record_activity(&self.key, "toStudio", kind, relative(&self.workspace, path));
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const WS: &str = r"C:\D\scripts\roblox projects\White Noise Games\Arm Wrestle Animals";

	fn classify_line(level: Level, line: &str) -> Option<Problem> {
		classify(level, line, Path::new(WS))
	}

	#[test]
	fn duplicate_instances() {
		let problem = classify_line(
			Level::Error,
			r"Instance with path: C:\D\scripts\roblox projects\White Noise Games\Arm Wrestle Animals\src\ReplicatedStorage\Data already exists! Skipping..",
		)
		.unwrap();

		assert_eq!(problem.id, "duplicate:src/ReplicatedStorage/Data");
		assert_eq!(problem.file.as_deref(), Some("src/ReplicatedStorage/Data"));
		assert_eq!(problem.fixes[0].action, "selectInStudio");
	}

	#[test]
	fn drift() {
		let problem = classify_line(
			Level::Warn,
			"Attempted to update instance that doesn't exist: Ref(Some(264099171931332843642885858605984537436))",
		)
		.unwrap();

		assert_eq!(problem.id, "drift");
		assert_eq!(problem.fixes[0].action, "reconnect");
	}

	#[test]
	fn locked_file() {
		let problem = classify_line(
			Level::Error,
			r"Failed to update Ref(Some(110549936291442514350529102061852130749)): Failed to write ModuleScript at C:\D\scripts\roblox projects\White Noise Games\Arm Wrestle Animals\src\ReplicatedStorage\Modules\CageReveal.luau: The requested operation cannot be performed on a file with a user-mapped section open. (os error 1224)",
		)
		.unwrap();

		assert_eq!(
			problem.file.as_deref(),
			Some("src/ReplicatedStorage/Modules/CageReveal.luau")
		);
		assert_eq!(problem.severity, "error");
	}

	#[test]
	fn missing_packages() {
		let problem = classify_line(
			Level::Warn,
			r"Path specified in the project does not exist: C:\D\scripts\roblox projects\White Noise Games\Arm Wrestle Animals\Packages. Please create this path and restart Aragon to watch for file changes in this path or remove it from the project to suppress this warning",
		)
		.unwrap();

		assert_eq!(problem.fixes[0].action, "wally");
		assert_eq!(problem.file.as_deref(), Some("Packages"));
	}

	#[test]
	fn unreadable_file() {
		let problem = classify_line(
			Level::Error,
			r#"Failed to process changes: Failed to read JsonModel at C:\D\scripts\roblox projects\White Noise Games\Arm Wrestle Animals\src\Bad.model.json: key must be a string at line 1 column 26, source: Project("ReplicatedStorage", "x")"#,
		)
		.unwrap();

		assert_eq!(problem.file.as_deref(), Some("src/Bad.model.json"));
		assert!(problem
			.detail
			.starts_with("src/Bad.model.json: key must be a string at line 1 column 26."));
	}

	#[test]
	fn noise_is_ignored() {
		assert!(classify_line(Level::Warn, "Failed to unwatch some path").is_none());
	}

	#[test]
	fn unknown_errors_dedupe_across_refs() {
		let a = classify_line(Level::Error, "Failed to remove Ref(Some(1234567890123)): boom").unwrap();
		let b = classify_line(Level::Error, "Failed to remove Ref(Some(9876543210987)): boom").unwrap();

		assert_eq!(a.id, b.id);
	}
}
