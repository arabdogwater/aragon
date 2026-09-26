//! Schema of the Studio plugin settings the dashboard manages. Mirrors
//! `DEFAULTS` in `plugin/src/Config.luau` (connection settings excluded,
//! the connector always connects to the hub on its own).

use serde::Serialize;
use serde_json::{json, Value};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginSetting {
	pub key: &'static str,
	pub name: &'static str,
	pub group: &'static str,
	pub description: &'static str,
	pub default: Value,
	/// `bool`, `number` or `enum`
	pub kind: &'static str,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub options: Vec<&'static str>,
}

pub fn plugin_settings() -> Vec<PluginSetting> {
	fn setting(
		key: &'static str,
		name: &'static str,
		group: &'static str,
		description: &'static str,
		default: Value,
		options: Vec<&'static str>,
	) -> PluginSetting {
		let kind = if !options.is_empty() {
			"enum"
		} else if default.is_boolean() {
			"bool"
		} else {
			"number"
		};

		PluginSetting {
			key,
			name,
			group,
			description,
			default,
			kind,
			options,
		}
	}

	vec![
		setting(
			"InitialSyncPriority",
			"Initial sync priority",
			"Sync",
			"Which side wins when Studio connects. Client = Studio is written to files (safe default), Server = files overwrite Studio.",
			json!("Client"),
			vec!["Server", "Client", "None"],
		),
		setting(
			"LiveHydrate",
			"Live hydrate",
			"Sync",
			"If an instance doesn't exist in Studio yet, hydrate it from the files automatically.",
			json!(true),
			vec![],
		),
		setting(
			"KeepUnknowns",
			"Keep unknowns",
			"Sync",
			"Keep Studio instances that don't exist in the file system instead of removing them.",
			json!(true),
			vec![],
		),
		setting(
			"TwoWaySync",
			"Two-way sync",
			"Syncback",
			"Write changes made in Studio back to your files.",
			json!(false),
			vec![],
		),
		setting(
			"SyncbackProperties",
			"Syncback properties",
			"Syncback",
			"Also sync non-script properties back to files (scripts always sync).",
			json!(false),
			vec![],
		),
		setting(
			"OnlyCodeMode",
			"Only code mode",
			"Syncback",
			"Only sync back scripts and instances that contain scripts.",
			json!(true),
			vec![],
		),
		setting(
			"OpenInEditor",
			"Open in editor",
			"Syncback",
			"Opening a synced script in Studio opens it in the Aragon dashboard editor instead.",
			json!(false),
			vec![],
		),
		setting(
			"DisplayPrompts",
			"Confirmation prompts",
			"Safety",
			"When Studio asks before applying big changes.",
			json!("Always"),
			vec!["Always", "Initial", "Never"],
		),
		setting(
			"ChangesThreshold",
			"Changes threshold",
			"Safety",
			"How many changes can be applied at once before Studio asks for confirmation.",
			json!(5),
			vec![],
		),
		setting(
			"OverridePackages",
			"Override packages",
			"Safety",
			"Allow writing to instances controlled by a PackageLink.",
			json!(true),
			vec![],
		),
		setting(
			"DiffLinesLimit",
			"Diff lines limit",
			"Studio",
			"Maximum number of lines rendered in the script diff view.",
			json!(3000),
			vec![],
		),
		setting(
			"LogLevel",
			"Log level",
			"Studio",
			"How much the plugin prints to the Studio output.",
			json!("Warn"),
			vec!["Off", "Error", "Warn", "Info", "Debug", "Trace"],
		),
	]
}

/// Validates and normalizes a value for a setting, `None` when invalid
pub fn coerce(key: &str, value: &Value) -> Option<Value> {
	let setting = plugin_settings().into_iter().find(|setting| setting.key == key)?;

	match setting.kind {
		"bool" => value.as_bool().map(Value::Bool),
		"number" => value
			.as_f64()
			.or_else(|| value.as_str().and_then(|s| s.parse().ok()))
			.filter(|n| n.is_finite() && *n >= 0.0)
			.map(|n| json!(n.round() as u64)),
		"enum" => value
			.as_str()
			.filter(|s| setting.options.contains(s))
			.map(|s| Value::String(s.to_owned())),
		_ => None,
	}
}

/// Defaults <- global overrides <- place overrides
pub fn merged(global: &BTreeMap<String, Value>, place: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
	let mut result = BTreeMap::new();

	for setting in plugin_settings() {
		let value = place
			.get(setting.key)
			.or_else(|| global.get(setting.key))
			.cloned()
			.unwrap_or(setting.default);

		result.insert(setting.key.to_owned(), value);
	}

	result
}
