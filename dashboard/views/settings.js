import { html, useState, useEffect } from "../vendor/preact-htm.js";
import { api, post, ago, useAction, toast } from "../lib.js";
import { Button, Switch, Icon, Thumb } from "../ui.js";

/// One control for any plugin setting kind (bool / number / enum)
export function SettingControl({ setting, value, onChange }) {
	const [draft, setDraft] = useState(String(value));

	useEffect(() => setDraft(String(value)), [value]);

	if (setting.kind === "bool") {
		return html`<${Switch} label=${setting.name} checked=${!!value} onChange=${onChange} />`;
	}

	if (setting.kind === "enum") {
		return html`<select class="select" value=${value} aria-label=${setting.name} onChange=${(e) => onChange(e.target.value)}>
			${setting.options.map((o) => html`<option value=${o}>${o}</option>`)}
		</select>`;
	}

	const commit = () => {
		const number = Number(draft);
		if (Number.isFinite(number) && number >= 0 && number !== value) onChange(Math.round(number));
		else setDraft(String(value));
	};

	return html`<input
		class="input narrow num"
		inputmode="numeric"
		aria-label=${setting.name}
		value=${draft}
		onInput=${(e) => setDraft(e.target.value.replace(/[^\d]/g, ""))}
		onBlur=${commit}
		onKeyDown=${(e) => e.key === "Enter" && e.target.blur()}
	/>`;
}

function Account({ state }) {
	const [key, setKey] = useState("");
	const [busy, run] = useAction();
	const account = state.account;

	const save = async (e) => {
		e.preventDefault();
		const ok = await run("key", () => post("/api/account/key", { apiKey: key }), "API key saved, loading private games…");
		if (ok) setKey("");
	};

	return html`<section class="settings-section" id="account">
		<header>
			<h2>Roblox account</h2>
			<p>
				Aragon learns who you are from Roblox Studio and loads your games, groups and places from Roblox's public
				APIs. An Open Cloud API key is optional: it adds private group experiences and the games your key is
				scoped to. It is stored in the Windows Credential Manager, never in a file.
			</p>
		</header>
		<div class="panel">
			${account
				? html`<div class="callout">
						${account.avatarUrl ? html`<img src=${account.avatarUrl} alt="" style="width:56px;border-radius:50%" />` : null}
						<div style="flex:1">
							<h2>${account.user.displayName || account.user.name}</h2>
							<p>@${account.user.name} · user ${account.user.id} · refreshed ${ago(account.lastRefresh)}</p>
							${account.lastError ? html`<p class="error-text">${account.lastError}</p>` : null}
						</div>
						<${Button} icon="refresh" busy=${busy === "refresh" || state.refresh.running} onClick=${() => run("refresh", () => post("/api/account/refresh"))}>
							${state.refresh.running ? state.refresh.stage : "Refresh"}
						<//>
					</div>`
				: html`<div class="callout">
						<img src="assets/key.webp" alt="" />
						<div>
							<h2>Not linked yet</h2>
							<p>Open any place in Roblox Studio with the Aragon plugin and your account links automatically.</p>
						</div>
					</div>`}
			<div class="setting">
				<div>
					<b><${Icon} name="key" /> Open Cloud API key <span class="badge">${state.hasApiKey ? "Saved" : "Optional"}</span></b>
					<p>
						Create one at${" "}
						<a href="#" onClick=${(e) => (e.preventDefault(), post("/api/open", { target: "https://create.roblox.com/dashboard/credentials" }))}>
							create.roblox.com → Credentials
						</a>. Any scope works for identity; add <span class="mono">legacy-group:manage</span> to include every group you manage.
					</p>
					${account && account.keyName ? html`<p class="faint">Key "${account.keyName}"${account.keyExpiration ? ` · expires ${new Date(account.keyExpiration).toLocaleDateString()}` : ""}</p>` : null}
				</div>
				${state.hasApiKey
					? html`<${Button}
							className="danger"
							busy=${busy === "remove"}
							onClick=${() => run("remove", () => api("/api/account/key", { method: "DELETE" }), "API key removed")}
						>
							Remove key
						<//>`
					: null}
			</div>
			<form class="key-form" onSubmit=${save}>
				<input
					class="input mono"
					type="password"
					autocomplete="off"
					placeholder=${state.hasApiKey ? "Paste a new key to replace the saved one" : "Paste your Open Cloud API key"}
					value=${key}
					onInput=${(e) => setKey(e.target.value)}
					aria-label="Open Cloud API key"
				/>
				<${Button} className="primary" type="submit" busy=${busy === "key"} disabled=${key.trim().length < 20}>Save key<//>
			</form>
		</div>
	</section>`;
}

function Projects({ state }) {
	const [busy, run] = useAction();
	const [templates, setTemplates] = useState([]);

	useEffect(() => {
		api("/api/templates").then(setTemplates, () => {});
	}, []);

	const setPref = (body, label) => run("pref", () => post("/api/prefs", body), label);

	const pick = async () => {
		const result = await run("pick", () => post("/api/pick-folder", { start: state.projectsRoot }));
		if (result && result.path) setPref({ projectsRoot: result.path }, "Projects folder updated");
	};

	return html`<section class="settings-section" id="projects">
		<header>
			<h2>Projects</h2>
			<p>Every game maps to its own folder: <span class="mono">root / owner / game</span>, other places of a game go in <span class="mono">places/</span>.</p>
		</header>
		<div class="panel">
			<div class="setting">
				<div>
					<b>Projects folder</b>
					<p class="mono">${state.projectsRoot}</p>
				</div>
				<div class="control">
					<${Button} icon="folder" onClick=${() => post("/api/open", { target: state.projectsRoot }).catch((e) => toast(e.message, "err"))}>Open<//>
					<${Button} busy=${busy === "pick"} onClick=${pick}>Change<//>
				</div>
			</div>
			<div class="setting">
				<div>
					<b>Default template</b>
					<p><span class="mono">quick</span> maps every service (best for existing games), <span class="mono">place</span> is a Client/Server/Shared layout.</p>
				</div>
				<select class="select" value=${state.prefs.template} onChange=${(e) => setPref({ template: e.target.value })} aria-label="Default template">
					${(templates.length ? templates : [state.prefs.template]).map((t) => html`<option value=${t}>${t}</option>`)}
				</select>
			</div>
			<div class="setting">
				<div>
					<b>Include every group I'm in</b>
					<p>By default only groups you own or manage are listed.</p>
				</div>
				<${Switch} label="Include every group" checked=${state.prefs.allGroups} onChange=${(v) => setPref({ allGroups: v }, "Saved, refresh to apply")} />
			</div>
			<div class="setting">
				<div>
					<b>Bring the dashboard up for new places</b>
					<p>When Studio opens a place that has no folder yet, focus this window for onboarding.</p>
				</div>
				<${Switch} label="Focus on onboarding" checked=${state.prefs.focusOnOnboarding} onChange=${(v) => setPref({ focusOnOnboarding: v })} />
			</div>
		</div>
	</section>`;
}

function Plugin({ state }) {
	const [busy, run] = useAction();
	const { schema, global } = state.pluginSettings;
	const groups = [...new Set(schema.map((s) => s.group))];

	const set = (key, value) => post("/api/plugin-settings", { key, value }).catch((e) => toast(e.message, "err"));

	return html`<section class="settings-section" id="plugin">
		<header>
			<h2>Studio plugin</h2>
			<p>The plugin only connects. Everything below is pushed to every open Studio instantly; places can override it on their Studio settings tab.</p>
		</header>
		<div class="panel">
			<div class="setting">
				<div>
					<b>
						Aragon plugin
						<span class=${`badge ${state.hub.pluginInstalled ? "ok" : "warn"}`}>${state.hub.pluginInstalled ? "Installed" : "Not installed"}</span>
					</b>
					<p class="mono">${state.hub.pluginPath || "Roblox Studio not found"}</p>
				</div>
				<${Button}
					className=${state.hub.pluginInstalled ? "" : "primary"}
					icon="download"
					busy=${busy === "install"}
					disabled=${!state.hub.pluginBundled}
					onClick=${() => run("install", () => post("/api/plugin/install"), "Plugin installed, restart Studio or reload plugins")}
				>
					${state.hub.pluginInstalled ? "Reinstall" : "Install"}
				<//>
			</div>
			<div class="setting">
				<div>
					<b>
						Roblox Studio
						<span class=${`badge ${state.hub.studioRunning ? "ok" : ""}`}>${state.hub.studioRestart || (state.hub.studioRunning ? "Running" : "Closed")}</span>
					</b>
					<p>Studio only loads plugins when it starts. Restart asks Studio to close normally (it still offers to save) and opens it again.</p>
				</div>
				${state.hub.studioRunning || state.hub.studioRestart
					? html`<${Button} icon="refresh" busy=${busy === "restart" || !!state.hub.studioRestart} onClick=${() => run("restart", () => post("/api/studio/restart"))}>Restart Studio<//>`
					: html`<${Button} icon="studio" busy=${busy === "open"} onClick=${() => run("open", () => post("/api/studio/open"), "Opening Roblox Studio…")}>Open Studio<//>`}
			</div>
		</div>
		${groups.map(
			(group) => html`<div class="panel" key=${group}>
				<div class="panel-head" style="padding-bottom:4px"><h3>${group}</h3></div>
				${schema
					.filter((s) => s.group === group)
					.map((setting) => {
						const changed = setting.key in global;
						const value = changed ? global[setting.key] : setting.default;

						return html`<div class="setting" key=${setting.key}>
							<div>
								<b>${setting.name} ${changed ? html`<span class="changed">changed</span>` : null}</b>
								<p>${setting.description}</p>
							</div>
							<div class="control">
								${changed ? html`<button type="button" class="btn ghost sm" onClick=${() => set(setting.key, null)}>Reset</button>` : null}
								<${SettingControl} setting=${setting} value=${value} onChange=${(v) => set(setting.key, v)} />
							</div>
						</div>`;
					})}
			</div>`,
		)}
	</section>`;
}

function Updates({ state }) {
	const [busy, run] = useAction();
	const update = state.update;
	const latest = update.latest;
	const available = update.status === "available" && latest;

	const check = () =>
		run("check", async () => {
			const result = await post("/api/update/check");
			toast(result.status === "available" ? `Aragon ${result.latest.version} is available` : result.status === "error" ? result.error : "You're on the latest version");
		});

	return html`<section class="settings-section" id="updates">
		<header>
			<h2>Updates</h2>
			<p>New versions show up in a corner card. One click downloads, verifies (SHA-256) and installs them, then Aragon restarts by itself and Studio reconnects.</p>
		</header>
		<div class="panel">
			<div class="setting">
				<div>
					<b>Aragon ${update.current} ${available ? html`<span class="changed">${latest.version} available</span>` : null}</b>
					<p>${update.lastCheck ? `Last checked ${ago(update.lastCheck)}` : "Not checked yet"}${update.status === "error" && update.error ? ` · ${update.error}` : ""}</p>
				</div>
				<div class="control">
					${available
						? html`<${Button} className="primary" icon="download" onClick=${() => run("install", () => post("/api/update/install"))}>Update to ${latest.version}<//>`
						: null}
					<${Button} icon="refresh" busy=${busy === "check" || update.status === "checking"} onClick=${check}>Check now<//>
				</div>
			</div>
			<div class="setting">
				<div>
					<b>Check automatically</b>
					<p>Looks for a new release every few hours. Nothing installs without your click.</p>
				</div>
				<${Switch}
					label="Check for updates automatically"
					checked=${state.prefs.checkUpdates}
					onChange=${(v) => run("auto", () => post("/api/prefs", { checkUpdates: v }))}
				/>
			</div>
		</div>
	</section>`;
}

function Removed({ state }) {
	const [busy, run] = useAction();
	const removed = [...state.removed].sort((a, b) => b.removedAt - a.removedAt);

	return html`<section class="settings-section" id="removed">
		<header>
			<h2>Removed places</h2>
			<p>
				Places you removed from the dashboard. They don't sync and nothing adds them back on its own: not Studio opening
				them, not a Roblox refresh. Add one back to sync it again (it gets a new setup), or forget it for good.
			</p>
		</header>
		<div class="panel">
			${removed.length
				? removed.map(
						(place) => html`<div class="setting" key=${place.key}>
							<div class="toolbar" style="flex-wrap:nowrap;gap:12px">
								<${Thumb} src=${place.iconUrl} className="removed-thumb" />
								<div>
									<b>${place.name || "Untitled place"}</b>
									<p>${place.placeId ? `place ${place.placeId} · ` : ""}removed ${ago(place.removedAt)}</p>
								</div>
							</div>
							<div class="control">
								<${Button}
									icon="refresh"
									busy=${busy === "restore" + place.key}
									onClick=${() => run("restore" + place.key, () => post(`/api/places/${place.key}/restore`), `${place.name || "Place"} is back`)}
								>
									Add back
								<//>
								<${Button}
									className="ghost"
									busy=${busy === "forget" + place.key}
									onClick=${() =>
										confirm(`Forget ${place.name || "this place"} completely? If Studio opens it again it shows up as a new place.`) &&
										run("forget" + place.key, () => api(`/api/removed/${place.key}`, { method: "DELETE" }), "Forgotten")}
								>
									Forget
								<//>
							</div>
						</div>`,
					)
				: html`<div class="setting"><div><b>Nothing removed</b><p>Right-click a place → Remove from dashboard.</p></div></div>`}
		</div>
	</section>`;
}

function Editors({ state }) {
	const [busy, run] = useAction();
	const editors = state.editors;
	const current = state.prefs.externalEditor || "builtin";

	return html`<section class="settings-section" id="editor">
		<header>
			<h2>Code editor</h2>
			<p>
				Where Studio's <i>Open In Editor</i>, error links in the Output and the command palette open files, at the right line.
				Aragon found ${editors.length ? editors.map((e) => e.name).join(", ") : "no external editor"} on this PC.
			</p>
		</header>
		<div class="panel">
			<div class="setting">
				<div>
					<b>Open scripts in</b>
					<p>Picking VS Code, Cursor or Windsurf opens the project's folder too, so the file lands in the right window.</p>
				</div>
				<div class="control">
					<select
						class="select"
						value=${current}
						aria-label="Open scripts in"
						onChange=${(e) => run("editor", () => post("/api/prefs", { externalEditor: e.target.value }), "Saved")}
					>
						<option value="builtin">Aragon's built-in editor</option>
						${editors.map((e) => html`<option value=${e.id}>${e.name}</option>`)}
					</select>
					<${Button} className="ghost icon" icon="refresh" aria-label="Look for editors again" title="Look for editors again" busy=${busy === "scan"} onClick=${() => run("scan", () => api("/api/editors"))} />
				</div>
			</div>
		</div>
	</section>`;
}

function Engine() {
	const [config, setConfig] = useState(null);
	const [filter, setFilter] = useState("");

	const load = () => api("/api/config").then(setConfig, (e) => toast(e.message, "err"));

	useEffect(() => {
		load();
	}, []);

	const set = async (key, value) => {
		try {
			await post("/api/config", { key, value });
			load();
		} catch (e) {
			toast(e.message, "err");
		}
	};

	if (!config) return html`<div class="skeleton" style="height:300px"></div>`;

	const settings = config.settings.filter(
		(s) => !filter || s.key.includes(filter.toLowerCase()) || s.doc.toLowerCase().includes(filter.toLowerCase()),
	);

	return html`<section class="settings-section" id="engine">
		<header class="toolbar" style="justify-content:space-between;align-items:flex-end">
			<div>
				<h2>Sync engine</h2>
				<p>Every setting of the Argon engine Aragon runs on. Saved to <span class="mono">~/.aragon/config.toml</span>.</p>
			</div>
			<input class="input" placeholder="Filter settings" value=${filter} onInput=${(e) => setFilter(e.target.value)} aria-label="Filter engine settings" />
		</header>
		<div class="panel">
			${settings.map((s) => {
				const changed = JSON.stringify(s.value) !== JSON.stringify(s.default);

				return html`<div class="setting" key=${s.key}>
					<div>
						<b><span class="mono" style="font-size:13px">${s.key}</span> ${changed ? html`<span class="changed">changed</span>` : null}</b>
						<p>${s.doc}</p>
					</div>
					<div class="control">
						${changed ? html`<button type="button" class="btn ghost sm" onClick=${() => set(s.key, s.default)}>Reset</button>` : null}
						${typeof s.default === "boolean"
							? html`<${Switch} label=${s.key} checked=${s.value} onChange=${(v) => set(s.key, v)} />`
							: html`<input
									class=${`input mono ${typeof s.default === "number" ? "narrow" : ""}`}
									value=${s.value}
									aria-label=${s.key}
									onKeyDown=${(e) => e.key === "Enter" && e.target.blur()}
									onBlur=${(e) => e.target.value !== String(s.value) && set(s.key, e.target.value)}
								/>`}
					</div>
				</div>`;
			})}
		</div>
	</section>`;
}

export function Settings({ state, params }) {
	useEffect(() => {
		const target = params[0] && document.getElementById(params[0]);
		if (target) target.scrollIntoView({ behavior: "smooth", block: "start" });
	}, [params[0]]);

	return html`<div class="page">
		<header class="page-head">
			<div class="toolbar" style="flex-wrap:nowrap;gap:14px">
				<img class="head-art mascot" src="assets/tools.webp" alt="" />
				<div>
					<h1>Settings</h1>
					<p>Everything Aragon does is configured here. Nothing to set up in Studio or any editor.</p>
				</div>
			</div>
		</header>
		<div class="settings-grid">
			<nav class="settings-nav" aria-label="Settings sections">
				<a href="#/settings/account">Roblox account</a>
				<a href="#/settings/projects">Projects</a>
				<a href="#/settings/plugin">Studio plugin</a>
				<a href="#/settings/editor">Code editor</a>
				<a href="#/settings/updates">Updates</a>
				<a href="#/settings/removed">Removed places${state.removed.length ? ` (${state.removed.length})` : ""}</a>
				<a href="#/settings/engine">Sync engine</a>
				<a href="#/settings/about">About</a>
			</nav>
			<div>
				<${Account} state=${state} />
				<${Projects} state=${state} />
				<${Plugin} state=${state} />
				<${Editors} state=${state} />
				<${Updates} state=${state} />
				<${Removed} state=${state} />
				<${Engine} />
				<section class="settings-section" id="about">
					<header>
						<h2>About</h2>
						<p>Aragon ${state.hub.version} · hub on localhost:${state.hub.port}. Built on the Argon sync engine by Dervex (Apache-2.0).</p>
					</header>
				</section>
			</div>
		</div>
	</div>`;
}
