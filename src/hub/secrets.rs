//! The Open Cloud API key lives in the OS credential store (Windows Credential
//! Manager / macOS Keychain / Secret Service), never in plain-text config

use anyhow::Result;
use keyring::Entry;

const SERVICE: &str = "Aragon";
const ACCOUNT: &str = "open-cloud-api-key";

fn entry() -> Result<Entry> {
	Ok(Entry::new(SERVICE, ACCOUNT)?)
}

pub fn get_api_key() -> Option<String> {
	entry().ok()?.get_password().ok().filter(|key| !key.is_empty())
}

pub fn set_api_key(key: &str) -> Result<()> {
	entry()?.set_password(key.trim())?;
	Ok(())
}

pub fn delete_api_key() -> Result<()> {
	match entry()?.delete_credential() {
		Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
		Err(err) => Err(err.into()),
	}
}
