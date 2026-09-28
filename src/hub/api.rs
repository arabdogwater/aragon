//! HTTP surface of the hub:
//! - `/hub/*`      Studio connector (msgpack): hello, poll, bye
//! - `/p/{place}/*` Studio sync endpoints of a mapped place (msgpack)
//! - `/api/*`      dashboard (JSON)
//! - `/ui/*`       the dashboard web app itself

use actix_msgpack::{MsgPack, MsgPackResponseBuilder};
use actix_web::{
	delete, get, post, put,
	web::{self, Data, Json, Path as UrlPath, Query},
	App, HttpResponse, HttpServer, Responder,
};
use anyhow::Result;
use documented::DocumentedFields;
use log::info;
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
	fs,
	hash::{Hash, Hasher},
	path::{Path, PathBuf},
	sync::Arc,
	thread,
	time::{Duration, Instant},
};

use super::{is_inside, secrets, settings, ui, Hub, MapRequest};
use crate::{
	config::{Config, ConfigKind},
	core::Core,
	ext::PathExt,
	installer,
	program::{Program, ProgramName},
	project::Project,
	server::{self, AuthRequest, ExecuteCode},
	util,
};

type HubData = Data<Arc<Hub>>;

fn error(err: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::BadRequest().json(json!({ "error": format!("{err:#}") }))
}

fn ok() -> HttpResponse {
	HttpResponse::Ok().json(json!({ "ok": true }))
}

async fn blocking<T, F>(f: F) -> std::result::Result<T, HttpResponse>
where
	F: FnOnce() -> T + Send + 'static,
	T: Send + 'static,
{
	web::block(f).await.map_err(error)
}

// Studio connector //////////////////////////////////////////////////////////

#[post("/hub/hello")]
async fn hello(request: MsgPack<super::HelloRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	match blocking(move || hub.hello(request.0)).await {
		Ok(response) => HttpResponse::Ok().msgpack(response),
		Err(response) => response,
	}
}

#[post("/hub/poll")]
async fn poll(request: MsgPack<AuthRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let id = request.client_id;

	match blocking(move || hub.poll(id)).await {
		Ok(Some(controls)) => HttpResponse::Ok().msgpack(controls),
		Ok(None) => HttpResponse::Unauthorized().body("Unknown client, say hello first"),
		Err(response) => response,
	}
}

#[post("/hub/bye")]
async fn bye(request: MsgPack<AuthRequest>, hub: HubData) -> impl Responder {
	hub.bye(request.client_id);
	HttpResponse::Ok().body("Bye")
}

#[post("/hub/synced")]
async fn synced(request: MsgPack<SyncedRequest>, hub: HubData) -> impl Responder {
	hub.record_sync(&request.place_key);
	HttpResponse::Ok().body("Recorded")
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncedRequest {
	place_key: String,
}

// Dashboard: state //////////////////////////////////////////////////////////

#[derive(Deserialize)]
struct StateQuery {
	v: Option<u64>,
	/// Content hash of the state the dashboard already shows
	h: Option<String>,
}

/// Hash of everything the dashboard renders (the version counter excluded)
fn content_hash(snapshot: &mut Value) -> String {
	let version = snapshot.as_object_mut().and_then(|object| object.remove("version"));

	let mut hasher = std::collections::hash_map::DefaultHasher::new();
	snapshot.to_string().hash(&mut hasher);

	if let (Some(object), Some(version)) = (snapshot.as_object_mut(), version) {
		object.insert("version".into(), version);
	}

	format!("{:016x}", hasher.finish())
}

#[get("/api/ping")]
async fn ping() -> impl Responder {
	HttpResponse::Ok().json(json!({ "app": "aragon", "version": env!("CARGO_PKG_VERSION") }))
}

/// Long-polls while the dashboard already has the current state. Bumps that
/// don't change anything the dashboard renders (a background re-scan that found
/// nothing new, a connector re-checking in) keep waiting instead of making the
/// dashboard re-render an identical page
#[get("/api/state")]
async fn state(query: Query<StateQuery>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let mut since = query.v;
	let known = query.h.clone();

	match blocking(move || {
		let deadline = Instant::now() + Duration::from_secs(25);

		loop {
			if let Some(version) = since {
				let left = deadline.saturating_duration_since(Instant::now());

				if !left.is_zero() {
					since = Some(hub.wait_for_change(version, left));
				}
			}

			let mut snapshot = hub.snapshot();
			let hash = content_hash(&mut snapshot);

			if since.is_none() || known.as_deref() != Some(hash.as_str()) || Instant::now() >= deadline {
				snapshot["hash"] = json!(hash);
				return snapshot;
			}
		}
	})
	.await
	{
		Ok(snapshot) => HttpResponse::Ok().json(snapshot),
		Err(response) => response,
	}
}

#[post("/api/focus")]
async fn focus(hub: HubData) -> impl Responder {
	hub.request_attention();
	ok()
}

// Dashboard: account ////////////////////////////////////////////////////////

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct KeyRequest {
	api_key: String,
}

#[post("/api/account/key")]
async fn set_key(request: Json<KeyRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = request.api_key.trim().to_owned();

	if key.len() < 20 {
		return error("That doesn't look like an Open Cloud API key");
	}

	let result = blocking(move || -> Result<()> {
		// Validate before storing so a typo never replaces a working key
		let roblox = super::roblox::Roblox::new();
		roblox.introspect(&key)?;

		secrets::set_api_key(&key)?;
		hub.refresh_account()
	})
	.await;

	match result {
		Ok(Ok(())) => ok(),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

#[delete("/api/account/key")]
async fn delete_key(hub: HubData) -> impl Responder {
	if let Err(err) = secrets::delete_api_key() {
		return error(err);
	}

	// Keep the public account data, just drop what only the key could see
	let hub = hub.get_ref().clone();
	thread::spawn(move || hub.refresh_account().ok());

	ok()
}

#[post("/api/account/refresh")]
async fn refresh(hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	thread::spawn(move || hub.refresh_account().ok());

	ok()
}

// Dashboard: places /////////////////////////////////////////////////////////

#[post("/api/places/{key}/map")]
async fn map_place(key: UrlPath<String>, request: Json<MapRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = key.into_inner();

	match blocking(move || hub.map_place(&key, request.into_inner())).await {
		Ok(Ok(path)) => HttpResponse::Ok().json(json!({ "ok": true, "project": path })),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

#[post("/api/places/{key}/unmap")]
async fn unmap_place(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.unmap_place(&key);
	ok()
}

#[post("/api/places/{key}/dismiss")]
async fn dismiss(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.dismiss(&key);
	ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AddPlaceRequest {
	place_id: u64,
}

#[post("/api/places")]
async fn add_place(request: Json<AddPlaceRequest>, hub: HubData) -> impl Responder {
	if request.place_id == 0 {
		return error("Enter a valid place ID");
	}

	let key = hub.add_place(request.place_id);
	HttpResponse::Ok().json(json!({ "ok": true, "key": key }))
}

#[delete("/api/places/{key}")]
async fn remove_place(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.remove_place(&key);
	ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlaceOptions {
	sync_enabled: Option<bool>,
	sourcemap: Option<bool>,
	/// Setting -> value, `null` removes the override
	plugin_overrides: Option<serde_json::Map<String, Value>>,
}

#[post("/api/places/{key}/options")]
async fn place_options(key: UrlPath<String>, request: Json<PlaceOptions>, hub: HubData) -> impl Responder {
	let key = key.into_inner();
	let mut reconnect = false;

	{
		let mut store = hub.store();

		let Some(place) = store.places.get_mut(&key) else {
			return error("Unknown place");
		};

		if let Some(enabled) = request.sync_enabled {
			reconnect |= place.sync_enabled != enabled;
			place.sync_enabled = enabled;
		}

		if let Some(sourcemap) = request.sourcemap {
			reconnect |= place.sourcemap != sourcemap;
			place.sourcemap = sourcemap;
		}

		if let Some(overrides) = &request.plugin_overrides {
			for (setting, value) in overrides {
				if value.is_null() {
					place.plugin_overrides.remove(setting);
				} else if let Some(value) = settings::coerce(setting, value) {
					place.plugin_overrides.insert(setting.clone(), value);
				}
			}
		}
	}

	if reconnect {
		hub.reconnect(&key, "Place options changed");
	}

	hub.broadcast_settings();
	hub.save();
	hub.bump();

	ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionRequest {
	action: String,
	code: Option<String>,
}

#[post("/api/places/{key}/action")]
async fn place_action(key: UrlPath<String>, request: Json<ActionRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = key.into_inner();
	let request = request.into_inner();

	let result = blocking(move || -> Result<Value> {
		let workspace = hub.workspace_of(&key);
		let place = hub.store().places.get(&key).cloned();
		let Some(place) = place else {
			anyhow::bail!("Unknown place");
		};

		match request.action.as_str() {
			"openFolder" => {
				open::that(workspace.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?)?;
			}
			"openStudio" => {
				anyhow::ensure!(place.place_id > 0, "Local files can't be opened by ID");

				let universe = place.universe_id.unwrap_or_default();
				open::that(format!(
					"roblox-studio:1+launchmode:edit+task:EditPlace+placeId:{}+universeId:{}",
					place.place_id, universe
				))?;
			}
			"openBrowser" => {
				anyhow::ensure!(place.place_id > 0, "Local files have no web page");
				open::that(format!("https://www.roblox.com/games/{}", place.place_id))?;
			}
			"openCreatorHub" => {
				let universe = place
					.universe_id
					.ok_or_else(|| anyhow::anyhow!("Game is not resolved yet"))?;
				open::that(format!(
					"https://create.roblox.com/dashboard/creations/experiences/{universe}/overview"
				))?;
			}
			"reconnect" => hub.reconnect(&key, "Reconnect requested from the dashboard"),
			"import" => {
				if let Some(place) = hub.store().places.get_mut(&key) {
					place.first_sync = Some(String::from("Client"));
				}

				hub.reconnect(&key, "Importing from Studio");
			}
			"rescan" => hub.scan_place(&key),
			"resolve" => hub.resolve_place(&key),
			"build" => {
				let project_path = place
					.project
					.clone()
					.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;
				let core = match hub.core_for(&key) {
					Some(core) => core,
					None => std::sync::Arc::new(Core::new(Project::load(&project_path)?, false)?),
				};

				let workspace = project_path.get_parent().to_owned();
				let output = workspace.join("build").join(format!("{}.rbxl", core.name()));

				fs::create_dir_all(output.get_parent())?;
				core.build(&output, false)?;

				return Ok(json!({ "ok": true, "output": output }));
			}
			"wally" => {
				let workspace = workspace.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;

				Program::new(ProgramName::Wally)
					.message("Failed to run Wally")
					.current_dir(&workspace)
					.arg("install")
					.output()?;
			}
			"exec" => {
				let code = request.code.unwrap_or_default();
				let core = hub
					.core_for(&key)
					.ok_or_else(|| anyhow::anyhow!("No Studio is syncing this place right now"))?;

				core.queue().push(ExecuteCode { code }, None)?;
			}
			other => anyhow::bail!("Unknown action {other}"),
		}

		hub.bump();
		Ok(json!({ "ok": true }))
	})
	.await;

	match result {
		Ok(Ok(value)) => HttpResponse::Ok().json(value),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

// Dashboard: project files (built-in editor) ////////////////////////////////

const HIDDEN_DIRS: [&str; 5] = [".git", "node_modules", "Packages", "ServerPackages", "DevPackages"];
const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024;

fn list_tree(root: &Path, dir: &Path, depth: usize, budget: &mut usize) -> Vec<Value> {
	let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
	entries.sort_by_key(|entry| (!entry.path().is_dir(), entry.file_name().to_ascii_lowercase()));

	let mut result = Vec::new();

	for entry in entries {
		if *budget == 0 {
			break;
		}

		*budget -= 1;

		let path = entry.path();
		let name = entry.file_name().to_string_lossy().to_string();
		let relative = path
			.strip_prefix(root)
			.unwrap_or(&path)
			.to_string_lossy()
			.replace('\\', "/");

		if path.is_dir() {
			let hidden = HIDDEN_DIRS.contains(&name.as_str());
			let children = if hidden || depth > 12 {
				vec![]
			} else {
				list_tree(root, &path, depth + 1, budget)
			};

			result.push(
				json!({ "name": name, "path": relative, "dir": true, "collapsed": hidden, "children": children }),
			);
		} else {
			result.push(json!({ "name": name, "path": relative, "dir": false }));
		}
	}

	result
}

fn resolve_file(hub: &Hub, key: &str, relative: &str) -> Result<(PathBuf, PathBuf)> {
	let workspace = hub
		.workspace_of(key)
		.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;
	let path = workspace.join(relative);

	anyhow::ensure!(
		!relative.is_empty() && is_inside(&workspace, &path),
		"Path escapes the project folder"
	);

	Ok((workspace, path))
}

#[get("/api/places/{key}/tree")]
async fn file_tree(key: UrlPath<String>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	let result = blocking(move || {
		let workspace = hub.workspace_of(&key)?;
		let mut budget = 5000;

		Some(json!({ "root": workspace, "children": list_tree(&workspace, &workspace, 0, &mut budget) }))
	})
	.await;

	match result {
		Ok(Some(tree)) => HttpResponse::Ok().json(tree),
		Ok(None) => error("Place is not mapped"),
		Err(response) => response,
	}
}

#[derive(Deserialize)]
struct FileQuery {
	path: String,
}

#[get("/api/places/{key}/file")]
async fn read_file(key: UrlPath<String>, query: Query<FileQuery>, hub: HubData) -> impl Responder {
	let (_, path) = match resolve_file(&hub, &key, &query.path) {
		Ok(paths) => paths,
		Err(err) => return error(err),
	};

	match fs::metadata(&path) {
		Ok(meta) if meta.len() > MAX_FILE_SIZE => error("File is too large to edit here"),
		Ok(_) => match fs::read(&path) {
			Ok(bytes) => match String::from_utf8(bytes) {
				Ok(text) => HttpResponse::Ok().json(json!({ "path": query.path, "content": text })),
				Err(_) => error("Binary file"),
			},
			Err(err) => error(err),
		},
		Err(err) => error(err),
	}
}

#[derive(Deserialize)]
struct WriteFile {
	content: String,
}

#[put("/api/places/{key}/file")]
async fn write_file(
	key: UrlPath<String>,
	query: Query<FileQuery>,
	request: Json<WriteFile>,
	hub: HubData,
) -> impl Responder {
	let (_, path) = match resolve_file(&hub, &key, &query.path) {
		Ok(paths) => paths,
		Err(err) => return error(err),
	};

	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).ok();
	}

	match fs::write(&path, &request.content) {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

#[delete("/api/places/{key}/file")]
async fn delete_file(key: UrlPath<String>, query: Query<FileQuery>, hub: HubData) -> impl Responder {
	let (_, path) = match resolve_file(&hub, &key, &query.path) {
		Ok(paths) => paths,
		Err(err) => return error(err),
	};

	// Deleted files go to the OS trash so nothing is lost for good
	match trash::delete(&path) {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

// Dashboard: settings ///////////////////////////////////////////////////////

#[get("/api/config")]
async fn get_config() -> impl Responder {
	let defaults = Config::default();
	let config = Config::new();

	let settings: Vec<Value> = (&defaults)
		.into_iter()
		.filter_map(|(key, default)| {
			let doc = Config::get_field_docs(key).ok()?.trim().to_owned();
			let value = config.get(key)?;

			Some(json!({ "key": key, "default": default, "value": value, "doc": doc }))
		})
		.collect();

	HttpResponse::Ok().json(json!({ "settings": settings, "path": config.kind().path() }))
}

#[derive(Deserialize)]
struct ConfigRequest {
	key: String,
	value: Value,
}

#[post("/api/config")]
async fn set_config(request: Json<ConfigRequest>) -> impl Responder {
	let value = match &request.value {
		Value::String(value) => value.clone(),
		value => value.to_string(),
	};

	let result = (|| -> Result<()> {
		let path = util::get_aragon_dir()?.join("config.toml");
		let mut config = Config::new_mut();

		anyhow::ensure!(config.has_setting(&request.key), "Unknown setting {}", request.key);

		config
			.set(&request.key, &value)
			.map_err(|err| anyhow::anyhow!("Invalid value: {err}"))?;
		config.save(&path)?;

		Ok(())
	})();

	match result {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PluginSettingRequest {
	key: String,
	/// `null` restores the default
	value: Value,
}

#[post("/api/plugin-settings")]
async fn set_plugin_setting(request: Json<PluginSettingRequest>, hub: HubData) -> impl Responder {
	{
		let mut store = hub.store();

		if request.value.is_null() {
			store.plugin_settings.remove(&request.key);
		} else {
			match settings::coerce(&request.key, &request.value) {
				Some(value) => {
					store.plugin_settings.insert(request.key.clone(), value);
				}
				None => return error("Invalid value for this setting"),
			}
		}
	}

	hub.broadcast_settings();
	hub.save();
	hub.bump();

	ok()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrefsRequest {
	projects_root: Option<PathBuf>,
	all_groups: Option<bool>,
	focus_on_onboarding: Option<bool>,
	template: Option<String>,
	welcomed: Option<bool>,
}

#[post("/api/prefs")]
async fn set_prefs(request: Json<PrefsRequest>, hub: HubData) -> impl Responder {
	{
		let mut store = hub.store();

		if let Some(root) = &request.projects_root {
			store.projects_root = Some(root.clone());
		}

		if let Some(all_groups) = request.all_groups {
			store.prefs.all_groups = all_groups;
		}

		if let Some(focus_on_onboarding) = request.focus_on_onboarding {
			store.prefs.focus_on_onboarding = focus_on_onboarding;
		}

		if let Some(template) = &request.template {
			store.prefs.template.clone_from(template);
		}

		if let Some(welcomed) = request.welcomed {
			store.prefs.welcomed = welcomed;
		}
	}

	hub.save();
	hub.bump();

	ok()
}

#[get("/api/templates")]
async fn templates() -> impl Responder {
	let templates: Vec<String> = util::get_aragon_dir()
		.ok()
		.and_then(|dir| fs::read_dir(dir.join("templates")).ok())
		.into_iter()
		.flatten()
		.flatten()
		.filter(|entry| entry.path().is_dir())
		.map(|entry| entry.file_name().to_string_lossy().to_string())
		.filter(|name| name != "plugin" && name != "model" && name != "package")
		.collect();

	HttpResponse::Ok().json(templates)
}

#[derive(Deserialize)]
struct PickFolderRequest {
	start: Option<PathBuf>,
}

#[post("/api/pick-folder")]
async fn pick_folder(request: Json<PickFolderRequest>) -> impl Responder {
	let start = request.start.clone();

	let result = blocking(move || {
		let mut dialog = rfd::FileDialog::new().set_title("Choose a folder");

		if let Some(start) = start.filter(|path| path.exists()) {
			dialog = dialog.set_directory(start);
		}

		dialog.pick_folder()
	})
	.await;

	match result {
		Ok(path) => HttpResponse::Ok().json(json!({ "path": path })),
		Err(response) => response,
	}
}

#[post("/api/plugin/install")]
async fn install_plugin(hub: HubData) -> impl Responder {
	let result = util::get_plugin_path().and_then(|path| installer::install_plugin(&path, false));
	hub.bump();

	match result {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

#[post("/api/studio/restart")]
async fn restart_studio(hub: HubData) -> impl Responder {
	hub.restart_studio();
	ok()
}

#[post("/api/studio/open")]
async fn open_studio(hub: HubData) -> impl Responder {
	match crate::studio::launch(None) {
		Ok(()) => {
			hub.bump();
			ok()
		}
		Err(err) => error(err),
	}
}

#[derive(Deserialize)]
struct OpenRequest {
	target: String,
}

/// Opens a folder or an allow-listed URL with the OS
#[post("/api/open")]
async fn open_target(request: Json<OpenRequest>) -> impl Responder {
	let target = request.target.trim();

	let allowed = target.starts_with("https://create.roblox.com/")
		|| target.starts_with("https://www.roblox.com/")
		|| target.starts_with("https://argon.wiki/")
		|| Path::new(target).is_dir();

	if !allowed {
		return error("Refusing to open that target");
	}

	match open::that(target) {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

pub fn run(hub: Arc<Hub>) -> Result<()> {
	let port = hub.port;

	Config::load_virtual(ConfigKind::Default).ok();

	info!("Aragon hub listening on {}", server::format_address("localhost", port));

	actix_web::rt::System::new().block_on(async move {
		HttpServer::new(move || {
			App::new()
				.app_data(Data::new(hub.clone()))
				.app_data(server::msgpack_config())
				.app_data(web::JsonConfig::default().limit(8 * 1024 * 1024))
				.service(hello)
				.service(poll)
				.service(bye)
				.service(synced)
				.service(ping)
				.service(state)
				.service(focus)
				.service(set_key)
				.service(delete_key)
				.service(refresh)
				.service(add_place)
				.service(remove_place)
				.service(map_place)
				.service(unmap_place)
				.service(dismiss)
				.service(place_options)
				.service(place_action)
				.service(file_tree)
				.service(read_file)
				.service(write_file)
				.service(delete_file)
				.service(get_config)
				.service(set_config)
				.service(set_plugin_setting)
				.service(set_prefs)
				.service(templates)
				.service(pick_folder)
				.service(install_plugin)
				.service(restart_studio)
				.service(open_studio)
				.service(open_target)
				.service(web::scope("/p/{place}").configure(server::configure_sync))
				.configure(ui::configure)
		})
		.workers(4)
		.disable_signals()
		.bind(("localhost", port))?
		.run()
		.await
	})?;

	Ok(())
}
