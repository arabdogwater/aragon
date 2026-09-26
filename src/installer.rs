use anyhow::Result;
use colored::Colorize;
use include_dir::{include_dir, Dir};
use rbx_dom_weak::{types::Variant, ustr};
use std::{env, fs, path::Path};

use crate::{
	aragon_error, aragon_info,
	ext::PathExt,
	updater,
	util::{self, get_plugin_path},
};

const PLACE_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/place");
const PLUGIN_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/plugin");
const PACKAGE_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/package");
const MODEL_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/model");
const QUICK_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/quick");
const EMPTY_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/assets/templates/empty");

const ARAGON_PLUGIN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/Aragon.rbxm"));

pub fn is_managed() -> bool {
	let path = match env::current_exe() {
		Ok(path) => path,
		Err(_) => return false,
	};

	// Dev builds (cargo target dir) never install themselves
	let is_dev = path.contains(&["target", "debug"]) || path.contains(&["target", "release"]);

	is_dev || (!path.contains(&[".aragon", "bin"]) && (path.contains(&["bin"]) || path.contains(&["tool-storage"])))
}

pub fn verify(is_managed: bool, with_plugin: bool) -> Result<()> {
	if !is_managed {
		let bin_dir = util::get_aragon_dir()?.join("bin");

		if !bin_dir.exists() {
			fs::create_dir_all(&bin_dir)?;
		}

		globenv::set_path(&bin_dir.to_string())?;

		#[cfg(not(target_os = "windows"))]
		let exe_path = bin_dir.join("aragon");

		#[cfg(target_os = "windows")]
		let exe_path = bin_dir.join("aragon.exe");

		// Keep a copy on PATH so `aragon` works from any terminal. Never prompt
		// here: a double-clicked dashboard has no console to answer in
		if !exe_path.exists() {
			fs::copy(env::current_exe()?, &exe_path)?;
		}
	}

	install_templates(false)?;

	if with_plugin {
		let plugin_path = get_plugin_path()?;

		if !plugin_path.exists() {
			install_plugin(&plugin_path, false)?;
		}
	}

	Ok(())
}

pub fn install_plugin(path: &Path, _show_progress: bool) -> Result<()> {
	fs::create_dir_all(path.get_parent())?;

	#[allow(clippy::const_is_empty)]
	if ARAGON_PLUGIN.is_empty() {
		aragon_error!("This Aragon build has no bundled Studio plugin! Build it with Rojo (see README)");
		return Ok(());
	}

	fs::write(path, ARAGON_PLUGIN)?;

	let version = get_plugin_version();

	if path.contains(&["Roblox", "Plugins"]) {
		let mut status = updater::get_status()?;
		status.plugin_version = version.clone();

		updater::set_status(&status)?;
	}

	aragon_info!("Installed Aragon plugin, version: {}", version.bold());

	Ok(())
}

pub fn install_templates(update: bool) -> Result<()> {
	let templates_dir = util::get_aragon_dir()?.join("templates");

	let place_template = templates_dir.join("place");
	let plugin_template = templates_dir.join("plugin");
	let package_template = templates_dir.join("package");
	let model_template = templates_dir.join("model");
	let quick_template = templates_dir.join("quick");
	let empty_template = templates_dir.join("empty");

	if update || !place_template.exists() {
		fs::create_dir_all(&place_template)?;
		install_template(&PLACE_TEMPLATE, &place_template)?;
	}

	if update || !plugin_template.exists() {
		fs::create_dir_all(&plugin_template)?;
		install_template(&PLUGIN_TEMPLATE, &plugin_template)?;
	}

	if update || !package_template.exists() {
		fs::create_dir_all(&package_template)?;
		install_template(&PACKAGE_TEMPLATE, &package_template)?;
	}

	if update || !model_template.exists() {
		fs::create_dir_all(&model_template)?;
		install_template(&MODEL_TEMPLATE, &model_template)?;
	}

	if update || !quick_template.exists() {
		fs::create_dir_all(&quick_template)?;
		install_template(&QUICK_TEMPLATE, &quick_template)?;
	}

	if update || !empty_template.exists() {
		fs::create_dir_all(&empty_template)?;
		install_template(&EMPTY_TEMPLATE, &empty_template)?;
	}

	Ok(())
}

fn install_template(template: &Dir, path: &Path) -> Result<()> {
	for file in template.files() {
		if file.path().get_name() != ".gitkeep" {
			fs::write(path.join(file.path()), file.contents())?;
		}
	}

	for dir in template.dirs() {
		fs::create_dir_all(path.join(dir.path()))?;
		install_template(dir, path)?;
	}

	Ok(())
}

pub fn get_plugin_version() -> String {
	// May seem hacky, but this function will only be
	// called once for most users and is non-critical anyway
	if let Ok(dom) = rbx_binary::from_reader(ARAGON_PLUGIN) {
		for (_, instance) in dom.into_raw().1 {
			if instance.name == "manifest" && instance.class == "ModuleScript" {
				if let Some(Variant::String(source)) = instance.properties.get(&ustr("Source")) {
					// Rojo/Argon turn wally.toml into a Luau table, key style varies
					// (`version = "x"` or `["version"] = "x"`), so find the value loosely
					for (index, _) in source.match_indices("version") {
						let rest = source[index + 7..].trim_start_matches(['"', ']', ' ']);

						if let Some(rest) = rest.strip_prefix('=') {
							let rest = rest.trim_start();

							if let Some(value) = rest.strip_prefix('"').and_then(|v| v.split('"').next()) {
								return value.to_owned();
							}
						}
					}
				}
			}
		}
	}

	String::from("0.0.0")
}
