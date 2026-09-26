use anyhow::Result;
use clap::Parser;
use colored::Colorize;

use crate::aragon_info;

const LINK: &str = env!("CARGO_PKG_HOMEPAGE");

/// Open Aragon's documentation in the browser
#[derive(Parser)]
pub struct Doc {}

impl Doc {
	pub fn main(self) -> Result<()> {
		aragon_info!("Launched browser. Manually go to: {}", LINK.bold());

		open::that(LINK)?;

		Ok(())
	}
}
