use anyhow::{bail, Result};
use clap::Parser;
use colored::Colorize;
use reqwest::blocking::Client;
use std::{thread, time::Duration};

use crate::{
	aragon_info,
	hub::{self, Hub},
	server,
};

/// Open the Aragon dashboard and start the hub Studio connects to (default)
#[derive(Parser, Default)]
pub struct Dashboard {
	/// Hub port the Studio connector knocks on
	#[arg(short = 'P', long)]
	port: Option<u16>,

	/// Run the hub without a tray icon or window (servers, CI)
	#[arg(long)]
	headless: bool,

	/// Start in the system tray without opening the dashboard window
	#[arg(short, long)]
	background: bool,

	/// Load the dashboard UI from this URL instead (UI development)
	#[arg(long, hide = true)]
	ui_url: Option<String>,
}

impl Dashboard {
	pub fn main(self) -> Result<()> {
		let port = self.port.unwrap_or(hub::HUB_PORT);

		// Single instance: if a hub already runs, just bring its window up
		if !server::is_port_free("localhost", port) {
			let client = Client::builder().timeout(Duration::from_secs(2)).build()?;
			let url = server::format_address("127.0.0.1", port);

			let is_aragon = client
				.get(format!("{url}/api/ping"))
				.send()
				.and_then(|response| response.json::<serde_json::Value>())
				.is_ok_and(|body| body["app"] == "aragon");

			if is_aragon {
				client.post(format!("{url}/api/focus")).send().ok();
				aragon_info!("Aragon is already running, brought the dashboard to front");
				return Ok(());
			}

			bail!(
				"Port {} is taken by another program. Start Aragon with {} and set the same port in the Studio plugin",
				port.to_string().bold(),
				"--port <PORT>".bold()
			);
		}

		// The hub has no terminal to ask questions in: every Argon prompt (e.g.
		// "apply 20 000 changes from Studio?") takes its default answer instead of
		// blocking a sync thread forever. Set before any hub thread starts
		std::env::set_var("RUST_YES", "1");

		let hub = Hub::new(port);
		hub.start_background();

		let server_hub = hub.clone();

		thread::Builder::new().name("hub-server".into()).spawn(move || {
			if let Err(err) = server_hub.start_server() {
				crate::aragon_error!("Hub server stopped: {}", err);
				std::process::exit(1);
			}
		})?;

		aragon_info!(
			"Aragon hub is running on {} - Studio will connect automatically",
			hub.hub_url().bold()
		);

		if self.headless {
			loop {
				thread::park();
			}
		}

		hub::window::run(hub, self.ui_url, !self.background)
	}
}
