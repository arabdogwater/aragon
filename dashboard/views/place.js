import { html, useState } from "../vendor/preact-htm.js";
import { go, post, api, fmt, fmtFull, fmtMinutes, ago, series, sum, placeStatus, useAction, toast } from "../lib.js";
import { Icon, Thumb, Copyable, Button, BarChart, Languages, Switch, Seg } from "../ui.js";
import { CountUp } from "../motion.js";
import { SettingControl } from "./settings.js";

function Overview({ place }) {
	const [range, setRange] = useState(30);
	const added = series(place.stats.daily, range, "added");
	const minutes = series(place.stats.daily, range, "minutes");
	const addedTotal = sum(added.map((p) => p.value));
	const minutesTotal = sum(minutes.map((p) => p.value));

	return html`<div style="display:grid;gap:14px">
		<div class="stats-row num stagger">
			<div class="stat-cell"><span>Lines of code</span><b><${CountUp} value=${place.stats.loc} format=${fmt} /></b><small>${fmt(place.stats.files)} script files</small></div>
			<div class="stat-cell"><span>Written · ${range}d</span><b><${CountUp} value=${addedTotal} format=${fmt} /></b><small>${fmt(place.stats.linesAdded)} all time</small></div>
			<div class="stat-cell"><span>Time synced · ${range}d</span><b>${fmtMinutes(minutesTotal)}</b><small>${fmtMinutes(place.stats.minutes)} all time</small></div>
			<div class="stat-cell"><span>Last active</span><b>${ago(place.stats.lastActive)}</b><small>${fmt(place.stats.syncs)} syncs</small></div>
		</div>
		<div class="split">
			<section class="panel">
				<div class="panel-head">
					<div>
						<h2>Lines written per day</h2>
						<p>Growth of every script file between scans, counted without blank lines</p>
					</div>
					<${Seg}
						label="Range"
						value=${range}
						onChange=${setRange}
						options=${[
							{ value: 14, label: "14d" },
							{ value: 30, label: "30d" },
							{ value: 90, label: "90d" },
						]}
					/>
				</div>
				<${BarChart}
					points=${added}
					unit="lines"
					describe=${(p) => (p.entry ? `${fmtMinutes(p.entry.minutes)} in Studio · ${fmtFull(p.entry.removed)} removed` : "No activity")}
				/>
			</section>
			<section class="panel">
				<div class="panel-head"><h2>Languages</h2></div>
				<${Languages} languages=${place.stats.languages} />
				<div class="panel-head" style="padding-top:4px"><h2>Details</h2></div>
				<dl class="kv">
					<dt>Place ID</dt>
					<dd>${place.placeId ? html`<${Copyable} value=${place.placeId} />` : "Local file"}</dd>
					<dt>Universe ID</dt>
					<dd>${place.universeId ? html`<${Copyable} value=${place.universeId} />` : "—"}</dd>
					<dt>Owner</dt>
					<dd>${place.ownerName || "—"} ${place.ownerType ? html`<span class="faint">(${place.ownerType})</span>` : null}</dd>
					<dt>Visits</dt>
					<dd class="num">${fmt(place.visits)}</dd>
					<dt>Playing now</dt>
					<dd class="num">${fmt(place.playing)}</dd>
					<dt>Project file</dt>
					<dd class="mono">${place.project || "Not mapped"}</dd>
				</dl>
			</section>
		</div>
	</div>`;
}

function StudioSettings({ place, state }) {
	const schema = state.pluginSettings.schema;
	const global = state.pluginSettings.global;
	const overrides = place.pluginOverrides || {};

	const set = (key, value) =>
		post(`/api/places/${place.key}/options`, { pluginOverrides: { [key]: value } }).catch((err) => toast(err.message, "err"));

	return html`<section class="panel">
		<div class="panel-head">
			<div>
				<h2>Studio sync settings for this place</h2>
				<p>Overrides the global plugin settings only for ${place.name}. Changes reach Studio instantly.</p>
			</div>
		</div>
		${schema.map((setting) => {
			const inherited = setting.key in global ? global[setting.key] : setting.default;
			const overridden = setting.key in overrides;
			const value = overridden ? overrides[setting.key] : inherited;

			return html`<div class="setting" key=${setting.key}>
				<div>
					<b>${setting.name} ${overridden ? html`<span class="changed">overridden</span>` : null}</b>
					<p>${setting.description}</p>
				</div>
				<div class="control">
					${overridden
						? html`<button type="button" class="btn ghost sm" onClick=${() => set(setting.key, null)}>Use global</button>`
						: null}
					<${SettingControl} setting=${setting} value=${value} onChange=${(v) => set(setting.key, v)} />
				</div>
			</div>`;
		})}
	</section>`;
}

function RunCode({ place }) {
	const [code, setCode] = useState('print("Hello from Aragon 🐕")');
	const [busy, run] = useAction();
	const live = place.syncedClients > 0;

	const exec = () => run("exec", () => post(`/api/places/${place.key}/action`, { action: "exec", code }), "Sent to Studio");

	return html`<section class="panel">
		<div class="panel-head">
			<div>
				<h2>Run in Studio</h2>
				<p>Runs Luau in the connected Studio (edit mode), like the command bar. Output shows in Studio's Output window.</p>
			</div>
			<span class=${`badge ${live ? "ok" : ""}`}>${live ? "Studio connected" : "No Studio syncing"}</span>
		</div>
		<div class="console">
			<textarea
				spellcheck="false"
				value=${code}
				onInput=${(e) => setCode(e.target.value)}
				onKeyDown=${(e) => {
					if ((e.ctrlKey || e.metaKey) && e.key === "Enter") exec();
				}}
				aria-label="Luau code"
			></textarea>
			<div class="toolbar" style="justify-content:flex-end">
				<span class="faint">Ctrl Enter</span>
				<${Button} className="primary" icon="play" busy=${busy === "exec"} disabled=${!live} onClick=${exec}>Run<//>
			</div>
		</div>
	</section>`;
}

export function Place({ state, params }) {
	const [key, tabParam] = params;
	const place = state.places.find((p) => p.key === key);
	const [busy, run] = useAction();
	const tab = tabParam || "overview";

	if (!place) {
		return html`<div class="page">
			<div class="empty panel">
				<img src="assets/sleepy.webp" alt="" />
				<h2>Place not found</h2>
				<p>It may have been removed from the dashboard.</p>
				<a class="btn" href="#/">Back to places</a>
			</div>
		</div>`;
	}

	const status = placeStatus(place, state.sessions);
	const action = (name, label, extra = {}) =>
		run(name, () => post(`/api/places/${place.key}/action`, { action: name, ...extra }), label);

	const build = async () => {
		const result = await action("build");
		if (result && result.output) toast(`Built ${result.output}`);
	};

	const setTab = (next) => go(`#/place/${place.key}/${next}`);

	return html`<div class="page">
		<a href="#/" class="faint" style="display:inline-flex;align-items:center;gap:4px"><${Icon} name="back" />Places</a>

		<section class="hero">
			<div class="art" style=${place.thumbnailUrl ? `background-image:url("${place.thumbnailUrl}")` : ""}></div>
			<div class="hero-body">
				<${Thumb} src=${place.iconUrl} />
				<div class="hero-meta">
					<div class="toolbar">
						<h1>${place.name || "Untitled place"}</h1>
						<span class=${`badge ${status.kind}`}>${status.live ? html`<span class=${`dot ${status.kind} live`}></span>` : null}${status.label}</span>
						${place.privacy === "Private" ? html`<span class="badge">Private</span>` : null}
					</div>
					<div class="facts">
						${!place.isRoot && place.gameName ? html`<span>in ${place.gameName}</span>` : null}
						<span>${place.ownerName || "Owner unknown"}</span>
						${place.placeId ? html`<${Copyable} value=${place.placeId}>place ${place.placeId}<//>` : null}
						${place.universeId ? html`<${Copyable} value=${place.universeId}>universe ${place.universeId}<//>` : null}
					</div>
					${status.message ? html`<p class="error-text">${status.message}</p>` : null}
				</div>
				<div class="hero-actions">
					${place.project
						? html`
								<${Button} icon="folder" onClick=${() => action("openFolder")}>Folder<//>
								<${Button} icon="code" onClick=${() => go(`#/editor/${place.key}`)}>Code<//>
							`
						: html`<${Button} className="primary" icon="folder" onClick=${() => go(`#/setup/${place.key}`)}>Map folder<//>`}
					${place.placeId
						? html`<${Button} className=${place.project ? "primary" : ""} icon="studio" onClick=${() => action("openStudio", "Opening Roblox Studio…")}>Open in Studio<//>`
						: null}
				</div>
			</div>
		</section>

		<div class="toolbar" style="justify-content:space-between">
			<div class="tabs" role="tablist">
				${[
					["overview", "Overview"],
					["sync", "Sync"],
					["studio", "Studio settings"],
					["run", "Run code"],
				].map(
					([id, label]) => html`<button type="button" role="tab" aria-selected=${tab === id ? "true" : "false"} onClick=${() => setTab(id)}>${label}</button>`,
				)}
			</div>
			<div class="toolbar">
				${place.universeId
					? html`<${Button} className="ghost sm" icon="external" onClick=${() => action("openCreatorHub")}>Creator Hub<//>`
					: null}
				${place.placeId ? html`<${Button} className="ghost sm" icon="external" onClick=${() => action("openBrowser")}>Roblox page<//>` : null}
			</div>
		</div>

		${tab === "overview" ? html`<${Overview} place=${place} />` : null}
		${tab === "studio" ? html`<${StudioSettings} place=${place} state=${state} />` : null}
		${tab === "run" ? html`<${RunCode} place=${place} />` : null}
		${tab === "sync"
			? html`<section class="panel">
					<div class="setting">
						<div>
							<b>Sync enabled</b>
							<p>When off, Studio stays connected to the hub but nothing syncs for this place.</p>
						</div>
						<${Switch}
							label="Sync enabled"
							checked=${place.syncEnabled}
							onChange=${(v) => run("sync", () => post(`/api/places/${place.key}/options`, { syncEnabled: v }))}
						/>
					</div>
					<div class="setting">
						<div>
							<b>Sourcemap</b>
							<p>Keep sourcemap.json up to date for Luau LSP and other tools.</p>
						</div>
						<${Switch}
							label="Sourcemap"
							checked=${place.sourcemap}
							onChange=${(v) => run("sourcemap", () => post(`/api/places/${place.key}/options`, { sourcemap: v }))}
						/>
					</div>
					<div class="setting">
						<div>
							<b>Import from Studio</b>
							<p>Pull the current Studio scripts into the project folder on the next connection (Studio wins once).</p>
						</div>
						<${Button} icon="import" busy=${busy === "import"} disabled=${!place.project} onClick=${() => action("import", "Importing from Studio…")}>Import<//>
					</div>
					<div class="setting">
						<div>
							<b>Reconnect Studio</b>
							<p>Drops the current sync and lets Studio connect again with fresh settings.</p>
						</div>
						<${Button} icon="refresh" busy=${busy === "reconnect"} onClick=${() => action("reconnect", "Reconnecting…")}>Reconnect<//>
					</div>
					<div class="setting">
						<div>
							<b>Build place file</b>
							<p>Builds the project into build/(project name).rbxl inside the project folder.</p>
						</div>
						<${Button} icon="box" busy=${busy === "build"} disabled=${!place.project} onClick=${build}>Build .rbxl<//>
					</div>
					<div class="setting">
						<div>
							<b>Install Wally packages</b>
							<p>Runs wally install in the project folder.</p>
						</div>
						<${Button} icon="download" busy=${busy === "wally"} disabled=${!place.project} onClick=${() => action("wally", "Wally packages installed")}>Wally install<//>
					</div>
					<div class="setting">
						<div>
							<b>Rescan analytics</b>
							<p>Recount lines of code now instead of waiting for the next scan.</p>
						</div>
						<${Button} icon="chart" busy=${busy === "rescan"} disabled=${!place.project} onClick=${() => action("rescan", "Rescanned")}>Rescan<//>
					</div>
					<div class="setting">
						<div>
							<b>Project folder</b>
							<p class="mono">${place.project || "Not mapped yet"}</p>
						</div>
						<div class="control">
							${place.project
								? html`<${Button} onClick=${() => go(`#/setup/${place.key}`)}>Change<//>
										<${Button}
											className="danger"
											busy=${busy === "unmap"}
											onClick=${() =>
												confirm("Unmap this place? The folder and files stay on disk.") &&
												run("unmap", () => post(`/api/places/${place.key}/unmap`), "Unmapped")}
										>
											Unmap
										<//>`
								: html`<${Button} className="primary" onClick=${() => go(`#/setup/${place.key}`)}>Map folder<//>`}
						</div>
					</div>
					<div class="setting">
						<div>
							<b>Remove from dashboard</b>
							<p>Forgets this place and its analytics. Files on disk are not touched.</p>
						</div>
						<${Button}
							className="danger"
							icon="trash"
							onClick=${async () => {
								if (!confirm(`Remove ${place.name} from Aragon?`)) return;
								await run("remove", () => api(`/api/places/${place.key}`, { method: "DELETE" }), "Removed");
								go("#/");
							}}
						>
							Remove
						<//>
					</div>
				</section>`
			: null}
	</div>`;
}
