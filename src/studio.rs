use anyhow::Result;
use roblox_install::RobloxStudio;
use std::{
	path::PathBuf,
	process::{Command, Stdio},
};

#[cfg(target_os = "windows")]
use winsafe::{co::SW, EnumWindows};

pub fn launch(path: Option<PathBuf>) -> Result<()> {
	let studio_path = RobloxStudio::locate()?.application_path().to_owned();

	Command::new(studio_path)
		.arg(path.unwrap_or_default())
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.spawn()?;

	Ok(())
}

/// Process name of Roblox Studio on Windows
#[cfg(target_os = "windows")]
const STUDIO_PROCESS: &str = "RobloxStudioBeta.exe";

/// Asks every Studio window to close, like pressing X (unsaved work prompts)
pub fn close_gracefully() -> Result<()> {
	#[cfg(target_os = "windows")]
	{
		// No /F: taskkill sends WM_CLOSE so Studio can ask to save first
		Command::new("taskkill")
			.args(["/IM", STUDIO_PROCESS])
			.stdout(Stdio::null())
			.stderr(Stdio::null())
			.status()?;
	}

	#[cfg(target_os = "macos")]
	{
		Command::new("osascript")
			.args(["-e", "tell application \"RobloxStudio\" to quit"])
			.status()?;
	}

	Ok(())
}

/// Whether any Studio process is alive (windows may already be gone)
pub fn is_process_running() -> bool {
	#[cfg(target_os = "windows")]
	{
		Command::new("tasklist")
			.args(["/NH", "/FI", &format!("IMAGENAME eq {STUDIO_PROCESS}")])
			.output()
			.map(|output| String::from_utf8_lossy(&output.stdout).contains(STUDIO_PROCESS))
			.unwrap_or(false)
	}

	#[cfg(not(target_os = "windows"))]
	{
		is_running(None).unwrap_or(false)
	}
}

#[allow(unused_variables)]
pub fn is_running(title: Option<String>) -> Result<bool> {
	#[cfg(target_os = "macos")]
	{
		let output = Command::new("osascript")
				.args([
					"-e",
					"tell app \"System Events\" to get the title of every window of (processes whose background only is false)",
				])
				.output()?;

		let windows = String::from_utf8(output.stdout)?;

		if let Some(title) = title {
			Ok(windows.contains(&format!("{title} - Roblox Studio")))
		} else {
			Ok(windows.contains("Roblox Studio"))
		}
	}

	#[cfg(target_os = "windows")]
	{
		let is_studio_running = EnumWindows(|hwnd| -> bool {
			if !hwnd.IsWindowVisible() {
				return true;
			}

			if let Ok(text) = hwnd.GetWindowText() {
				if let Some(title) = &title {
					if text == format!("{} - Roblox Studio", title) {
						return false;
					}
				} else if text.contains("Roblox Studio") {
					return false;
				}
			}

			true
		})
		.is_err();

		Ok(is_studio_running)
	}

	#[cfg(target_os = "linux")]
	{
		anyhow::bail!("This feature is not yet supported on Linux!");
	}
}

#[allow(unused_variables)]
pub fn focus(title: Option<String>) -> Result<()> {
	#[cfg(target_os = "macos")]
	{
		if let Some(title) = title {
			Command::new("osascript")
				.args([
					"-e",
					r#"tell application "System Events"
						repeat with theProcess in processes whose name is "RobloxStudio"
								tell theProcess
									set windowList to windows whose name contains "Aragon - Roblox Studio"
									
									if (count of windowList) > 0 then
										set frontmost to true
										perform action "AXRaise" of window 1
									end if
								end tell
						end repeat
					end tell"#,
				])
				.output()?;
		} else {
			Command::new("osascript")
				.args([
					"-e",
					r#"tell application "System Events"
						tell process "RobloxStudio"
							set frontmost to true
							perform action "AXRaise" of window 1
						end tell
					end tell"#,
				])
				.output()?;
		}

		Ok(())
	}

	#[cfg(target_os = "windows")]
	{
		let result = EnumWindows(|hwnd| -> bool {
			if !hwnd.IsWindowVisible() {
				return true;
			}

			if let Ok(text) = hwnd.GetWindowText() {
				if let Some(title) = &title {
					if text == format!("{} - Roblox Studio", title) {
						hwnd.SetForegroundWindow();
						hwnd.ShowWindow(SW::RESTORE);

						return false;
					}
				} else if text.contains("Roblox Studio") {
					hwnd.SetForegroundWindow();
					hwnd.ShowWindow(SW::RESTORE);

					return false;
				}
			}

			true
		});

		match result {
			Ok(()) => (),
			Err(err) => {
				if err.raw() != 0 {
					anyhow::bail!("Failed to focus Roblox Studio: {}", err)
				}
			}
		}

		Ok(())
	}

	#[cfg(target_os = "linux")]
	{
		anyhow::bail!("This feature is not yet supported on Linux!");
	}
}
