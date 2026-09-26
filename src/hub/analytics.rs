//! Lines-of-code analytics. Each mapped project is scanned periodically; the
//! per-file line counts are diffed against the previous scan, so growth of a
//! file counts as lines written and shrinkage as lines removed.

use std::{
	collections::{BTreeMap, HashMap},
	fs,
	path::Path,
};

use super::store::{now, PlaceStats};

const IGNORED_DIRS: [&str; 12] = [
	".git",
	".github",
	".vscode",
	"node_modules",
	"Packages",
	"ServerPackages",
	"DevPackages",
	"include",
	"out",
	"target",
	"build",
	"dist",
];

const MAX_FILES: usize = 20_000;

pub struct ScanResult {
	pub files: HashMap<String, u64>,
	pub languages: BTreeMap<String, u64>,
}

fn language(path: &Path) -> Option<&'static str> {
	let name = path.file_name()?.to_str()?;

	if name.ends_with(".d.ts") {
		return None;
	}

	match path.extension()?.to_str()? {
		"luau" => Some("Luau"),
		"lua" => Some("Lua"),
		"ts" | "tsx" => Some("TypeScript"),
		_ => None,
	}
}

pub fn scan(workspace: &Path) -> ScanResult {
	let mut result = ScanResult {
		files: HashMap::new(),
		languages: BTreeMap::new(),
	};

	let mut stack = vec![workspace.to_owned()];

	while let Some(dir) = stack.pop() {
		let Ok(entries) = fs::read_dir(&dir) else {
			continue;
		};

		for entry in entries.flatten() {
			let path = entry.path();
			let Ok(file_type) = entry.file_type() else {
				continue;
			};

			if file_type.is_dir() {
				let name = entry.file_name();
				let name = name.to_string_lossy();

				if !IGNORED_DIRS.contains(&name.as_ref()) && !name.starts_with('.') {
					stack.push(path);
				}

				continue;
			}

			let Some(language) = language(&path) else {
				continue;
			};

			let Ok(contents) = fs::read(&path) else {
				continue;
			};

			let lines = contents.split(|b| *b == b'\n').filter(|line| !is_blank(line)).count() as u64;
			let relative = path
				.strip_prefix(workspace)
				.unwrap_or(&path)
				.to_string_lossy()
				.replace('\\', "/");

			result.files.insert(relative, lines);
			*result.languages.entry(language.to_owned()).or_default() += lines;

			if result.files.len() >= MAX_FILES {
				return result;
			}
		}
	}

	result
}

fn is_blank(line: &[u8]) -> bool {
	line.iter().all(|b| b.is_ascii_whitespace())
}

/// Folds a new scan into the stats, returns (added, removed) lines
pub fn apply(stats: &mut PlaceStats, scan: ScanResult) -> (u64, u64) {
	let mut added = 0;
	let mut removed = 0;

	if stats.baseline {
		for (file, &lines) in &scan.files {
			let previous = stats.file_lines.get(file).copied().unwrap_or(0);

			if lines > previous {
				added += lines - previous;
			} else {
				removed += previous - lines;
			}
		}

		for (file, &lines) in &stats.file_lines {
			if !scan.files.contains_key(file) {
				removed += lines;
			}
		}
	}

	let loc: u64 = scan.files.values().sum();

	stats.baseline = true;
	stats.loc = loc;
	stats.files = scan.files.len() as u64;
	stats.languages = scan.languages;
	stats.file_lines = scan.files;
	stats.last_scan = Some(now());
	stats.lines_added += added;
	stats.lines_removed += removed;

	let day = stats.day();
	day.added += added;
	day.removed += removed;
	day.loc = loc;

	if added > 0 || removed > 0 {
		stats.last_active = Some(now());
	}

	(added, removed)
}
