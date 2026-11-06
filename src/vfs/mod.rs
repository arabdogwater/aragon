use crossbeam_channel::Receiver;
use std::{
	io::Result,
	path::{Path, PathBuf},
	sync::{Arc, Mutex, RwLock},
};

use self::{mem_backend::MemBackend, std_backend::StdBackend};
use crate::lock;

pub mod debouncer;
pub mod mem_backend;
pub mod std_backend;

#[derive(Debug, Clone)]
pub enum VfsEvent {
	Create(PathBuf),
	Delete(PathBuf),
	Write(PathBuf),
}

pub trait VfsBackend: Send {
	fn read(&self, path: &Path) -> Result<Vec<u8>>;
	fn read_to_string(&self, path: &Path) -> Result<String>;
	fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>>;

	fn write(&mut self, path: &Path, contents: &[u8]) -> Result<()>;
	fn create_dir(&mut self, path: &Path) -> Result<()>;
	fn rename(&mut self, from: &Path, to: &Path) -> Result<()>;
	fn remove(&mut self, path: &Path) -> Result<()>;

	fn exists(&self, path: &Path) -> bool;
	fn is_dir(&self, path: &Path) -> bool;
	fn is_file(&self, path: &Path) -> bool;

	fn watch(&mut self, path: &Path, recursive: bool) -> Result<()>;
	fn unwatch(&mut self, path: &Path) -> Result<()>;
	fn pause(&mut self);
	fn resume(&mut self);

	fn receiver(&self) -> Receiver<VfsEvent>;
}

impl VfsEvent {
	pub fn path(&self) -> &Path {
		match self {
			VfsEvent::Create(path) | VfsEvent::Delete(path) | VfsEvent::Write(path) => path.as_ref(),
		}
	}
}

/// What a write from Studio did to a file on disk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JournalKind {
	Created,
	Updated,
	Removed,
	Renamed,
}

/// Watches what syncing does to the disk. The hub uses it to keep a copy of every file Studio overwrites or
/// deletes (the safety net) and to show live sync activity. Every call happens on the sync thread, before the
/// lock on the file system is taken, so an implementation may read the files freely
pub trait VfsJournal: Send + Sync {
	/// `path` exists and is about to be overwritten with `contents`
	fn before_write(&self, path: &Path, contents: &[u8]);
	/// `path` (a file or a whole folder) is about to be deleted
	fn before_remove(&self, path: &Path);
	/// A write from Studio reached the disk
	fn written(&self, path: &Path, kind: JournalKind);
	/// A change made on disk (an editor, an AI agent) was sent to Studio
	fn incoming(&self, path: &Path, changes: usize);
}

pub struct Vfs {
	inner: Mutex<Box<dyn VfsBackend>>,
	journal: RwLock<Option<Arc<dyn VfsJournal>>>,
}

impl Vfs {
	pub fn new(watch: bool) -> Self {
		Self {
			inner: Mutex::new(Box::new(StdBackend::new(watch))),
			journal: RwLock::new(None),
		}
	}

	pub fn new_virtual() -> Self {
		Self {
			inner: Mutex::new(Box::new(MemBackend::new())),
			journal: RwLock::new(None),
		}
	}

	pub fn read(&self, path: &Path) -> Result<Vec<u8>> {
		lock!(self.inner).read(path)
	}

	pub fn read_to_string(&self, path: &Path) -> Result<String> {
		lock!(self.inner).read_to_string(path)
	}

	pub fn read_dir(&self, path: &Path) -> Result<Vec<PathBuf>> {
		lock!(self.inner).read_dir(path)
	}

	pub fn set_journal(&self, journal: Arc<dyn VfsJournal>) {
		*self.journal.write().unwrap() = Some(journal);
	}

	fn journal(&self) -> Option<Arc<dyn VfsJournal>> {
		self.journal.read().unwrap().clone()
	}

	/// Reports a change made on disk that was synced to Studio
	pub fn journal_incoming(&self, path: &Path, changes: usize) {
		if let Some(journal) = self.journal() {
			journal.incoming(path, changes);
		}
	}

	pub fn write(&self, path: &Path, contents: &[u8]) -> Result<()> {
		let Some(journal) = self.journal() else {
			return lock!(self.inner).write(path, contents);
		};

		let existed = self.is_file(path);

		if existed {
			journal.before_write(path, contents);
		}

		lock!(self.inner).write(path, contents)?;
		journal.written(path, if existed { JournalKind::Updated } else { JournalKind::Created });

		Ok(())
	}

	pub fn create_dir(&self, path: &Path) -> Result<()> {
		lock!(self.inner).create_dir(path)
	}

	pub fn rename(&self, from: &Path, to: &Path) -> Result<()> {
		lock!(self.inner).rename(from, to)?;

		if let Some(journal) = self.journal() {
			journal.written(to, JournalKind::Renamed);
		}

		Ok(())
	}

	pub fn remove(&self, path: &Path) -> Result<()> {
		let journal = self.journal();

		if let Some(journal) = &journal {
			journal.before_remove(path);
		}

		lock!(self.inner).remove(path)?;

		if let Some(journal) = &journal {
			journal.written(path, JournalKind::Removed);
		}

		Ok(())
	}

	pub fn exists(&self, path: &Path) -> bool {
		lock!(self.inner).exists(path)
	}

	pub fn is_dir(&self, path: &Path) -> bool {
		lock!(self.inner).is_dir(path)
	}

	pub fn is_file(&self, path: &Path) -> bool {
		lock!(self.inner).is_file(path)
	}

	pub fn watch(&self, path: &Path, recursive: bool) -> Result<()> {
		lock!(self.inner).watch(path, recursive)
	}

	pub fn unwatch(&self, path: &Path) -> Result<()> {
		lock!(self.inner).unwatch(path)
	}

	pub fn pause(&self) {
		lock!(self.inner).pause()
	}

	pub fn resume(&self) {
		lock!(self.inner).resume()
	}

	pub fn receiver(&self) -> Receiver<VfsEvent> {
		lock!(self.inner).receiver()
	}
}
