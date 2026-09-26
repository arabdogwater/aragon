//! Thin blocking client for the Roblox web + Open Cloud APIs the dashboard needs.
//!
//! Everything here works without a `.ROBLOSECURITY` cookie:
//! - the Open Cloud API key is only used for introspection (who owns it,
//!   which universes it is scoped to) and the optional group listing
//! - games, places and thumbnails come from the public web APIs
//!
//! Endpoints and limits were verified against the live APIs (Sept 2026).

use anyhow::{bail, Context, Result};
use reqwest::{blocking::Client, header, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::HashMap, thread, time::Duration};

const USER_AGENT: &str = concat!("Aragon/", env!("CARGO_PKG_VERSION"));

pub struct Roblox {
	client: Client,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct KeyInfo {
	pub name: String,
	pub user_id: u64,
	pub enabled: bool,
	pub expired: bool,
	pub expiration: Option<String>,
	/// Universe IDs the key is explicitly restricted to (empty = all or none)
	pub universe_ids: Vec<u64>,
	/// Group IDs the key is explicitly restricted to
	pub group_ids: Vec<u64>,
	pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
	pub id: u64,
	pub name: String,
	pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupMembership {
	pub id: u64,
	pub name: String,
	pub role: String,
	pub rank: u8,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UniverseInfo {
	pub id: u64,
	pub name: String,
	pub root_place_id: u64,
	pub creator_id: u64,
	pub creator_name: String,
	/// `User` or `Group`
	pub creator_type: String,
	pub privacy: Option<String>,
	pub visits: Option<u64>,
	pub playing: Option<u64>,
	pub favorites: Option<u64>,
	pub updated: Option<String>,
	pub created: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceInfo {
	pub id: u64,
	pub universe_id: u64,
	pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ThumbKind {
	GameIcon,
	GameThumbnail,
	PlaceIcon,
	AvatarHeadShot,
	GroupIcon,
}

impl ThumbKind {
	fn as_str(&self) -> &'static str {
		match self {
			ThumbKind::GameIcon => "GameIcon",
			ThumbKind::GameThumbnail => "GameThumbnail",
			ThumbKind::PlaceIcon => "PlaceIcon",
			ThumbKind::AvatarHeadShot => "AvatarHeadShot",
			ThumbKind::GroupIcon => "GroupIcon",
		}
	}

	fn size(&self) -> &'static str {
		match self {
			ThumbKind::GameIcon | ThumbKind::PlaceIcon => "256x256",
			ThumbKind::GameThumbnail => "768x432",
			ThumbKind::AvatarHeadShot | ThumbKind::GroupIcon => "150x150",
		}
	}
}

impl Roblox {
	pub fn new() -> Self {
		let client = Client::builder()
			.user_agent(USER_AGENT)
			.timeout(Duration::from_secs(15))
			.build()
			.expect("Failed to build HTTP client");

		Self { client }
	}

	fn get<T: DeserializeOwned>(&self, url: &str) -> Result<T> {
		for attempt in 0..3 {
			let response = self.client.get(url).send()?;

			if response.status() == StatusCode::TOO_MANY_REQUESTS {
				thread::sleep(Duration::from_millis(1200 * (attempt + 1)));
				continue;
			}

			if !response.status().is_success() {
				bail!("{} returned {}", url, response.status());
			}

			return Ok(response.json()?);
		}

		bail!("{url} is rate limited, try again in a minute")
	}

	fn post<T: DeserializeOwned>(&self, url: &str, body: &Value) -> Result<T> {
		for attempt in 0..3 {
			let response = self
				.client
				.post(url)
				.header(header::CONTENT_TYPE, "application/json")
				.json(body)
				.send()?;

			if response.status() == StatusCode::TOO_MANY_REQUESTS {
				thread::sleep(Duration::from_millis(1200 * (attempt + 1)));
				continue;
			}

			if !response.status().is_success() {
				bail!("{} returned {}", url, response.status());
			}

			return Ok(response.json()?);
		}

		bail!("{url} is rate limited, try again in a minute")
	}

	/// Validates an Open Cloud API key and returns who owns it
	pub fn introspect(&self, api_key: &str) -> Result<KeyInfo> {
		let response = self
			.client
			.post("https://apis.roblox.com/api-keys/v1/introspect")
			.header(header::CONTENT_TYPE, "application/json")
			.json(&json!({ "apiKey": api_key.trim() }))
			.send()?;

		let status = response.status();
		let body: Value = response.json().unwrap_or(Value::Null);

		if !status.is_success() {
			let message = body["message"].as_str().unwrap_or("Unknown error");
			bail!("Roblox rejected the API key: {message}");
		}

		let mut info = KeyInfo {
			name: body["name"].as_str().unwrap_or("API key").to_owned(),
			user_id: body["authorizedUserId"].as_u64().unwrap_or_default(),
			enabled: body["enabled"].as_bool().unwrap_or(true),
			expired: body["expired"].as_bool().unwrap_or(false),
			expiration: body["expirationTimeUtc"].as_str().map(str::to_owned),
			..Default::default()
		};

		let ids = |value: &Value| -> Vec<u64> {
			value
				.as_array()
				.map(|ids| ids.iter().filter_map(|id| id.as_str()?.parse().ok()).collect())
				.unwrap_or_default()
		};

		for scope in body["scopes"].as_array().into_iter().flatten() {
			if let Some(name) = scope["name"].as_str() {
				info.scopes.push(name.to_owned());
			}

			info.universe_ids.extend(ids(&scope["universeIds"]));
			info.group_ids.extend(ids(&scope["groupIds"]));

			for datastore in scope["universeDatastores"].as_array().into_iter().flatten() {
				if let Some(id) = datastore["universeId"].as_str().and_then(|id| id.parse().ok()) {
					info.universe_ids.push(id);
				}
			}
		}

		info.universe_ids.sort_unstable();
		info.universe_ids.dedup();
		info.group_ids.sort_unstable();
		info.group_ids.dedup();

		if info.user_id == 0 {
			bail!("Roblox did not report an owner for this API key");
		}

		Ok(info)
	}

	/// Groups the key owner can manage (needs `legacy-group:manage`, optional)
	pub fn manageable_groups(&self, api_key: &str) -> Result<Vec<u64>> {
		let response = self
			.client
			.get("https://apis.roblox.com/legacy-develop/v1/user/groups/canmanage")
			.header("x-api-key", api_key.trim())
			.send()?;

		if !response.status().is_success() {
			bail!("Key has no legacy-group:manage scope");
		}

		let body: Value = response.json()?;

		Ok(body["data"]
			.as_array()
			.into_iter()
			.flatten()
			.filter_map(|group| group["id"].as_u64())
			.collect())
	}

	pub fn users(&self, ids: &[u64]) -> Result<Vec<UserInfo>> {
		if ids.is_empty() {
			return Ok(vec![]);
		}

		let body: Value = self.post(
			"https://users.roblox.com/v1/users",
			&json!({ "userIds": ids, "excludeBannedUsers": false }),
		)?;

		Ok(body["data"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|user| UserInfo {
				id: user["id"].as_u64().unwrap_or_default(),
				name: user["name"].as_str().unwrap_or_default().to_owned(),
				display_name: user["displayName"].as_str().unwrap_or_default().to_owned(),
			})
			.collect())
	}

	pub fn user_groups(&self, user_id: u64) -> Result<Vec<GroupMembership>> {
		let body: Value = self.get(&format!(
			"https://groups.roblox.com/v2/users/{user_id}/groups/roles?includeLocked=true"
		))?;

		Ok(body["data"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|entry| GroupMembership {
				id: entry["group"]["id"].as_u64().unwrap_or_default(),
				name: entry["group"]["name"].as_str().unwrap_or_default().to_owned(),
				role: entry["role"]["name"].as_str().unwrap_or_default().to_owned(),
				rank: entry["role"]["rank"].as_u64().unwrap_or_default() as u8,
			})
			.collect())
	}

	/// Public games of a user (private user games need a cookie, Roblox limitation)
	pub fn user_universes(&self, user_id: u64) -> Result<Vec<u64>> {
		self.paginate(&format!(
			"https://games.roblox.com/v2/users/{user_id}/games?accessFilter=2&limit=50&sortOrder=Asc"
		))
	}

	/// All games of a group, including private ones
	pub fn group_universes(&self, group_id: u64) -> Result<Vec<u64>> {
		self.paginate(&format!(
			"https://games.roblox.com/v2/groups/{group_id}/gamesV2?accessFilter=1&limit=100&sortOrder=Asc"
		))
	}

	fn paginate(&self, base: &str) -> Result<Vec<u64>> {
		let mut ids = Vec::new();
		let mut cursor = String::new();

		// Hard cap so a group with thousands of games can't stall the hub
		for _ in 0..10 {
			let url = if cursor.is_empty() {
				base.to_owned()
			} else {
				format!("{base}&cursor={cursor}")
			};

			let body: Value = self.get(&url)?;

			for game in body["data"].as_array().into_iter().flatten() {
				if let Some(id) = game["id"].as_u64() {
					ids.push(id);
				}
			}

			match body["nextPageCursor"].as_str() {
				Some(next) if !next.is_empty() => {
					cursor = next.to_owned();
					// gamesV2 is limited to 3 requests per second
					thread::sleep(Duration::from_millis(350));
				}
				_ => break,
			}
		}

		Ok(ids)
	}

	pub fn place_universe(&self, place_id: u64) -> Result<u64> {
		let body: Value = self.get(&format!(
			"https://apis.roblox.com/universes/v1/places/{place_id}/universe"
		))?;

		body["universeId"]
			.as_u64()
			.context("Roblox does not know this place (unpublished or deleted)")
	}

	/// Batch universe details: develop multiget (includes private games) merged
	/// with the games API (visits, playing, favorites)
	pub fn universes(&self, ids: &[u64]) -> Result<Vec<UniverseInfo>> {
		let mut result: HashMap<u64, UniverseInfo> = HashMap::new();

		for chunk in ids.chunks(100) {
			let query = chunk.iter().map(|id| format!("ids={id}")).collect::<Vec<_>>().join("&");
			let body: Value = self.get(&format!("https://develop.roblox.com/v1/universes/multiget?{query}"))?;

			for universe in body["data"].as_array().into_iter().flatten() {
				let id = universe["id"].as_u64().unwrap_or_default();

				if id == 0 {
					continue;
				}

				result.insert(
					id,
					UniverseInfo {
						id,
						name: universe["name"].as_str().unwrap_or_default().to_owned(),
						root_place_id: universe["rootPlaceId"].as_u64().unwrap_or_default(),
						creator_id: universe["creatorTargetId"].as_u64().unwrap_or_default(),
						creator_name: universe["creatorName"].as_str().unwrap_or_default().to_owned(),
						creator_type: universe["creatorType"].as_str().unwrap_or("User").to_owned(),
						privacy: universe["privacyType"].as_str().map(str::to_owned),
						updated: universe["updated"].as_str().map(str::to_owned),
						created: universe["created"].as_str().map(str::to_owned),
						..Default::default()
					},
				);
			}
		}

		for chunk in ids.chunks(50) {
			let query = chunk.iter().map(u64::to_string).collect::<Vec<_>>().join(",");
			let body: Value = match self.get(&format!("https://games.roblox.com/v1/games?universeIds={query}")) {
				Ok(body) => body,
				Err(_) => continue,
			};

			for game in body["data"].as_array().into_iter().flatten() {
				let id = game["id"].as_u64().unwrap_or_default();

				// Private universes queried alone come back as id 0 placeholders
				if let Some(universe) = result.get_mut(&id) {
					universe.visits = game["visits"].as_u64();
					universe.playing = game["playing"].as_u64();
					universe.favorites = game["favoritedCount"].as_u64();
				}
			}
		}

		Ok(ids.iter().filter_map(|id| result.remove(id)).collect())
	}

	pub fn universe_places(&self, universe_id: u64) -> Result<Vec<PlaceInfo>> {
		let body: Value = self.get(&format!(
			"https://develop.roblox.com/v1/universes/{universe_id}/places?sortOrder=Asc&limit=100"
		))?;

		Ok(body["data"]
			.as_array()
			.into_iter()
			.flatten()
			.map(|place| PlaceInfo {
				id: place["id"].as_u64().unwrap_or_default(),
				universe_id,
				name: place["name"].as_str().unwrap_or_default().to_owned(),
			})
			.collect())
	}

	/// Resolves image URLs for many targets at once; unknown/pending ones are skipped
	pub fn thumbnails(&self, targets: &[(ThumbKind, u64)]) -> HashMap<(ThumbKind, u64), String> {
		let mut result = HashMap::new();

		for chunk in targets.chunks(100) {
			let requests: Vec<Value> = chunk
				.iter()
				.map(|(kind, id)| {
					json!({
						"requestId": format!("{}:{}", kind.as_str(), id),
						"type": kind.as_str(),
						"targetId": id,
						"size": kind.size(),
						"format": "Webp",
						"isCircular": false,
					})
				})
				.collect();

			let body: Value = match self.post("https://thumbnails.roblox.com/v1/batch", &Value::Array(requests)) {
				Ok(body) => body,
				Err(_) => continue,
			};

			for entry in body["data"].as_array().into_iter().flatten() {
				let (Some(request_id), Some(url)) = (entry["requestId"].as_str(), entry["imageUrl"].as_str()) else {
					continue;
				};

				if let Some(target) = chunk
					.iter()
					.find(|(kind, id)| format!("{}:{}", kind.as_str(), id) == request_id)
				{
					result.insert(*target, url.to_owned());
				}
			}
		}

		result
	}
}
