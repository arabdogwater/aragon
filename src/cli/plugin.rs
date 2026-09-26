use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::{fs, path::PathBuf};

use crate::{aragon_info, config::Config, ext::PathExt, installer, util};

/// Install Aragon Roblox Studio plugin locally
#[derive(Parser)]
pub struct Plugin {
	/// Whether to `install` or `uninstall` the plugin
	#[arg(hide_possible_values = true)]
	mode: Option<PluginMode>,
	/// Custom plugin installation path
	#[arg()]
	path: Option<PathBuf>,
}

impl Plugin {
	pub fn main(self) -> Result<()> {
		let plugin_path = if let Some(path) = self.path {
			let smart_paths = Config::new().smart_paths;

			if path.is_dir() || (smart_paths && (path.extension().is_none())) {
				if !smart_paths || path.get_name().to_lowercase() != "aragon" {
					path.join("Aragon.rbxm")
				} else {
					path.with_extension("rbxm")
				}
			} else {
				path
			}
		} else {
			util::get_plugin_path()?
		};

		match self.mode.unwrap_or_default() {
			PluginMode::Install => {
				aragon_info!("Installing Aragon plugin..");
				installer::install_plugin(&plugin_path, true)?;
			}
			PluginMode::Uninstall => {
				aragon_info!("Uninstalling Aragon plugin..");
				fs::remove_file(plugin_path)?;
			}
		}

		Ok(())
	}
}

#[derive(Clone, Default, ValueEnum)]
enum PluginMode {
	#[default]
	Install,
	Uninstall,
}
