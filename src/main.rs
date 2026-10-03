use env_logger::WriteStyle;
use log::{debug, error, info, warn, LevelFilter};
use puffin_http::Server;
use std::{
	env,
	io::{self, IsTerminal},
	mem::ManuallyDrop,
	process::ExitCode,
	thread,
};

use aragon::{aragon_error, cli::Cli, config::Config, crash_handler, installer, logger, stats, updater, util};

const PROFILER_ADDRESS: &str = "localhost:8888";

/// When the exe is double-clicked Explorer gives it a fresh console that only
/// this process uses; hide it so only the dashboard window shows. Running
/// from a terminal keeps the console (and logs) as usual.
#[cfg(windows)]
fn detach_console_if_double_clicked() {
	use windows_sys::Win32::{
		System::Console::{FreeConsole, GetConsoleProcessList, GetConsoleWindow},
		UI::WindowsAndMessaging::{ShowWindow, SW_HIDE},
	};

	unsafe {
		let mut processes = [0u32; 2];

		if GetConsoleProcessList(processes.as_mut_ptr(), 2) == 1 {
			let window = GetConsoleWindow();

			if !window.is_null() {
				ShowWindow(window, SW_HIDE);
			}

			FreeConsole();
		}
	}
}

fn main() -> ExitCode {
	crash_handler::hook();

	let config_kind = Config::load();
	let config = Config::new().clone();

	let cli = Cli::new();

	let is_managed = installer::is_managed();
	let installation = installer::verify(is_managed, config.install_plugin && !cli.is_dashboard());

	#[cfg(windows)]
	if cli.is_dashboard() {
		detach_console_if_double_clicked();
	}

	let yes = cli.yes();
	let backtrace = cli.backtrace();
	let mut verbosity = cli.verbosity();
	let log_style = cli.log_style();

	// The hub runs in the tray with no console, so it always keeps a log file
	// (~/.aragon/logs/hub.log) with at least info level: sync failures must be findable
	let log_file = cli
		.is_dashboard()
		.then(|| util::get_aragon_dir().ok())
		.flatten()
		.map(|dir| dir.join("logs").join("hub.log"));

	if log_file.is_some() && verbosity < LevelFilter::Info {
		verbosity = LevelFilter::Info;
	}

	if log_style == WriteStyle::Auto && io::stdin().is_terminal() {
		env::set_var("RUST_LOG_STYLE", "always");
	} else {
		env::set_var(
			"RUST_LOG_STYLE",
			match log_style {
				WriteStyle::Always => "always",
				_ => "never",
			},
		)
	}

	env::set_var("RUST_VERBOSE", verbosity.as_str());
	env::set_var("RUST_YES", if yes { "1" } else { "0" });
	env::set_var("RUST_BACKTRACE", if backtrace { "1" } else { "0" });

	logger::init_with_file(verbosity, log_style, log_file.as_deref());

	match config_kind {
		Ok(kind) => info!("{kind:?} config loaded"),
		Err(err) => error!("Failed to load config file: {err}"),
	}

	match installation {
		Ok(()) => info!("Aragon installation verified successfully!"),
		Err(err) => warn!("Failed to verify Aragon installation: {err}"),
	}

	let handle = thread::spawn(move || {
		if !is_managed && config.check_updates {
			match updater::check_for_updates(config.install_plugin, config.update_templates, !config.auto_update) {
				Ok(()) => info!("Update check completed successfully!"),
				Err(err) => warn!("Update check failed: {err}"),
			}
		}

		if config.share_stats {
			match stats::track() {
				Ok(()) => info!("Stat tracker initialized successfully!"),
				Err(err) => warn!("Failed to initialize stat tracker: {err}"),
			}
		}
	});

	if cfg!(debug_assertions) && cli.profile() {
		match Server::new(PROFILER_ADDRESS) {
			Ok(server) => {
				let _ = ManuallyDrop::new(server);

				info!("Profiler started at {PROFILER_ADDRESS}");
			}
			Err(err) => {
				error!("Failed to start profiler: {err}");
			}
		}

		puffin::set_scopes_on(true);
	}

	let exit_code = match cli.main() {
		Ok(()) => {
			debug!("Successfully executed command!");
			ExitCode::SUCCESS
		}
		Err(err) => {
			aragon_error!("{}", err);
			ExitCode::FAILURE
		}
	};

	handle.join().ok();
	stats::save().ok();

	exit_code
}
