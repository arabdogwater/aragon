use anyhow::Result;
use std::{env, fs::File, path::PathBuf, process::Command};

// Bundles the Aragon Studio plugin into the binary so the CLI/dashboard can
// install it without any network access. The plugin is built from `plugin/`
// with Rojo; when Rojo or the plugin packages are missing we fall back to an
// empty file and the installer reports that no bundled plugin exists.
fn main() -> Result<()> {
	let out_path = PathBuf::from(env::var("OUT_DIR")?).join("Aragon.rbxm");
	let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
	let plugin_dir = manifest_dir.join("plugin");

	println!("cargo:rerun-if-changed=assets/aragon.rc");
	println!("cargo:rerun-if-changed=assets/branding/aragon.ico");

	// Exe icon + version info shown by Explorer and the taskbar
	if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
		embed_resource::compile("assets/aragon.rc", embed_resource::NONE)
			.manifest_optional()
			.map_err(|err| anyhow::anyhow!("Failed to embed Windows resources: {err}"))?;
	}

	println!("cargo:rerun-if-changed=plugin/src");
	println!("cargo:rerun-if-changed=plugin/default.project.json");
	println!("cargo:rerun-if-changed=plugin/wally.toml");

	if !plugin_dir.join("Packages").exists() {
		println!("cargo:warning=plugin/Packages missing (run `wally install` in plugin/), bundling no plugin");
		File::create(out_path)?;
		return Ok(());
	}

	let status = Command::new("rojo")
		.current_dir(&plugin_dir)
		.args(["build", "default.project.json", "--output"])
		.arg(&out_path)
		.status();

	match status {
		Ok(status) if status.success() => {}
		Ok(status) => {
			println!("cargo:warning=rojo exited with {status}, bundling no plugin");
			File::create(out_path)?;
		}
		Err(err) => {
			println!("cargo:warning=rojo not found ({err}), bundling no plugin");
			File::create(out_path)?;
		}
	}

	Ok(())
}
