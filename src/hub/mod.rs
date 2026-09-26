//! The Aragon hub: one local server every Studio session connects to.
//!
//! Studio's connector says hello with its place ID, the hub looks up which
//! project folder that place is mapped to, lazily spins up a sync `Core` for
//! it and serves it under `/p/{place}`. Unmapped places trigger onboarding in
//! the dashboard. The dashboard itself is a web app served by the same
//! server and shown in a native webview window.

use anyhow::{bail, Context, Result};
use log::{debug, error, info, warn};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
	collections::{BTreeMap, HashMap, HashSet, VecDeque},
	fs,
	path::{Path, PathBuf},
	process::Child,
	sync::{
		atomic::{AtomicBool, Ordering},
		Arc, Condvar, Mutex, MutexGuard,
	},
	thread,
	time::{Duration, Instant},
};

use self::{
	roblox::{Roblox, ThumbKind, UniverseInfo},
	store::{now, place_key, sanitize, Account, HubStore, OwnerRecord, PlaceRecord, PlaceSource},
};
use crate::{
	config::Config,
	core::Core,
	ext::PathExt,
	integration, lock,
	program::{Program, ProgramName},
	project::{self, Project},
	server, stats, studio,
	workspace::{self, WorkspaceConfig, WorkspaceLicense},
};

pub mod analytics;
pub mod api;
pub mod roblox;
pub mod secrets;
pub mod settings;
pub mod store;
pub mod ui;
pub mod window;

/// Default hub port, the Studio connector knocks here
pub const HUB_PORT: u16 = 7373;

const SESSION_TIMEOUT: Duration = Duration::from_secs(45);
const POLL_TIMEOUT: Duration = Duration::from_secs(20);
const CORE_IDLE_TIMEOUT: Duration = Duration::from_secs(180);

struct LoadedCore {
	core: Arc<Core>,
	project_path: PathBuf,
	last_used: Instant,
	children: Vec<Child>,
}

impl Drop for LoadedCore {
	fn drop(&mut self) {
		for child in &mut self.children {
			child.kill().ok();
		}
	}
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum SessionState {
	/// Place is mapped, the connector syncs with `/p/{place}`
	Mapped,
	/// Waiting for the user to finish onboarding in the dashboard
	Onboarding,
	/// Sync disabled for this place from the dashboard
	Paused,
	/// Project failed to load
	Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudioSession {
	pub client_id: u32,
	pub place_key: String,
	pub place_id: u64,
	pub game_id: u64,
	pub place_name: String,
	pub studio_user_id: Option<u64>,
	pub plugin_version: String,
	pub state: SessionState,
	pub message: Option<String>,
	pub connected_at: i64,
	#[serde(skip)]
	last_seen: Instant,
}

/// Commands the hub sends to a Studio connector through `/hub/poll`
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind")]
pub enum Control {
	Settings { settings: BTreeMap<String, Value> },
	Reconnect,
	Disconnect { message: String },
	Toast { message: String },
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HelloRequest {
	pub client_id: u32,
	pub place_id: u64,
	pub game_id: u64,
	pub place_name: String,
	pub creator_id: Option<u64>,
	pub creator_type: Option<String>,
	pub studio_user_id: Option<u64>,
	pub plugin_version: String,
	pub is_file: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloResponse {
	pub state: SessionState,
	pub place_key: String,
	pub base: String,
	pub message: Option<String>,
	pub project_name: Option<String>,
	pub settings: BTreeMap<String, Value>,
	pub initial_sync_priority: Option<String>,
	pub hub_version: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MapRequest {
	/// Workspace folder, the project file is created/resolved inside
	pub path: PathBuf,
	pub template: Option<String>,
	pub import_from_studio: bool,
	pub git: Option<bool>,
	pub wally: Option<bool>,
	pub selene: Option<bool>,
	pub docs: Option<bool>,
	pub license: Option<String>,
}

#[derive(Debug, Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshStatus {
	pub running: bool,
	pub stage: String,
	pub error: Option<String>,
}

pub struct Hub {
	pub port: u16,
	store: Mutex<HubStore>,
	cores: Mutex<HashMap<String, LoadedCore>>,
	sessions: Mutex<HashMap<u32, StudioSession>>,
	controls: Mutex<HashMap<u32, VecDeque<Control>>>,
	controls_changed: Condvar,
	version: Mutex<u64>,
	version_changed: Condvar,
	dismissed: Mutex<HashSet<String>>,
	resolving: Mutex<HashSet<String>>,
	refresh: Mutex<RefreshStatus>,
	studio_running: AtomicBool,
	attention: Mutex<Option<Box<dyn Fn() + Send + Sync>>>,
	dirty: AtomicBool,
	open_request: Mutex<Option<Value>>,
	studio_restart: Mutex<Option<String>>,
	roblox: Roblox,
}

fn now_millis() -> i64 {
	chrono::Utc::now().timestamp_millis()
}

impl Hub {
	pub fn new(port: u16) -> Arc<Self> {
		Arc::new(Self {
			port,
			store: Mutex::new(HubStore::load()),
			cores: Mutex::new(HashMap::new()),
			sessions: Mutex::new(HashMap::new()),
			controls: Mutex::new(HashMap::new()),
			controls_changed: Condvar::new(),
			version: Mutex::new(1),
			version_changed: Condvar::new(),
			dismissed: Mutex::new(HashSet::new()),
			resolving: Mutex::new(HashSet::new()),
			refresh: Mutex::new(RefreshStatus::default()),
			studio_running: AtomicBool::new(false),
			attention: Mutex::new(None),
			dirty: AtomicBool::new(false),
			open_request: Mutex::new(None),
			studio_restart: Mutex::new(None),
			roblox: Roblox::new(),
		})
	}

	pub fn store(&self) -> MutexGuard<'_, HubStore> {
		lock!(self.store)
	}

	/// Called whenever something needs the user's eyes (new place to onboard)
	pub fn set_attention(&self, callback: impl Fn() + Send + Sync + 'static) {
		*lock!(self.attention) = Some(Box::new(callback));
	}

	fn request_attention(&self) {
		if let Some(callback) = lock!(self.attention).as_ref() {
			callback();
		}
	}

	/// Marks the state as changed: wakes dashboard long-polls, saves soon
	pub fn bump(&self) {
		*lock!(self.version) += 1;
		self.version_changed.notify_all();
		self.dirty.store(true, Ordering::Relaxed);
	}

	pub fn version(&self) -> u64 {
		*lock!(self.version)
	}

	/// Blocks until the state version differs from `since` or the timeout hits
	pub fn wait_for_change(&self, since: u64, timeout: Duration) -> u64 {
		let version = lock!(self.version);

		let (version, _) = self
			.version_changed
			.wait_timeout_while(version, timeout, |version| *version == since)
			.unwrap();

		*version
	}

	pub fn save(&self) {
		if let Err(err) = self.store().save() {
			error!("Failed to save hub state: {err}");
		}
	}

	pub fn start_background(self: &Arc<Self>) {
		let hub = self.clone();

		thread::Builder::new()
			.name("hub-ticker".into())
			.spawn(move || {
				let mut ticks: u64 = 0;

				loop {
					thread::sleep(Duration::from_secs(5));
					ticks += 1;

					hub.expire_sessions();

					if ticks.is_multiple_of(3) {
						let running = studio::is_running(None).unwrap_or(false);

						if hub.studio_running.swap(running, Ordering::Relaxed) != running {
							hub.bump();
						}
					}

					if ticks.is_multiple_of(12) {
						hub.accrue_minutes();
						hub.reap_cores();
					}

					if ticks.is_multiple_of(6) {
						hub.scan_active();
					}

					if hub.dirty.swap(false, Ordering::Relaxed) {
						hub.save();
					}
				}
			})
			.unwrap();

		let hub = self.clone();

		thread::spawn(move || {
			hub.discover_projects();

			if secrets::get_api_key().is_some() || hub.store().studio_user_id.is_some() {
				let stale = hub
					.store()
					.account
					.as_ref()
					.and_then(|account| account.last_refresh)
					.is_none_or(|last| now() - last > 6 * 3600);

				if stale {
					if let Err(err) = hub.refresh_account() {
						warn!("Account refresh failed: {err}");
					}
				}
			}

			// Resolve any place we know by ID but not by universe yet
			let unresolved: Vec<String> = hub
				.store()
				.places
				.values()
				.filter(|place| place.place_id > 0 && place.universe_id.is_none())
				.map(|place| place.key.clone())
				.collect();

			for key in unresolved {
				hub.resolve_place(&key);
			}

			// First scan of every mapped project so analytics have a baseline
			let mapped: Vec<String> = hub
				.store()
				.places
				.values()
				.filter(|place| place.project.is_some())
				.map(|place| place.key.clone())
				.collect();

			for key in mapped {
				hub.scan_place(&key);
			}

			hub.bump();
		});
	}

	// Studio sessions ////////////////////////////////////////////////////////

	pub fn hello(self: &Arc<Self>, request: HelloRequest) -> HelloResponse {
		let key = place_key(request.place_id, &request.place_name);

		let is_new = {
			let mut store = self.store();
			let is_new = !store.places.contains_key(&key);

			let place = store
				.places
				.entry(key.clone())
				.or_insert_with(|| PlaceRecord::new(&key, request.place_id, PlaceSource::Connected));

			if !request.place_name.is_empty() && (place.name.is_empty() || !place.is_root) {
				place.name.clone_from(&request.place_name);
			}

			place.last_connected = Some(now());
			is_new
		};

		if let Some(user_id) = request.studio_user_id.filter(|id| *id > 0) {
			self.set_studio_user(user_id);
		}

		if is_new && request.place_id > 0 {
			let hub = self.clone();
			let key = key.clone();
			thread::spawn(move || hub.resolve_place(&key));
		}

		// A project may already exist at the suggested location
		if self.store().places[&key].project.is_none() {
			self.try_auto_map(&key);
		}

		let (state, message, project_name) = self.connect_state(&key);

		let initial_sync_priority = {
			let mut store = self.store();
			let place = store.places.get_mut(&key).unwrap();

			if state == SessionState::Mapped {
				place.first_sync.take()
			} else {
				None
			}
		};

		let settings = self.settings_for(&key);

		let session = StudioSession {
			client_id: request.client_id,
			place_key: key.clone(),
			place_id: request.place_id,
			game_id: request.game_id,
			place_name: request.place_name.clone(),
			studio_user_id: request.studio_user_id,
			plugin_version: request.plugin_version.clone(),
			state,
			message: message.clone(),
			connected_at: now(),
			last_seen: Instant::now(),
		};

		let was_known = lock!(self.sessions).insert(request.client_id, session).is_some();
		lock!(self.controls).entry(request.client_id).or_default();

		if state == SessionState::Onboarding
			&& !was_known
			&& !lock!(self.dismissed).contains(&key)
			&& self.store().prefs.focus_on_onboarding
		{
			self.request_attention();
		}

		if !was_known {
			info!("Studio connected: {} ({key}), state: {state:?}", request.place_name);
		}

		self.bump();

		HelloResponse {
			state,
			base: format!("/p/{key}"),
			place_key: key,
			message,
			project_name,
			settings,
			initial_sync_priority,
			hub_version: env!("CARGO_PKG_VERSION").to_owned(),
		}
	}

	fn connect_state(self: &Arc<Self>, key: &str) -> (SessionState, Option<String>, Option<String>) {
		let (project, sync_enabled) = {
			let store = self.store();
			let place = &store.places[key];
			(place.project.clone(), place.sync_enabled)
		};

		let Some(project) = project else {
			return (SessionState::Onboarding, None, None);
		};

		if !sync_enabled {
			return (
				SessionState::Paused,
				Some("Sync is paused for this place in the dashboard".into()),
				None,
			);
		}

		match self.ensure_core(key) {
			Ok(core) => (SessionState::Mapped, None, Some(core.name())),
			Err(err) => {
				error!("Failed to load project {project:?}: {err}");
				(SessionState::Error, Some(format!("{err:#}")), None)
			}
		}
	}

	/// Studio connector heartbeat + control channel (long-poll)
	pub fn poll(&self, client_id: u32) -> Option<Vec<Control>> {
		{
			let mut sessions = lock!(self.sessions);
			let session = sessions.get_mut(&client_id)?;
			session.last_seen = Instant::now();
		}

		let controls = lock!(self.controls);

		let (mut controls, _) = self
			.controls_changed
			.wait_timeout_while(controls, POLL_TIMEOUT, |controls| {
				controls.get(&client_id).is_some_and(|queue| queue.is_empty())
			})
			.unwrap();

		if let Some(session) = lock!(self.sessions).get_mut(&client_id) {
			session.last_seen = Instant::now();
		}

		Some(
			controls
				.get_mut(&client_id)
				.map(|queue| queue.drain(..).collect())
				.unwrap_or_default(),
		)
	}

	pub fn bye(&self, client_id: u32) {
		if let Some(session) = lock!(self.sessions).remove(&client_id) {
			info!("Studio disconnected: {}", session.place_name);
		}

		lock!(self.controls).remove(&client_id);
		self.bump();
	}

	fn push_control(&self, key: &str, control: Control) {
		let clients: Vec<u32> = lock!(self.sessions)
			.values()
			.filter(|session| session.place_key == key)
			.map(|session| session.client_id)
			.collect();

		let mut controls = lock!(self.controls);

		for client in clients {
			controls.entry(client).or_default().push_back(control.clone());
		}

		self.controls_changed.notify_all();
	}

	fn push_control_all(&self, control: impl Fn(&str) -> Control) {
		let clients: Vec<(u32, String)> = lock!(self.sessions)
			.values()
			.map(|session| (session.client_id, session.place_key.clone()))
			.collect();

		let mut controls = lock!(self.controls);

		for (client, key) in clients {
			controls.entry(client).or_default().push_back(control(&key));
		}

		self.controls_changed.notify_all();
	}

	/// Tells every Studio on this place to say hello again (after remapping etc.)
	pub fn reconnect(&self, key: &str, message: &str) {
		self.unload_core(key, message);
		self.push_control(key, Control::Reconnect);
	}

	fn expire_sessions(&self) {
		let expired: Vec<u32> = lock!(self.sessions)
			.values()
			.filter(|session| session.last_seen.elapsed() > SESSION_TIMEOUT)
			.map(|session| session.client_id)
			.collect();

		for client in expired {
			self.bye(client);
		}
	}

	pub fn sessions(&self) -> Vec<StudioSession> {
		lock!(self.sessions).values().cloned().collect()
	}

	pub fn settings_for(&self, key: &str) -> BTreeMap<String, Value> {
		let store = self.store();
		let overrides = store.places.get(key).map(|place| place.plugin_overrides.clone());

		settings::merged(&store.plugin_settings, &overrides.unwrap_or_default())
	}

	/// Pushes fresh plugin settings to every connected Studio
	pub fn broadcast_settings(&self) {
		let all: HashMap<String, BTreeMap<String, Value>> = lock!(self.sessions)
			.values()
			.map(|session| session.place_key.clone())
			.collect::<HashSet<_>>()
			.into_iter()
			.map(|key| {
				let settings = self.settings_for(&key);
				(key, settings)
			})
			.collect();

		self.push_control_all(|key| Control::Settings {
			settings: all.get(key).cloned().unwrap_or_default(),
		});
	}

	fn set_restart_stage(&self, stage: Option<&str>) {
		*lock!(self.studio_restart) = stage.map(str::to_owned);
		self.bump();
	}

	/// Closes Roblox Studio the polite way (like clicking X, so it still asks to
	/// save unsaved work), waits for it to exit, then starts it again so it
	/// loads the freshly installed plugin
	pub fn restart_studio(self: &Arc<Self>) {
		if lock!(self.studio_restart).is_some() {
			return;
		}

		let hub = self.clone();

		thread::spawn(move || {
			hub.set_restart_stage(Some("Closing Roblox Studio"));

			if let Err(err) = studio::close_gracefully() {
				warn!("Failed to ask Studio to close: {err}");
			}

			let started = Instant::now();

			while studio::is_process_running() {
				if started.elapsed() > Duration::from_secs(120) {
					// Most likely waiting on a "save changes?" dialog
					hub.set_restart_stage(None);
					return;
				}

				if started.elapsed() > Duration::from_secs(3) {
					hub.set_restart_stage(Some("Waiting for Studio to close (check for a save prompt)"));
				}

				thread::sleep(Duration::from_millis(500));
			}

			hub.set_restart_stage(Some("Starting Roblox Studio"));

			if let Err(err) = studio::launch(None) {
				warn!("Failed to start Studio: {err}");
			}

			thread::sleep(Duration::from_secs(4));
			hub.studio_running
				.store(studio::is_running(None).unwrap_or(false), Ordering::Relaxed);
			hub.set_restart_stage(None);
		});
	}

	/// Asks the dashboard to show a file in its editor and come to front
	pub fn request_open(&self, key: &str, path: &str) {
		*lock!(self.open_request) = Some(json!({
			"key": key,
			"path": path,
			"nonce": now_millis(),
		}));

		self.request_attention();
		self.bump();
	}

	pub fn dismiss(&self, key: &str) {
		lock!(self.dismissed).insert(key.to_owned());
		self.bump();
	}

	// Cores ///////////////////////////////////////////////////////////////////

	pub fn core_for(&self, key: &str) -> Option<Arc<Core>> {
		let mut cores = lock!(self.cores);
		let loaded = cores.get_mut(key)?;
		loaded.last_used = Instant::now();

		Some(loaded.core.clone())
	}

	pub fn ensure_core(self: &Arc<Self>, key: &str) -> Result<Arc<Core>> {
		let (project_path, sourcemap) = {
			let store = self.store();
			let place = store.places.get(key).context("Unknown place")?;
			(
				place.project.clone().context("Place is not mapped to a project")?,
				place.sourcemap,
			)
		};

		{
			let mut cores = lock!(self.cores);

			if let Some(loaded) = cores.get_mut(key) {
				if loaded.project_path == project_path {
					loaded.last_used = Instant::now();
					return Ok(loaded.core.clone());
				}

				cores.remove(key);
			}
		}

		if !project_path.exists() {
			bail!("Project file {} does not exist anymore", project_path.display());
		}

		let project = Project::load(&project_path)?;

		if !project.is_place() {
			bail!(
				"{} is not a place project (its root must be a DataModel)",
				project_path.display()
			);
		}

		let config = Config::new().clone();
		let workspace_dir = project.workspace_dir.clone();
		let use_ts = config.ts_mode || (config.detect_project && project.is_ts());

		if config.use_wally || (config.detect_project && project.is_wally()) {
			integration::check_wally_packages(&workspace_dir);
		}

		let mut children = Vec::new();

		if use_ts {
			if let Ok(Some(child)) = Program::new(ProgramName::Npx)
				.message("Failed to start roblox-ts")
				.current_dir(&workspace_dir)
				.arg("rbxtsc")
				.arg("--watch")
				.spawn()
			{
				children.push(child);
			}
		}

		let core = Arc::new(Core::new(project, true)?);

		// Studio's "Open In Editor" opens the file in the dashboard editor
		let hub = Arc::downgrade(self);
		let place = key.to_owned();
		let root = workspace_dir.clone();

		core.set_opener(Arc::new(move |path: &Path| {
			if let Some(hub) = hub.upgrade() {
				let relative = path
					.strip_prefix(&root)
					.unwrap_or(path)
					.to_string_lossy()
					.replace('\\', "/");
				hub.request_open(&place, &relative);
			}
		}));

		if sourcemap || config.with_sourcemap {
			let path = project_path.with_file_name("sourcemap.json");
			let queue = core.queue();
			let sourcemap_core = core.clone();

			queue.subscribe_internal().ok();
			core.sourcemap(Some(path.clone()), false).ok();

			// Holds a weak reference so an unloaded core can actually be dropped
			let weak = Arc::downgrade(&sourcemap_core);
			drop(sourcemap_core);

			thread::spawn(move || loop {
				let Some(core) = weak.upgrade() else {
					break;
				};

				let queue = core.queue();
				drop(core);

				if queue.get_change(0).is_err() {
					break;
				}

				match weak.upgrade() {
					Some(core) => {
						core.sourcemap(Some(path.clone()), false).ok();
					}
					None => break,
				}
			});
		}

		stats::sessions_started(1);

		lock!(self.cores).insert(
			key.to_owned(),
			LoadedCore {
				core: core.clone(),
				project_path,
				last_used: Instant::now(),
				children,
			},
		);

		info!("Loaded project for place {key}");
		self.bump();

		Ok(core)
	}

	pub fn unload_core(&self, key: &str, message: &str) {
		if let Some(loaded) = lock!(self.cores).remove(key) {
			loaded.core.queue().disconnect_all(message);
			info!("Unloaded project for place {key}");
			self.bump();
		}
	}

	fn reap_cores(&self) {
		let live: HashSet<String> = lock!(self.sessions)
			.values()
			.map(|session| session.place_key.clone())
			.collect();

		let idle: Vec<String> = lock!(self.cores)
			.iter()
			.filter(|(key, loaded)| {
				!live.contains(*key)
					&& loaded.core.queue().client_count() == 0
					&& loaded.last_used.elapsed() > CORE_IDLE_TIMEOUT
			})
			.map(|(key, _)| key.clone())
			.collect();

		for key in idle {
			self.unload_core(&key, "Idle");
		}
	}

	pub fn is_loaded(&self, key: &str) -> bool {
		lock!(self.cores).contains_key(key)
	}

	pub fn synced_clients(&self, key: &str) -> usize {
		lock!(self.cores)
			.get(key)
			.map_or(0, |loaded| loaded.core.queue().client_count())
	}

	// Mapping & onboarding ////////////////////////////////////////////////////

	/// Where a place lives by default: `<root>/<owner>/<game>[/<place>]`
	pub fn suggested_path(&self, key: &str) -> PathBuf {
		let store = self.store();
		let root = store.projects_root();

		let Some(place) = store.places.get(key) else {
			return root.join("Unsorted").join(sanitize(key));
		};

		let universe = place.universe_id.and_then(|id| store.universes.get(&id));

		match universe {
			Some(universe) => {
				let base = root
					.join(sanitize(&universe.creator_name))
					.join(sanitize(&universe.name));

				if place.is_root || universe.root_place_id == place.place_id {
					base
				} else {
					base.join("places").join(sanitize(&place.name))
				}
			}
			None if place.place_id == 0 => root.join("Local files").join(sanitize(&place.name)),
			None => root
				.join("Unsorted")
				.join(sanitize(if place.name.is_empty() { key } else { &place.name })),
		}
	}

	fn try_auto_map(&self, key: &str) {
		let suggested = self.suggested_path(key);

		let Ok(project_path) = project::resolve(suggested.clone()) else {
			return;
		};

		if project_path.exists() {
			info!("Auto-mapped place {key} to existing project {}", project_path.display());

			if let Some(place) = self.store().places.get_mut(key) {
				place.project = Some(project_path);
			}

			self.scan_place(key);
			self.bump();
		}
	}

	/// Finds existing projects under the projects root that declare place IDs
	fn discover_projects(&self) {
		let root = self.store().projects_root();
		let mut stack = vec![(root, 0)];
		let mut found = Vec::new();

		while let Some((dir, depth)) = stack.pop() {
			let Ok(entries) = fs::read_dir(&dir) else {
				continue;
			};

			for entry in entries.flatten() {
				let path = entry.path();

				if path.is_dir() {
					let name = path.get_name();

					if depth < 4 && !name.starts_with('.') && name != "node_modules" && name != "Packages" {
						stack.push((path, depth + 1));
					}
				} else if path.get_name().ends_with(".project.json") {
					found.push(path);
				}
			}
		}

		let mut store = self.store();

		for path in found {
			let Ok(contents) = fs::read_to_string(&path) else {
				continue;
			};

			let Ok(json) = serde_json::from_str::<Value>(&contents) else {
				continue;
			};

			let place_ids = json["placeIds"]
				.as_array()
				.or_else(|| json["servePlaceIds"].as_array())
				.cloned()
				.unwrap_or_default();

			for id in place_ids.iter().filter_map(Value::as_u64) {
				let key = id.to_string();

				let place = store
					.places
					.entry(key.clone())
					.or_insert_with(|| PlaceRecord::new(&key, id, PlaceSource::Manual));

				if place.project.is_none() {
					info!("Discovered project {} for place {id}", path.display());
					place.project = Some(path.clone());
				}
			}
		}
	}

	pub fn map_place(self: &Arc<Self>, key: &str, request: MapRequest) -> Result<PathBuf> {
		if !self.store().places.contains_key(key) {
			bail!("Unknown place {key}");
		}

		let workspace_dir = if request.path.as_os_str().is_empty() {
			self.suggested_path(key)
		} else {
			request.path.clone()
		};

		fs::create_dir_all(&workspace_dir).context("Failed to create the project folder")?;

		let mut project_path = project::resolve(workspace_dir.clone())?;

		if !project_path.exists() {
			let config = Config::new().clone();
			let template = request
				.template
				.clone()
				.unwrap_or_else(|| self.store().prefs.template.clone());

			project_path = workspace_dir.join("default.project.json");

			workspace::init(WorkspaceConfig {
				project: &project_path,
				template: &template,
				license: WorkspaceLicense {
					force: request.license.is_some(),
					inner: request.license.as_deref().unwrap_or(&config.license),
				},
				git: request.git.unwrap_or(config.use_git),
				wally: request.wally.unwrap_or(config.use_wally),
				selene: request.selene.unwrap_or(config.use_selene),
				docs: request.docs.unwrap_or(config.include_docs),
				rojo_mode: config.rojo_mode,
				use_lua: config.lua_extension,
			})?;

			stats::projects_created(1);
		}

		{
			let mut store = self.store();
			let place = store.places.get_mut(key).unwrap();

			place.project = Some(project_path.clone());
			// Onboarding picks the first sync direction explicitly, regardless
			// of the global InitialSyncPriority default
			place.first_sync = Some(String::from(if request.import_from_studio {
				"Client"
			} else {
				"Server"
			}));
			place.sync_enabled = true;
		}

		lock!(self.dismissed).remove(key);

		self.reconnect(key, "Project mapping changed");
		self.scan_place(key);
		self.save();
		self.bump();

		Ok(project_path)
	}

	pub fn unmap_place(&self, key: &str) {
		if let Some(place) = self.store().places.get_mut(key) {
			place.project = None;
		}

		self.reconnect(key, "Place was unmapped in the dashboard");
		self.save();
		self.bump();
	}

	pub fn add_place(self: &Arc<Self>, place_id: u64) -> String {
		let key = place_id.to_string();

		self.store()
			.places
			.entry(key.clone())
			.or_insert_with(|| PlaceRecord::new(&key, place_id, PlaceSource::Manual));

		let hub = self.clone();
		let resolve_key = key.clone();
		thread::spawn(move || hub.resolve_place(&resolve_key));

		self.bump();
		key
	}

	pub fn remove_place(&self, key: &str) {
		self.unload_core(key, "Place was removed from the dashboard");
		self.store().places.remove(key);
		self.save();
		self.bump();
	}

	// Roblox resolution ///////////////////////////////////////////////////////

	/// Resolves universe, owner and images of a single place
	pub fn resolve_place(&self, key: &str) {
		if !lock!(self.resolving).insert(key.to_owned()) {
			return;
		}

		let result = (|| -> Result<()> {
			let place_id = self.store().places.get(key).map(|place| place.place_id).unwrap_or(0);

			if place_id == 0 {
				return Ok(());
			}

			let universe_id = self.roblox.place_universe(place_id)?;
			let universe = self.roblox.universes(&[universe_id])?.pop();

			let mut targets = vec![(ThumbKind::GameIcon, universe_id), (ThumbKind::PlaceIcon, place_id)];

			if let Some(universe) = &universe {
				targets.push((ThumbKind::GameThumbnail, universe_id));
				targets.push((
					if universe.creator_type == "Group" {
						ThumbKind::GroupIcon
					} else {
						ThumbKind::AvatarHeadShot
					},
					universe.creator_id,
				));
			}

			let thumbs = self.roblox.thumbnails(&targets);
			let mut store = self.store();

			if let Some(universe) = universe {
				let owner_key = OwnerRecord::key(&universe.creator_type, universe.creator_id);
				let kind = if universe.creator_type == "Group" {
					ThumbKind::GroupIcon
				} else {
					ThumbKind::AvatarHeadShot
				};

				let owner = store.owners.entry(owner_key).or_insert_with(|| OwnerRecord {
					id: universe.creator_id,
					kind: universe.creator_type.clone(),
					name: universe.creator_name.clone(),
					..Default::default()
				});

				if owner.icon_url.is_none() {
					owner.icon_url = thumbs.get(&(kind, universe.creator_id)).cloned();
				}

				let is_root = universe.root_place_id == place_id;

				if let Some(place) = store.places.get_mut(key) {
					place.is_root = is_root;

					if is_root || place.name.is_empty() {
						place.name.clone_from(&universe.name);
					}
				}

				store.universes.insert(universe_id, universe);
			}

			if let Some(place) = store.places.get_mut(key) {
				place.universe_id = Some(universe_id);
				place.icon_url = thumbs
					.get(&(ThumbKind::GameIcon, universe_id))
					.or_else(|| thumbs.get(&(ThumbKind::PlaceIcon, place_id)))
					.cloned();
				place.thumbnail_url = thumbs.get(&(ThumbKind::GameThumbnail, universe_id)).cloned();
			}

			Ok(())
		})();

		lock!(self.resolving).remove(key);

		match result {
			Ok(()) => debug!("Resolved place {key}"),
			Err(err) => warn!("Failed to resolve place {key}: {err}"),
		}

		// Now that the owner and game are known the suggested folder may exist
		let unmapped = self
			.store()
			.places
			.get(key)
			.is_some_and(|place| place.project.is_none());

		if unmapped {
			self.try_auto_map(key);
		}

		self.bump();
	}

	pub fn refresh_status(&self) -> RefreshStatus {
		lock!(self.refresh).clone()
	}

	fn set_stage(&self, stage: &str) {
		let mut refresh = lock!(self.refresh);
		refresh.stage = stage.to_owned();
		drop(refresh);
		self.bump();
	}

	/// Pulls everything the user has: account, groups, games, places.
	/// Works from the Studio user ID alone (public data); an Open Cloud key
	/// adds the private experiences it can see.
	pub fn refresh_account(&self) -> Result<()> {
		{
			let mut refresh = lock!(self.refresh);

			if refresh.running {
				return Ok(());
			}

			*refresh = RefreshStatus {
				running: true,
				stage: "Looking you up".into(),
				error: None,
			};
		}

		self.bump();

		let result = self.refresh_account_inner();

		{
			let mut refresh = lock!(self.refresh);
			refresh.running = false;
			refresh.stage = String::new();
			refresh.error = result.as_ref().err().map(|err| format!("{err:#}"));
		}

		if let Err(err) = &result {
			if let Some(account) = self.store().account.as_mut() {
				account.last_error = Some(format!("{err:#}"));
			}
		}

		self.save();
		self.bump();

		result
	}

	/// Remembers who is using Studio and refreshes the account when it's new
	fn set_studio_user(self: &Arc<Self>, user_id: u64) {
		let (changed, stale) = {
			let mut store = self.store();
			let changed = store.studio_user_id != Some(user_id);
			store.studio_user_id = Some(user_id);

			let stale = store
				.account
				.as_ref()
				.and_then(|account| account.last_refresh)
				.is_none_or(|last| now() - last > 6 * 3600);

			(changed, stale)
		};

		if changed || stale {
			let hub = self.clone();

			thread::spawn(move || {
				if let Err(err) = hub.refresh_account() {
					warn!("Account refresh failed: {err}");
				}
			});
		}
	}

	fn refresh_account_inner(&self) -> Result<()> {
		let api_key = secrets::get_api_key();
		let mut key_error = None;

		let key_info = match &api_key {
			Some(api_key) => match self.roblox.introspect(api_key) {
				Ok(info) if info.expired => {
					key_error = Some(String::from(
						"Your API key has expired, create a new one on create.roblox.com",
					));
					None
				}
				Ok(info) if !info.enabled => {
					key_error = Some(String::from("Your API key is disabled, enable it on create.roblox.com"));
					None
				}
				Ok(info) => Some(info),
				Err(err) => {
					key_error = Some(format!("{err:#}"));
					None
				}
			},
			None => None,
		};

		let user_id = key_info
			.as_ref()
			.map(|info| info.user_id)
			.or(self.store().studio_user_id)
			.context("Open any place in Studio (or add an API key) so Aragon knows who you are")?;

		self.set_stage("Loading your account");

		let user = self
			.roblox
			.users(&[user_id])?
			.pop()
			.context("Roblox did not return the key owner")?;

		self.set_stage("Loading your groups");

		let all_groups = self.store().prefs.all_groups;
		let memberships = self.roblox.user_groups(user.id).unwrap_or_default();
		let manageable = match (&api_key, &key_info) {
			(Some(api_key), Some(_)) => self.roblox.manageable_groups(api_key).unwrap_or_default(),
			_ => Vec::new(),
		};
		let key_groups = key_info.as_ref().map(|info| info.group_ids.clone()).unwrap_or_default();
		let key_universes = key_info
			.as_ref()
			.map(|info| info.universe_ids.clone())
			.unwrap_or_default();

		let groups: Vec<_> = memberships
			.into_iter()
			.filter(|group| {
				all_groups || group.rank == 255 || manageable.contains(&group.id) || key_groups.contains(&group.id)
			})
			.collect();

		self.set_stage("Finding your games");

		let mut universe_ids = self.roblox.user_universes(user.id).unwrap_or_default();

		for group in &groups {
			match self.roblox.group_universes(group.id) {
				Ok(ids) => universe_ids.extend(ids),
				Err(err) => warn!("Failed to list games of group {}: {err}", group.name),
			}
		}

		universe_ids.extend(&key_universes);

		// Every place ever connected from Studio counts too: this is how private
		// user-owned games show up (Roblox can't list those without a cookie)
		universe_ids.extend(self.store().places.values().filter_map(|place| place.universe_id));
		universe_ids.sort_unstable();
		universe_ids.dedup();

		self.set_stage(&format!("Resolving {} games", universe_ids.len()));

		let universes = self.roblox.universes(&universe_ids)?;

		self.set_stage("Mapping places");

		let mut places = Vec::new();

		for universe in &universes {
			match self.roblox.universe_places(universe.id) {
				Ok(list) => places.extend(list),
				Err(_) => places.push(roblox::PlaceInfo {
					id: universe.root_place_id,
					universe_id: universe.id,
					name: universe.name.clone(),
				}),
			}
		}

		self.set_stage("Fetching images");

		let mut targets = vec![(ThumbKind::AvatarHeadShot, user.id)];
		targets.extend(groups.iter().map(|group| (ThumbKind::GroupIcon, group.id)));
		targets.extend(universes.iter().map(|universe| (ThumbKind::GameIcon, universe.id)));
		targets.extend(universes.iter().map(|universe| (ThumbKind::GameThumbnail, universe.id)));
		targets.extend(
			places
				.iter()
				.filter(|place| !universes.iter().any(|universe| universe.root_place_id == place.id))
				.map(|place| (ThumbKind::PlaceIcon, place.id)),
		);

		let thumbs = self.roblox.thumbnails(&targets);

		let mut store = self.store();

		store.account = Some(Account {
			avatar_url: thumbs.get(&(ThumbKind::AvatarHeadShot, user.id)).cloned(),
			key: key_info,
			user: user.clone(),
			last_refresh: Some(now()),
			last_error: key_error,
		});

		// Ownership flags are recomputed from scratch every refresh
		for owner in store.owners.values_mut() {
			owner.mine = false;
		}

		store.owners.insert(
			OwnerRecord::key("User", user.id),
			OwnerRecord {
				id: user.id,
				kind: "User".into(),
				name: user.name.clone(),
				icon_url: thumbs.get(&(ThumbKind::AvatarHeadShot, user.id)).cloned(),
				role: Some("You".into()),
				mine: true,
			},
		);

		for group in &groups {
			store.owners.insert(
				OwnerRecord::key("Group", group.id),
				OwnerRecord {
					id: group.id,
					kind: "Group".into(),
					name: group.name.clone(),
					icon_url: thumbs.get(&(ThumbKind::GroupIcon, group.id)).cloned(),
					role: Some(group.role.clone()),
					mine: true,
				},
			);
		}

		let universes: HashMap<u64, UniverseInfo> = universes.into_iter().map(|u| (u.id, u)).collect();

		for place in places {
			let Some(universe) = universes.get(&place.universe_id) else {
				continue;
			};

			let key = place.id.to_string();
			let is_root = universe.root_place_id == place.id;

			let record = store
				.places
				.entry(key.clone())
				.or_insert_with(|| PlaceRecord::new(&key, place.id, PlaceSource::Owned));

			if record.source != PlaceSource::Owned {
				record.source = PlaceSource::Owned;
			}

			record.universe_id = Some(universe.id);
			record.is_root = is_root;
			record.name = if is_root { universe.name.clone() } else { place.name };
			record.icon_url = thumbs
				.get(&(ThumbKind::GameIcon, universe.id))
				.or_else(|| thumbs.get(&(ThumbKind::PlaceIcon, place.id)))
				.cloned();
			record.thumbnail_url = thumbs.get(&(ThumbKind::GameThumbnail, universe.id)).cloned();
		}

		for (id, universe) in universes {
			store.universes.insert(id, universe);
		}

		// Places that were "owned" before but aren't anymore become plain history
		let owned_places: HashSet<u64> = store
			.universes
			.values()
			.filter(|universe| {
				store
					.owners
					.get(&OwnerRecord::key(&universe.creator_type, universe.creator_id))
					.is_some_and(|owner| owner.mine)
			})
			.map(|universe| universe.id)
			.collect();

		for place in store.places.values_mut() {
			if place.source == PlaceSource::Owned && !place.universe_id.is_some_and(|id| owned_places.contains(&id)) {
				place.source = PlaceSource::Connected;
			}
		}

		drop(store);

		// Owned games may already have folders on disk
		let unmapped: Vec<String> = self
			.store()
			.places
			.values()
			.filter(|place| place.project.is_none())
			.map(|place| place.key.clone())
			.collect();

		for key in unmapped {
			self.try_auto_map(&key);
		}

		Ok(())
	}

	// Analytics ///////////////////////////////////////////////////////////////

	pub fn scan_place(&self, key: &str) {
		let Some(project) = self.store().places.get(key).and_then(|place| place.project.clone()) else {
			return;
		};

		let workspace = project.get_parent().to_owned();
		let scan = analytics::scan(&workspace);

		if let Some(place) = self.store().places.get_mut(key) {
			let (added, removed) = analytics::apply(&mut place.stats, scan);

			if added + removed > 0 {
				stats::lines_synced(added as u32);
			}
		}

		self.bump();
	}

	fn scan_active(&self) {
		let active: HashSet<String> = lock!(self.sessions)
			.values()
			.filter(|session| session.state == SessionState::Mapped)
			.map(|session| session.place_key.clone())
			.chain(lock!(self.cores).keys().cloned())
			.collect();

		for key in active {
			self.scan_place(&key);
		}
	}

	fn accrue_minutes(&self) {
		let active: HashSet<String> = lock!(self.sessions)
			.values()
			.filter(|session| session.state == SessionState::Mapped)
			.map(|session| session.place_key.clone())
			.collect();

		if active.is_empty() {
			return;
		}

		let mut store = self.store();

		for key in active {
			if let Some(place) = store.places.get_mut(&key) {
				place.stats.minutes_connected += 1;
				place.stats.last_active = Some(now());
				place.stats.day().minutes += 1;
			}
		}

		drop(store);
		self.bump();
	}

	pub fn record_sync(&self, key: &str) {
		if let Some(place) = self.store().places.get_mut(key) {
			place.stats.syncs += 1;
			place.stats.day().syncs += 1;
			place.stats.last_active = Some(now());
		}

		self.dirty.store(true, Ordering::Relaxed);
	}

	// Snapshot for the dashboard //////////////////////////////////////////////

	pub fn snapshot(&self) -> Value {
		let sessions = self.sessions();
		let refresh = self.refresh_status();
		let dismissed = lock!(self.dismissed).clone();
		let loaded: HashSet<String> = lock!(self.cores).keys().cloned().collect();
		let synced: HashMap<String, usize> = lock!(self.cores)
			.iter()
			.map(|(key, loaded)| (key.clone(), loaded.core.queue().client_count()))
			.collect();

		let keys: Vec<String> = self.store().places.keys().cloned().collect();
		let suggested: HashMap<String, PathBuf> =
			keys.iter().map(|key| (key.clone(), self.suggested_path(key))).collect();

		let store = self.store();

		let places: Vec<Value> = store
			.places
			.values()
			.map(|place| {
				let universe = place.universe_id.and_then(|id| store.universes.get(&id));
				let owner_key = universe.map(|u| OwnerRecord::key(&u.creator_type, u.creator_id));
				let live = sessions.iter().filter(|s| s.place_key == place.key).count();
				let daily: Vec<Value> = place
					.stats
					.daily
					.iter()
					.rev()
					.take(90)
					.map(|(day, stats)| json!({ "day": day, "added": stats.added, "removed": stats.removed, "minutes": stats.minutes, "syncs": stats.syncs, "loc": stats.loc }))
					.collect();

				json!({
					"key": place.key,
					"placeId": place.place_id,
					"universeId": place.universe_id,
					"name": place.name,
					"gameName": universe.map(|u| u.name.clone()),
					"isRoot": place.is_root,
					"source": format!("{:?}", place.source),
					"iconUrl": place.icon_url,
					"thumbnailUrl": place.thumbnail_url,
					"ownerKey": owner_key,
					"ownerName": universe.map(|u| u.creator_name.clone()),
					"ownerType": universe.map(|u| u.creator_type.clone()),
					"privacy": universe.and_then(|u| u.privacy.clone()),
					"visits": universe.and_then(|u| u.visits),
					"playing": universe.and_then(|u| u.playing),
					"favorites": universe.and_then(|u| u.favorites),
					"updated": universe.and_then(|u| u.updated.clone()),
					"project": place.project,
					"workspace": place.project.as_ref().map(|p| p.get_parent().to_owned()),
					"projectExists": place.project.as_ref().is_some_and(|p| p.exists()),
					"suggestedPath": suggested.get(&place.key),
					"syncEnabled": place.sync_enabled,
					"sourcemap": place.sourcemap,
					"importPending": place.first_sync.as_deref() == Some("Client"),
					"pluginOverrides": place.plugin_overrides,
					"firstSeen": place.first_seen,
					"lastConnected": place.last_connected,
					"live": live,
					"loaded": loaded.contains(&place.key),
					"syncedClients": synced.get(&place.key).copied().unwrap_or(0),
					"resolving": place.place_id > 0 && place.universe_id.is_none(),
					"score": place.stats.score(),
					"stats": {
						"loc": place.stats.loc,
						"files": place.stats.files,
						"linesAdded": place.stats.lines_added,
						"linesRemoved": place.stats.lines_removed,
						"minutes": place.stats.minutes_connected,
						"syncs": place.stats.syncs,
						"lastActive": place.stats.last_active,
						"lastScan": place.stats.last_scan,
						"languages": place.stats.languages,
						"daily": daily,
					},
				})
			})
			.collect();

		let onboarding: Vec<&String> = sessions
			.iter()
			.filter(|session| session.state == SessionState::Onboarding && !dismissed.contains(&session.place_key))
			.map(|session| &session.place_key)
			.collect::<HashSet<_>>()
			.into_iter()
			.collect();

		let plugin_path = crate::util::get_plugin_path().ok();

		json!({
			"version": self.version(),
			"hub": {
				"port": self.port,
				"version": env!("CARGO_PKG_VERSION"),
				"studioRunning": self.studio_running.load(Ordering::Relaxed),
				"pluginInstalled": plugin_path.as_ref().is_some_and(|p| p.exists()),
				"pluginPath": plugin_path,
				"pluginBundled": crate::installer::get_plugin_version() != "0.0.0",
				"pluginBundledVersion": crate::installer::get_plugin_version(),
				"pluginInstalledVersion": crate::updater::get_status().ok().map(|status| status.plugin_version),
				"studioRestart": lock!(self.studio_restart).clone(),
			},
			"account": store.account.as_ref().map(|account| json!({
				"user": account.user,
				"avatarUrl": account.avatar_url,
				"keyName": account.key.as_ref().map(|key| key.name.clone()),
				"keyExpiration": account.key.as_ref().and_then(|key| key.expiration.clone()),
				"scopes": account.key.as_ref().map(|key| key.scopes.clone()).unwrap_or_default(),
				"lastRefresh": account.last_refresh,
				"lastError": account.last_error,
			})),
			"hasApiKey": secrets::get_api_key().is_some(),
			"studioUserId": store.studio_user_id,
			"refresh": refresh,
			"prefs": store.prefs,
			"projectsRoot": store.projects_root(),
			"owners": store.owners.iter().map(|(key, owner)| (key.clone(), json!(owner))).collect::<serde_json::Map<_, _>>(),
			"places": places,
			"sessions": sessions,
			"onboarding": onboarding,
			"openRequest": lock!(self.open_request).clone(),
			"pluginSettings": {
				"schema": settings::plugin_settings(),
				"global": store.plugin_settings,
			},
		})
	}

	pub fn start_server(self: &Arc<Self>) -> Result<()> {
		api::run(self.clone())
	}

	pub fn hub_url(&self) -> String {
		server::format_address("localhost", self.port)
	}

	pub fn workspace_of(&self, key: &str) -> Option<PathBuf> {
		self.store()
			.places
			.get(key)
			.and_then(|place| place.project.as_ref())
			.map(|project| project.get_parent().to_owned())
	}
}

/// Path is inside `root` (after resolving `..`), used by the file editor API
pub fn is_inside(root: &Path, path: &Path) -> bool {
	let root = path_clean::clean(root);
	let path = path_clean::clean(path);

	path.starts_with(root)
}
