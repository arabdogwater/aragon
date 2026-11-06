//! In-app updates: the hub checks GitHub for a newer release, the dashboard shows it in a corner card, and one
//! click downloads it (with progress), checks it against the release's SHA256SUMS, swaps the running exe and
//! restarts into the new version.

use anyhow::{bail, Context, Result};
use log::{info, warn};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
	env, fs,
	io::{Cursor, Read},
	path::{Path, PathBuf},
	process::Command,
	sync::Arc,
	thread,
	time::{Duration, Instant},
};

use super::{store::now, Hub};
use crate::lock;

const CHECK_EVERY: Duration = Duration::from_secs(6 * 3600);
const DEFAULT_REPO: &str = "arabdogwater/aragon";

/// Release notes of every version, shown in "What's new" after an update
pub const CHANGELOG: &str = include_str!("../../CHANGELOG.md");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum UpdateStatus {
	#[default]
	Idle,
	Checking,
	UpToDate,
	Available,
	Downloading,
	Verifying,
	Installing,
	Restarting,
	Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
	pub version: String,
	pub name: String,
	pub notes: String,
	pub url: String,
	pub published_at: String,
	pub asset_url: String,
	pub asset_name: String,
	pub size: u64,
	pub sums_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct UpdateState {
	pub status: UpdateStatus,
	pub current: String,
	pub latest: Option<Release>,
	pub downloaded: u64,
	pub total: u64,
	/// Bytes per second over the download so far
	pub speed: f64,
	pub error: Option<String>,
	pub last_check: Option<i64>,
}

impl UpdateState {
	pub fn new() -> Self {
		Self {
			current: env!("CARGO_PKG_VERSION").to_owned(),
			..Self::default()
		}
	}
}

fn repo() -> String {
	env::var("ARAGON_UPDATE_REPO")
		.ok()
		.or_else(|| option_env!("ARAGON_REPO").map(str::to_owned))
		.unwrap_or_else(|| DEFAULT_REPO.to_owned())
}

/// `1.2.10` > `1.2.9`, pre-release tags ignored
pub fn is_newer(candidate: &str, current: &str) -> bool {
	let parse = |v: &str| -> Vec<u64> {
		v.trim_start_matches('v')
			.split(['-', '+'])
			.next()
			.unwrap_or("")
			.split('.')
			.map(|p| p.parse().unwrap_or(0))
			.collect()
	};

	parse(candidate) > parse(current)
}

/// The `## [version]` section of the changelog, without its heading
pub fn changelog_section(version: &str) -> Option<String> {
	let heading = format!("## [{version}]");
	let start = CHANGELOG.find(&heading)?;
	let body = &CHANGELOG[start + heading.len()..];
	let body = body.split_once('\n').map_or("", |(_, rest)| rest);
	let end = body.find("\n## [").unwrap_or(body.len());

	Some(body[..end].trim().to_owned())
}

fn client() -> Result<reqwest::blocking::Client> {
	Ok(reqwest::blocking::Client::builder()
		.user_agent(concat!("aragon-hub/", env!("CARGO_PKG_VERSION")))
		.connect_timeout(Duration::from_secs(15))
		.build()?)
}

fn fetch_latest() -> Result<Option<Release>> {
	let url = env::var("ARAGON_UPDATE_API")
		.unwrap_or_else(|_| format!("https://api.github.com/repos/{}/releases/latest", repo()));

	let response = client()?
		.get(&url)
		.header("Accept", "application/vnd.github+json")
		.timeout(Duration::from_secs(20))
		.send()?;

	if response.status().as_u16() == 404 {
		return Ok(None);
	}

	let release: serde_json::Value = response.error_for_status()?.json()?;
	let tag = release["tag_name"].as_str().context("Release has no tag")?;
	let version = tag.trim_start_matches('v').to_owned();
	let assets = release["assets"].as_array().cloned().unwrap_or_default();

	let asset = assets.iter().find(|asset| {
		let name = asset["name"].as_str().unwrap_or("");
		name.ends_with(".zip") && name.contains("windows")
	});

	let Some(asset) = asset else {
		return Ok(None);
	};

	let sums_url = assets
		.iter()
		.find(|asset| asset["name"].as_str() == Some("SHA256SUMS.txt"))
		.and_then(|asset| asset["browser_download_url"].as_str())
		.map(str::to_owned);

	Ok(Some(Release {
		name: release["name"].as_str().unwrap_or(tag).to_owned(),
		notes: release["body"].as_str().unwrap_or("").to_owned(),
		url: release["html_url"].as_str().unwrap_or("").to_owned(),
		published_at: release["published_at"].as_str().unwrap_or("").to_owned(),
		asset_url: asset["browser_download_url"].as_str().unwrap_or("").to_owned(),
		asset_name: asset["name"].as_str().unwrap_or("").to_owned(),
		size: asset["size"].as_u64().unwrap_or(0),
		sums_url,
		version,
	}))
}

impl Hub {
	pub fn update_state(&self) -> UpdateState {
		lock!(self.update).clone()
	}

	fn set_update(&self, f: impl FnOnce(&mut UpdateState)) {
		let before = lock!(self.update).status;

		f(&mut lock!(self.update));

		// Progress is polled by the dashboard; only status changes wake every long-poll
		if lock!(self.update).status != before {
			self.bump();
		}
	}

	/// Checks now, then every few hours (unless turned off in Settings)
	pub fn start_update_checks(self: &Arc<Self>) {
		let hub = self.clone();

		thread::Builder::new()
			.name("hub-updates".into())
			.spawn(move || {
				// Leave startup alone, and clean up after a previous update
				thread::sleep(Duration::from_secs(8));
				cleanup_old_exe();

				loop {
					if hub.store().prefs.check_updates {
						hub.check_for_update();
					}

					thread::sleep(CHECK_EVERY);
				}
			})
			.unwrap();
	}

	pub fn check_for_update(&self) {
		let busy = matches!(
			lock!(self.update).status,
			UpdateStatus::Checking
				| UpdateStatus::Downloading
				| UpdateStatus::Verifying
				| UpdateStatus::Installing
				| UpdateStatus::Restarting
		);

		if busy {
			return;
		}

		self.set_update(|state| {
			state.status = UpdateStatus::Checking;
			state.error = None;
		});

		let result = fetch_latest();

		self.set_update(|state| {
			state.last_check = Some(now());

			match result {
				Ok(Some(release)) if is_newer(&release.version, &state.current) => {
					info!("Aragon {} is available", release.version);
					state.status = UpdateStatus::Available;
					state.latest = Some(release);
				}
				Ok(release) => {
					state.status = UpdateStatus::UpToDate;
					state.latest = release;
				}
				Err(err) => {
					warn!("Update check failed: {err}");
					state.status = UpdateStatus::Error;
					state.error = Some(format!("Couldn't check for updates: {err}"));
				}
			}
		});
	}

	/// Downloads, verifies and installs the latest release, then restarts into it (in the background)
	pub fn install_update(self: &Arc<Self>) -> Result<()> {
		let release = {
			let state = lock!(self.update);

			if matches!(
				state.status,
				UpdateStatus::Downloading
					| UpdateStatus::Verifying
					| UpdateStatus::Installing
					| UpdateStatus::Restarting
			) {
				return Ok(());
			}

			state.latest.clone().context("No update to install, check first")?
		};

		let hub = self.clone();

		thread::Builder::new()
			.name("hub-update-install".into())
			.spawn(move || {
				if let Err(err) = hub.run_install(&release) {
					warn!("Update failed: {err:#}");

					hub.set_update(|state| {
						state.status = UpdateStatus::Error;
						state.error = Some(format!("{err:#}"));
					});
				}
			})?;

		Ok(())
	}

	fn run_install(self: &Arc<Self>, release: &Release) -> Result<()> {
		self.set_update(|state| {
			state.status = UpdateStatus::Downloading;
			state.downloaded = 0;
			state.total = release.size;
			state.speed = 0.0;
			state.error = None;
		});

		let client = client()?;
		let mut response = client
			.get(&release.asset_url)
			.timeout(Duration::from_secs(600))
			.send()?
			.error_for_status()?;

		let total = response.content_length().unwrap_or(release.size);
		let mut bytes = Vec::with_capacity(total as usize);
		let mut chunk = vec![0u8; 64 * 1024];
		let started = Instant::now();

		loop {
			let read = response.read(&mut chunk)?;

			if read == 0 {
				break;
			}

			bytes.extend_from_slice(&chunk[..read]);

			let mut state = lock!(self.update);
			state.downloaded = bytes.len() as u64;
			state.total = total.max(state.downloaded);
			state.speed = bytes.len() as f64 / started.elapsed().as_secs_f64().max(0.001);
		}

		self.set_update(|state| state.status = UpdateStatus::Verifying);

		if let Some(sums_url) = &release.sums_url {
			let sums = client.get(sums_url).send()?.error_for_status()?.text()?;
			let expected = sums
				.lines()
				.find(|line| line.trim_end().ends_with(&release.asset_name))
				.and_then(|line| line.split_whitespace().next())
				.context("The release's checksum list doesn't cover the download")?
				.to_lowercase();

			let actual: String = Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect();

			if actual != expected {
				bail!("The download is damaged (checksum mismatch), nothing was changed. Try again");
			}
		}

		self.set_update(|state| state.status = UpdateStatus::Installing);

		let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).context("The download isn't a valid zip")?;
		let mut exe = Vec::new();

		{
			let index = (0..archive.len())
				.find(|&i| {
					archive
						.by_index(i)
						.is_ok_and(|file| file.name().to_lowercase().ends_with("aragon.exe"))
				})
				.context("aragon.exe is missing from the release")?;

			archive.by_index(index)?.read_to_end(&mut exe)?;
		}

		let current = env::current_exe()?;
		replace_exe(&current, &exe)?;

		// Keep the copy on PATH (`aragon` in a terminal) on the same version
		if let Ok(bin) = crate::util::get_aragon_dir().map(|dir| dir.join("bin").join("aragon.exe")) {
			if bin.exists() && path_clean::clean(&bin) != path_clean::clean(&current) {
				fs::write(&bin, &exe).ok();
			}
		}

		info!("Installed Aragon {}, restarting", release.version);
		self.set_update(|state| state.status = UpdateStatus::Restarting);

		// Let the dashboard see "Restarting" before this process goes away
		thread::sleep(Duration::from_millis(900));
		restart(&current)
	}
}

/// Windows can't overwrite a running exe but can rename it: the old one steps aside as `.old`
fn replace_exe(current: &Path, new: &[u8]) -> Result<()> {
	let staged = current.with_extension("exe.new");
	let old = current.with_extension("exe.old");

	fs::write(&staged, new).context("Couldn't write the new version next to the old one")?;
	fs::remove_file(&old).ok();
	fs::rename(current, &old).context("Couldn't move the running version aside")?;

	if let Err(err) = fs::rename(&staged, current) {
		// Put the old one back so Aragon still starts
		fs::rename(&old, current).ok();
		return Err(err).context("Couldn't put the new version in place");
	}

	Ok(())
}

fn cleanup_old_exe() {
	if let Ok(current) = env::current_exe() {
		fs::remove_file(current.with_extension("exe.old")).ok();
	}
}

/// Starts the new exe (it waits for this one to free the hub port) and exits
fn restart(exe: &PathBuf) -> Result<()> {
	// Same command as now, minus `--background`: the dashboard was open when the update was started
	let args: Vec<String> = env::args()
		.skip(1)
		.filter(|arg| arg != "--background" && arg != "-b")
		.collect();

	let mut command = Command::new(exe);
	command
		.args(&args)
		.env("ARAGON_RESTARTED_FROM", env!("CARGO_PKG_VERSION"))
		.env("ARAGON_WAIT_FOR_PORT", "1");

	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// DETACHED_PROCESS: no console window for the new hub
		command.creation_flags(0x0000_0008);
	}

	command.spawn().context("Couldn't start the new version")?;
	std::process::exit(0);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn versions() {
		assert!(is_newer("1.1.0", "1.0.6"));
		assert!(is_newer("v1.0.10", "1.0.9"));
		assert!(!is_newer("1.1.0", "1.1.0"));
		assert!(!is_newer("1.0.9", "1.1.0"));
		assert!(!is_newer("1.1.0-beta", "1.1.0"));
	}

	#[test]
	fn changelog_has_this_version() {
		let notes =
			changelog_section(env!("CARGO_PKG_VERSION")).expect("CHANGELOG.md needs a section for this version");
		assert!(!notes.contains("## ["));
		assert!(notes.len() > 20);
	}
}
