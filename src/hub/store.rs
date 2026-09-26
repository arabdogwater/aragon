//! Everything the hub remembers between runs, persisted as `~/.aragon/hub.json`

use anyhow::Result;
use chrono::{Local, NaiveDate};
use directories::UserDirs;
use log::warn;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
	collections::{BTreeMap, HashMap},
	fs,
	path::PathBuf,
};

use super::roblox::{KeyInfo, UniverseInfo, UserInfo};
use crate::util;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct HubStore {
	/// Root folder every game gets mapped under (`<root>/<owner>/<game>`)
	pub projects_root: Option<PathBuf>,
	pub places: BTreeMap<String, PlaceRecord>,
	pub universes: BTreeMap<u64, UniverseInfo>,
	pub owners: BTreeMap<String, OwnerRecord>,
	/// Roblox user ID reported by the Studio connector (`StudioService:GetUserId()`)
	pub studio_user_id: Option<u64>,
	pub account: Option<Account>,
	/// Global overrides of the Studio plugin settings (setting -> value)
	pub plugin_settings: BTreeMap<String, Value>,
	pub prefs: Prefs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Prefs {
	/// Include every group the user is in, not just the owned/managed ones
	pub all_groups: bool,
	/// Bring the dashboard to front when a new place needs onboarding
	pub focus_on_onboarding: bool,
	/// Default template used by onboarding
	pub template: String,
	/// First-run welcome (plugin install, Studio restart) was completed
	pub welcomed: bool,
}

impl Default for Prefs {
	fn default() -> Self {
		Self {
			all_groups: false,
			focus_on_onboarding: true,
			welcomed: false,
			template: String::from("quick"),
		}
	}
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Account {
	/// Present when an Open Cloud key was added (unlocks private experiences)
	pub key: Option<KeyInfo>,
	pub user: UserInfo,
	pub avatar_url: Option<String>,
	pub last_refresh: Option<i64>,
	pub last_error: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OwnerRecord {
	pub id: u64,
	/// `User` or `Group`
	pub kind: String,
	pub name: String,
	pub icon_url: Option<String>,
	pub role: Option<String>,
	/// Owned by the API key's user (their account or a group they own/manage)
	pub mine: bool,
}

impl OwnerRecord {
	pub fn key(kind: &str, id: u64) -> String {
		format!("{kind}:{id}")
	}
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub enum PlaceSource {
	/// Found through the account (owned games)
	#[default]
	Owned,
	/// Seen because a Studio session connected with it
	Connected,
	/// Added by hand from the dashboard
	Manual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaceRecord {
	pub key: String,
	pub place_id: u64,
	pub universe_id: Option<u64>,
	pub name: String,
	pub icon_url: Option<String>,
	pub thumbnail_url: Option<String>,
	pub is_root: bool,
	pub source: PlaceSource,

	/// Mapped `*.project.json`, `None` until onboarding is completed
	pub project: Option<PathBuf>,
	pub sync_enabled: bool,
	pub sourcemap: bool,
	/// One-time initial sync direction for the next connection ("Client" =
	/// import Studio into files, "Server" = files win), set by onboarding
	pub first_sync: Option<String>,
	/// Per-place Studio plugin setting overrides
	pub plugin_overrides: BTreeMap<String, Value>,

	pub first_seen: i64,
	pub last_connected: Option<i64>,
	pub stats: PlaceStats,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlaceStats {
	pub loc: u64,
	pub files: u64,
	pub lines_added: u64,
	pub lines_removed: u64,
	pub minutes_connected: u64,
	pub syncs: u64,
	pub last_active: Option<i64>,
	pub last_scan: Option<i64>,
	/// `YYYY-MM-DD` -> activity of that day
	pub daily: BTreeMap<String, DayStats>,
	/// Relative file path -> line count, baseline for "lines written"
	pub file_lines: HashMap<String, u64>,
	pub baseline: bool,
	/// Line counts per language (luau, lua, ts, ...)
	pub languages: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DayStats {
	pub added: u64,
	pub removed: u64,
	pub minutes: u64,
	pub syncs: u64,
	pub loc: u64,
}

pub fn today() -> String {
	Local::now().format("%Y-%m-%d").to_string()
}

pub fn now() -> i64 {
	chrono::Utc::now().timestamp()
}

impl PlaceStats {
	pub fn day(&mut self) -> &mut DayStats {
		let today = today();

		// Keep a year of history at most
		while self.daily.len() > 366 {
			let first = self.daily.keys().next().cloned().unwrap();
			self.daily.remove(&first);
		}

		self.daily.entry(today).or_default()
	}

	/// Activity score used to put the places worked on the most at the top.
	/// Last 14 days, newer days weigh more.
	pub fn score(&self) -> f64 {
		let today = Local::now().date_naive();
		let mut score = 0.0;

		for (day, stats) in self.daily.iter().rev().take(30) {
			let Ok(date) = NaiveDate::parse_from_str(day, "%Y-%m-%d") else {
				continue;
			};

			let age = (today - date).num_days();

			if !(0..14).contains(&age) {
				continue;
			}

			let weight = 1.0 - age as f64 / 14.0;
			score += weight * (stats.minutes as f64 + stats.added as f64 / 4.0 + stats.syncs as f64 / 10.0);
		}

		score
	}
}

impl PlaceRecord {
	pub fn new(key: &str, place_id: u64, source: PlaceSource) -> Self {
		Self {
			key: key.to_owned(),
			place_id,
			source,
			sync_enabled: true,
			first_seen: now(),
			..Default::default()
		}
	}
}

/// Place records are keyed by place ID; local `.rbxl` files (place ID 0)
/// get a stable key derived from their name instead
pub fn place_key(place_id: u64, name: &str) -> String {
	if place_id > 0 {
		place_id.to_string()
	} else {
		let slug: String = name
			.to_lowercase()
			.chars()
			.map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
			.collect();

		format!("file-{}", slug.trim_matches('-'))
	}
}

/// Makes a name safe to use as a folder on every OS
pub fn sanitize(name: &str) -> String {
	let cleaned: String = name
		.chars()
		.filter(|c| !c.is_control())
		.map(|c| match c {
			'<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => ' ',
			c => c,
		})
		// Emoji and other symbols from game titles make awful folder names
		.filter(|c| c.is_alphanumeric() || " -_.'()&!+,".contains(*c))
		.collect();

	let collapsed = cleaned.split_whitespace().collect::<Vec<_>>().join(" ");
	let trimmed = collapsed.trim_matches(|c: char| c == '.' || c == ' ');

	if trimmed.is_empty() {
		String::from("Untitled")
	} else {
		trimmed.chars().take(60).collect()
	}
}

pub fn default_projects_root() -> PathBuf {
	UserDirs::new()
		.and_then(|dirs| dirs.document_dir().map(|dir| dir.to_owned()))
		.or_else(|| UserDirs::new().map(|dirs| dirs.home_dir().to_owned()))
		.unwrap_or_else(|| PathBuf::from("."))
		.join("Aragon Projects")
}

fn store_path() -> Result<PathBuf> {
	Ok(util::get_aragon_dir()?.join("hub.json"))
}

impl HubStore {
	pub fn load() -> Self {
		let load = || -> Result<Self> {
			let path = store_path()?;

			if !path.exists() {
				return Ok(Self::default());
			}

			Ok(serde_json::from_str(&fs::read_to_string(path)?)?)
		};

		match load() {
			Ok(store) => store,
			Err(err) => {
				warn!("Hub store is corrupted, starting fresh: {err}");
				Self::default()
			}
		}
	}

	pub fn save(&self) -> Result<()> {
		let path = store_path()?;
		fs::create_dir_all(path.parent().unwrap())?;

		// Write to a temp file first so a crash can't leave a half-written store
		let temp = path.with_extension("json.tmp");
		fs::write(&temp, serde_json::to_vec(self)?)?;
		fs::rename(temp, path)?;

		Ok(())
	}

	pub fn projects_root(&self) -> PathBuf {
		self.projects_root.clone().unwrap_or_else(default_projects_root)
	}
}
