use crossbeam_channel::Receiver;
use log::{debug, warn};
use std::{
	fs,
	io::{Error, Result},
	path::{Path, PathBuf},
	sync::{Arc, RwLock},
};

use super::{
	debouncer::{Links, VfsDebouncer},
	VfsBackend, VfsEvent,
};
use crate::config::Config;

pub struct StdBackend {
	watching: bool,
	debouncer: VfsDebouncer,
	watched_paths: Vec<PathBuf>,
	/// Symlinks / junctions inside watched paths: (link, resolved target). The OS watcher never
	/// crosses them (a recursive ReadDirectoryChangesW stops at a junction), so each target gets its
	/// own watch and the debouncer maps its events back onto the link path the tree knows.
	links: Links,
}

impl StdBackend {
	pub fn new(watch: bool) -> Self {
		let links: Links = Arc::new(RwLock::new(Vec::new()));

		Self {
			watching: watch,
			debouncer: VfsDebouncer::new(links.clone()),
			watched_paths: Vec::new(),
			links,
		}
	}

	/// Watches the target of `path` if it is a link, then (recursive) every link below it.
	fn watch_links(&mut self, path: &Path, recursive: bool) {
		if is_link(path) {
			self.watch_link(path, recursive);
		} else if recursive && path.is_dir() {
			self.scan_links(path);
		}
	}

	fn scan_links(&mut self, dir: &Path) {
		let Ok(entries) = fs::read_dir(dir) else {
			return;
		};

		for entry in entries.flatten() {
			let Ok(file_type) = entry.file_type() else {
				continue;
			};

			let path = entry.path();

			if file_type.is_symlink() {
				self.watch_link(&path, true);
			} else if file_type.is_dir() {
				self.scan_links(&path);
			}
		}
	}

	fn watch_link(&mut self, link: &Path, recursive: bool) {
		if self.links.read().unwrap().iter().any(|(known, _)| known == link) {
			return;
		}

		let target = match fs::canonicalize(link) {
			Ok(target) => strip_verbatim(target),
			Err(err) => {
				warn!("Link {link:?} points nowhere, not watching it: {err}");
				return;
			}
		};

		let recursive = recursive && target.is_dir();

		if let Err(err) = self.debouncer.watch(&target, recursive) {
			warn!("Failed to watch link target {target:?} (from {link:?}): {err}");
			return;
		}

		debug!("Watching link {link:?} -> {target:?}");

		self.links.write().unwrap().push((link.to_owned(), target.clone()));

		if recursive {
			// links inside the target are seen through the link path, so scan them from there
			self.scan_links(link);
		}
	}
}

fn is_link(path: &Path) -> bool {
	fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// `fs::canonicalize` returns `\\?\C:\...` on Windows; the watcher reports paths in the form it was
/// given, so keep the plain form (UNC paths stay verbatim).
fn strip_verbatim(path: PathBuf) -> PathBuf {
	let text = path.to_string_lossy();

	match text.strip_prefix(r"\\?\") {
		Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
		_ => path,
	}
}

impl VfsBackend for StdBackend {
	fn read(&self, path: &Path) -> Result<Vec<u8>> {
		fs::read(path)
	}

	fn read_to_string(&self, path: &Path) -> Result<String> {
		let contents = fs::read_to_string(path)?;

		if Config::new().ignore_line_endings && contents.contains('\r') {
			return Ok(contents.replace("\r\n", "\n").replace("\r", "\n"));
		}

		Ok(contents)
	}

	fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
		let mut paths = Vec::new();

		for entry in fs::read_dir(path)? {
			paths.push(entry?.path());
		}

		Ok(paths)
	}

	fn write(&mut self, path: &Path, contents: &[u8]) -> Result<()> {
		fs::write(path, contents)
	}

	fn create_dir(&mut self, path: &Path) -> Result<()> {
		fs::create_dir_all(path)
	}

	fn rename(&mut self, from: &Path, to: &Path) -> Result<()> {
		fs::rename(from, to)
	}

	fn remove(&mut self, path: &Path) -> Result<()> {
		self.unwatch(path)?;

		if Config::new().move_to_bin {
			trash::delete(path).map_err(Error::other)
		} else if path.is_dir() {
			fs::remove_dir_all(path)
		} else {
			fs::remove_file(path)
		}
	}

	fn exists(&self, path: &Path) -> bool {
		path.exists()
	}

	fn is_dir(&self, path: &Path) -> bool {
		path.is_dir()
	}

	fn is_file(&self, path: &Path) -> bool {
		path.is_file()
	}

	fn watch(&mut self, path: &Path, recursive: bool) -> Result<()> {
		let path = path.to_owned();

		if !self.watching || self.watched_paths.iter().any(|p| path.starts_with(p)) {
			return Ok(());
		}

		// a link itself is watched through its target only (watching both doubles every event)
		if !is_link(&path) {
			self.debouncer.watch(&path, recursive)?;
		}

		self.watch_links(&path, recursive);
		self.watched_paths.push(path);

		Ok(())
	}

	fn unwatch(&mut self, path: &Path) -> Result<()> {
		if !self.watching {
			return Ok(());
		}

		let path = path.to_owned();

		self.watched_paths.retain(|p| {
			let unwatch = p.starts_with(&path);

			if unwatch {
				self.debouncer.unwatch(p).ok();
			}

			!unwatch
		});

		self.links.write().unwrap().retain(|(link, target)| {
			let unwatch = link.starts_with(&path);

			if unwatch {
				self.debouncer.unwatch(target).ok();
			}

			!unwatch
		});

		Ok(())
	}

	fn pause(&mut self) {
		self.debouncer.pause()
	}

	fn resume(&mut self) {
		self.debouncer.resume()
	}

	fn receiver(&self) -> Receiver<VfsEvent> {
		self.debouncer.receiver()
	}
}
