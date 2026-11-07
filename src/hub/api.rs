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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct OutputRequest {
	place_id: u64,
	#[serde(default)]
	place_name: String,
	#[serde(default)]
	context: String,
	messages: Vec<super::journal::OutputMessage>,
}

/// Studio's Output window, forwarded by the plugin
#[post("/hub/output")]
async fn studio_output(request: MsgPack<OutputRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let request = request.0;

	let record = move || {
		hub.record_output(
			request.place_id,
			&request.place_name,
			&request.context,
			request.messages,
		)
	};

	match blocking(record).await {
		Ok(()) => HttpResponse::Ok().body("Recorded"),
		Err(response) => response,
	}
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

#[post("/api/places/{key}/restore")]
async fn restore_place(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.restore_place(&key);
	ok()
}

#[delete("/api/removed/{key}")]
async fn forget_removed(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.forget_removed(&key);
	ok()
}

// Dashboard: grouping the Places page ///////////////////////////////////////

#[derive(Deserialize)]
struct ArrangeRequest {
	/// Owner key or custom group id the places are now in
	group: String,
	/// Every place of that group, in the order the user wants
	keys: Vec<String>,
}

/// Drag and drop on the Places page: puts places in a group, in this order
#[post("/api/places/arrange")]
async fn arrange_places(request: Json<ArrangeRequest>, hub: HubData) -> impl Responder {
	{
		let mut store = hub.store();

		let known = store.groups.iter().any(|g| g.id == request.group) || store.owners.contains_key(&request.group);
		let natural = request.group == "unsorted" || request.group == "local";

		if !known && !natural {
			return error("Unknown group");
		}

		for (index, key) in request.keys.iter().enumerate() {
			if let Some(place) = store.places.get_mut(key) {
				place.group = Some(request.group.clone());
				place.order = Some(index as i64);
			}
		}
	}

	hub.save();
	hub.bump();
	ok()
}

#[derive(Deserialize)]
struct MoveRequest {
	/// `null` puts the place back under its real owner
	group: Option<String>,
}

/// "Move to group" from the right-click menu: lands at the end of that group
#[post("/api/places/{key}/group")]
async fn move_place(key: UrlPath<String>, request: Json<MoveRequest>, hub: HubData) -> impl Responder {
	{
		let mut store = hub.store();

		// After the places the user already ordered there; with none ordered it simply joins the group (places
		// without a position follow the ordered ones)
		let last = request.group.as_ref().and_then(|group| {
			store
				.places
				.values()
				.filter(|place| place.group.as_ref() == Some(group))
				.filter_map(|place| place.order)
				.max()
				.map(|max| max + 1)
		});

		let Some(place) = store.places.get_mut(key.as_str()) else {
			return error("Unknown place");
		};

		place.group.clone_from(&request.group);
		place.order = last;
	}

	hub.save();
	hub.bump();
	ok()
}

#[derive(Deserialize)]
struct GroupRequest {
	name: String,
}

#[post("/api/groups")]
async fn create_group(request: Json<GroupRequest>, hub: HubData) -> impl Responder {
	let name = request.name.trim().to_owned();

	if name.is_empty() {
		return error("Give the group a name");
	}

	let id = format!("custom:{}", uuid::Uuid::new_v4().simple());

	{
		let mut store = hub.store();
		store.groups.push(super::store::CustomGroup { id: id.clone(), name });

		// New groups show up first
		store.group_order.insert(0, id.clone());
	}

	hub.save();
	hub.bump();
	HttpResponse::Ok().json(json!({ "ok": true, "id": id }))
}

#[post("/api/groups/{id}")]
async fn rename_group(id: UrlPath<String>, request: Json<GroupRequest>, hub: HubData) -> impl Responder {
	let name = request.name.trim().to_owned();

	if name.is_empty() {
		return error("Give the group a name");
	}

	{
		let mut store = hub.store();

		let Some(group) = store.groups.iter_mut().find(|g| g.id == *id) else {
			return error("Unknown group");
		};

		group.name = name;
	}

	hub.save();
	hub.bump();
	ok()
}

/// Deletes a custom group; its places go back under their real owner
#[delete("/api/groups/{id}")]
async fn delete_group(id: UrlPath<String>, hub: HubData) -> impl Responder {
	{
		let mut store = hub.store();
		let id = id.into_inner();

		store.groups.retain(|g| g.id != id);
		store.group_order.retain(|g| *g != id);

		for place in store.places.values_mut() {
			if place.group.as_deref() == Some(id.as_str()) {
				place.group = None;
				place.order = None;
			}
		}
	}

	hub.save();
	hub.bump();
	ok()
}

#[derive(Deserialize)]
struct GroupOrderRequest {
	order: Vec<String>,
}

#[post("/api/groups/order")]
async fn order_groups(request: Json<GroupOrderRequest>, hub: HubData) -> impl Responder {
	hub.store().group_order.clone_from(&request.order);
	hub.save();
	hub.bump();
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

// Dashboard: activity, output, problems, history ///////////////////////////

#[get("/api/places/{key}/activity")]
async fn place_activity(key: UrlPath<String>, hub: HubData) -> impl Responder {
	HttpResponse::Ok().json(hub.journal.activity(&key))
}

#[derive(Deserialize)]
struct SinceQuery {
	since: Option<u64>,
}

#[get("/api/places/{key}/output")]
async fn place_output(key: UrlPath<String>, query: Query<SinceQuery>, hub: HubData) -> impl Responder {
	HttpResponse::Ok().json(hub.journal.output(&key, query.since.unwrap_or(0)))
}

#[post("/api/places/{key}/output/clear")]
async fn clear_output(key: UrlPath<String>, hub: HubData) -> impl Responder {
	hub.journal.clear_output(&key);
	ok()
}

#[derive(Deserialize)]
struct DismissRequest {
	id: Option<String>,
}

#[post("/api/places/{key}/problems/dismiss")]
async fn dismiss_problem(key: UrlPath<String>, request: Json<DismissRequest>, hub: HubData) -> impl Responder {
	hub.journal.dismiss_problem(&key, request.id.as_deref());
	ok()
}

#[derive(Deserialize)]
struct FixRequest {
	action: String,
	arg: Option<String>,
}

/// The one-click fixes problem cards offer
#[post("/api/places/{key}/fix")]
async fn fix_problem(key: UrlPath<String>, request: Json<FixRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = key.into_inner();
	let request = request.into_inner();

	let result = blocking(move || -> Result<Value> {
		match request.action.as_str() {
			"reconnect" => {
				hub.journal.dismiss_problem(&key, None);
				hub.reconnect(&key, "Reconnecting to fix sync problems");
			}
			"wally" => {
				let workspace = hub
					.workspace_of(&key)
					.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;

				Program::new(ProgramName::Wally)
					.message("Failed to run Wally")
					.current_dir(&workspace)
					.arg("install")
					.output()?;

				hub.reconnect(&key, "Wally packages installed");
			}
			"openFile" => {
				let file = request.arg.unwrap_or_default();
				hub.open_file(&key, &file, 1);
			}
			"openFolder" => {
				open::that(
					hub.workspace_of(&key)
						.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?,
				)?;
			}
			"selectInStudio" => {
				let code = hub.select_in_studio_code(&key, &request.arg.unwrap_or_default())?;
				let core = hub
					.core_for(&key)
					.ok_or_else(|| anyhow::anyhow!("No Studio is syncing this place right now"))?;

				core.queue().push(ExecuteCode { code }, None)?;
			}
			other => anyhow::bail!("Unknown fix {other}"),
		}

		Ok(json!({ "ok": true }))
	})
	.await;

	match result {
		Ok(Ok(value)) => HttpResponse::Ok().json(value),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

#[get("/api/places/{key}/history")]
async fn place_history(key: UrlPath<String>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	match blocking(move || hub.journal.history(&key)).await {
		Ok(history) => HttpResponse::Ok().json(history),
		Err(response) => response,
	}
}

#[get("/api/places/{key}/history/{id}")]
async fn history_entry(path: UrlPath<(String, String)>, hub: HubData) -> impl Responder {
	let (key, id) = path.into_inner();
	let hub = hub.get_ref().clone();

	let result = blocking(move || -> Result<Value> {
		let (entry, bytes) = hub
			.journal
			.history_blob(&key, &id)
			.ok_or_else(|| anyhow::anyhow!("That copy is gone (copies are kept for 14 days)"))?;

		let current = hub
			.workspace_of(&key)
			.map(|w| w.join(&entry.path))
			.and_then(|path| fs::read(path).ok());

		let text = String::from_utf8(bytes).ok();
		let current_text = current.as_ref().and_then(|bytes| String::from_utf8(bytes.clone()).ok());

		Ok(json!({
			"entry": entry,
			"binary": text.is_none(),
			"content": text,
			"exists": current.is_some(),
			"current": current_text,
		}))
	})
	.await;

	match result {
		Ok(Ok(value)) => HttpResponse::Ok().json(value),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

#[derive(Deserialize)]
struct RestoreRequest {
	ids: Vec<String>,
}

/// Puts kept copies back on disk; the sync sends them to Studio. Whatever they replace is kept too, so a
/// restore can itself be undone
#[post("/api/places/{key}/history/restore")]
async fn restore_history(key: UrlPath<String>, request: Json<RestoreRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = key.into_inner();
	let ids = request.into_inner().ids;

	let result = blocking(move || -> Result<usize> {
		let workspace = hub
			.workspace_of(&key)
			.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;
		let mut restored = 0;

		for id in ids {
			let Some((entry, bytes)) = hub.journal.history_blob(&key, &id) else {
				continue;
			};

			let path = workspace.join(&entry.path);
			anyhow::ensure!(is_inside(&workspace, &path), "A copy points outside the project folder");

			if path.exists() {
				hub.journal.backup(&key, &workspace, &path, "replaced by restore");
			}

			if let Some(parent) = path.parent() {
				fs::create_dir_all(parent)?;
			}

			fs::write(&path, bytes)?;
			restored += 1;
		}

		Ok(restored)
	})
	.await;

	match result {
		Ok(Ok(restored)) => HttpResponse::Ok().json(json!({ "ok": true, "restored": restored })),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

// Dashboard: updates ////////////////////////////////////////////////////////

#[get("/api/update")]
async fn get_update(hub: HubData) -> impl Responder {
	HttpResponse::Ok().json(hub.update_state())
}

#[post("/api/update/check")]
async fn check_update(hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	let check = move || {
		hub.check_for_update();
		hub.update_state()
	};

	match blocking(check).await {
		Ok(update) => HttpResponse::Ok().json(update),
		Err(response) => response,
	}
}

#[post("/api/update/install")]
async fn install_update(hub: HubData) -> impl Responder {
	match hub.get_ref().install_update() {
		Ok(()) => ok(),
		Err(err) => error(err),
	}
}

#[derive(Deserialize)]
struct SkipRequest {
	version: Option<String>,
}

#[post("/api/update/skip")]
async fn skip_update(request: Json<SkipRequest>, hub: HubData) -> impl Responder {
	hub.store().prefs.skipped_update.clone_from(&request.version);
	hub.save();
	hub.bump();
	ok()
}

// Dashboard: editors & quick open ///////////////////////////////////////////

#[get("/api/editors")]
async fn get_editors(hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();

	match blocking(move || hub.refresh_editors()).await {
		Ok(editors) => HttpResponse::Ok().json(editors),
		Err(response) => response,
	}
}

#[derive(Deserialize)]
struct OpenInRequest {
	/// File relative to the project; the project folder when missing
	path: Option<String>,
	line: Option<u32>,
	/// Editor id; the one picked in Settings when missing
	editor: Option<String>,
}

#[post("/api/places/{key}/open")]
async fn open_in_editor(key: UrlPath<String>, request: Json<OpenInRequest>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let key = key.into_inner();
	let request = request.into_inner();

	let result = blocking(move || -> Result<()> {
		let workspace = hub
			.workspace_of(&key)
			.ok_or_else(|| anyhow::anyhow!("Place is not mapped"))?;

		match (request.editor, request.path) {
			(Some(editor), path) => {
				let file = path.map(|p| workspace.join(p));

				if let Some(file) = &file {
					anyhow::ensure!(is_inside(&workspace, file), "Path escapes the project folder");
				}

				super::editors::open(&editor, &workspace, file.as_deref(), request.line)
			}
			(None, Some(path)) => {
				hub.open_file(&key, &path, request.line.unwrap_or(1));
				Ok(())
			}
			(None, None) => {
				let editor = hub.store().prefs.external_editor.clone();
				anyhow::ensure!(editor != "builtin", "Pick your editor in Settings first");
				super::editors::open(&editor, &workspace, None, None)
			}
		}
	})
	.await;

	match result {
		Ok(Ok(())) => ok(),
		Ok(Err(err)) => error(err),
		Err(response) => response,
	}
}

#[derive(Deserialize)]
struct FilesQuery {
	q: String,
}

const SKIP_DIRS: [&str; 9] = [
	".git",
	"node_modules",
	"Packages",
	"ServerPackages",
	"DevPackages",
	".aragon",
	"build",
	"target",
	".vscode",
];

fn collect_files(root: &Path, dir: &Path, out: &mut Vec<String>) {
	if out.len() > 20_000 {
		return;
	}

	for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
		let path = entry.path();
		let name = entry.file_name().to_string_lossy().to_string();

		if path.is_dir() {
			if !SKIP_DIRS.contains(&name.as_str()) {
				collect_files(root, &path, out);
			}
		} else if let Ok(relative) = path.strip_prefix(root) {
			out.push(relative.to_string_lossy().replace('\\', "/"));
		}
	}
}

/// Fuzzy subsequence score, higher is better; file-name hits beat folder hits, code beats art
fn fuzzy_score(query: &str, path: &str) -> Option<i64> {
	let lower = path.to_lowercase();
	let name_start = lower.rfind('/').map_or(0, |i| i + 1);
	let name = &lower[name_start..];

	let code = [
		".luau", ".lua", ".ts", ".tsx", ".json", ".toml", ".md", ".yaml", ".yml", ".txt", ".csv",
	]
	.iter()
	.any(|ext| name.ends_with(ext));
	let kind = if code { 300 } else { 0 };

	if name.contains(query) {
		return Some(1000 + kind - path.len() as i64 + if name.starts_with(query) { 500 } else { 0 });
	}

	if lower.contains(query) {
		return Some(500 + kind - path.len() as i64);
	}

	let mut score = 0;
	let mut chars = lower.char_indices();

	for wanted in query.chars() {
		let (index, _) = chars.find(|(_, c)| *c == wanted)?;
		score += if index >= name_start { 3 } else { 1 };
	}

	Some(score - path.len() as i64 / 4)
}

/// Quick open across every mapped project (command palette)
#[get("/api/files")]
async fn search_files(query: Query<FilesQuery>, hub: HubData) -> impl Responder {
	let hub = hub.get_ref().clone();
	let q = query.q.trim().to_lowercase();

	let search = move || {
		let places: Vec<(String, String, PathBuf)> = hub
			.store()
			.places
			.values()
			.filter_map(|place| {
				let workspace = place.project.as_ref()?.get_parent().to_owned();
				Some((place.key.clone(), place.name.clone(), workspace))
			})
			.collect();

		let mut hits: Vec<(i64, Value)> = Vec::new();

		for (key, name, workspace) in places {
			let mut files = Vec::new();
			collect_files(&workspace, &workspace, &mut files);

			for file in files {
				if let Some(score) = fuzzy_score(&q, &file) {
					hits.push((score, json!({ "key": key, "place": name, "path": file })));
				}
			}
		}

		hits.sort_by_key(|hit| std::cmp::Reverse(hit.0));
		hits.into_iter().take(40).map(|(_, hit)| hit).collect::<Vec<_>>()
	};

	match blocking(search).await {
		Ok(hits) => HttpResponse::Ok().json(hits),
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
	check_updates: Option<bool>,
	last_version: Option<String>,
	external_editor: Option<String>,
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

		if let Some(check_updates) = request.check_updates {
			store.prefs.check_updates = check_updates;
		}

		if let Some(version) = &request.last_version {
			store.prefs.last_version = Some(version.clone());
		}

		if let Some(editor) = &request.external_editor {
			store.prefs.external_editor.clone_from(editor);
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
		|| target.starts_with("https://github.com/arabdogwater/")
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
				.service(studio_output)
				.service(place_activity)
				.service(place_output)
				.service(clear_output)
				.service(dismiss_problem)
				.service(fix_problem)
				.service(place_history)
				.service(history_entry)
				.service(restore_history)
				.service(get_update)
				.service(check_update)
				.service(install_update)
				.service(skip_update)
				.service(get_editors)
				.service(open_in_editor)
				.service(search_files)
				.service(ping)
				.service(state)
				.service(focus)
				.service(set_key)
				.service(delete_key)
				.service(refresh)
				.service(add_place)
				.service(remove_place)
				.service(restore_place)
				.service(forget_removed)
				.service(arrange_places)
				.service(order_groups)
				.service(create_group)
				.service(rename_group)
				.service(delete_group)
				.service(move_place)
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
