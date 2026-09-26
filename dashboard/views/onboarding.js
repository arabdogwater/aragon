import { html, useState, useEffect, useRef } from "../vendor/preact-htm.js";
import { api, post, go, useAction, toast } from "../lib.js";
import { Button, Switch, Thumb, Copyable, Icon } from "../ui.js";
import { burst } from "../motion.js";

/// Shown when Studio opens a place that has no project folder yet (or from "Map folder")
export function Onboarding({ state, placeKey, onClose }) {
	const place = state.places.find((p) => p.key === placeKey);
	const dialog = useRef(null);
	const [busy, run] = useAction();
	const [templates, setTemplates] = useState([]);
	const [path, setPath] = useState("");
	const [mode, setMode] = useState("import");
	const [template, setTemplate] = useState(state.prefs.template || "quick");
	const [options, setOptions] = useState({ git: true, wally: false, selene: false, docs: false });
	const [showKey, setShowKey] = useState(false);
	const [apiKey, setApiKey] = useState("");
	const [error, setError] = useState(null);
	const [mapping, setMapping] = useState(false);
	const [done, setDone] = useState(null);

	useEffect(() => {
		api("/api/templates").then(setTemplates, () => {});
	}, []);

	// Suggested folder follows the place once Roblox resolved its owner/game
	const suggested = place && (place.project ? place.workspace : place.suggestedPath);
	const [touched, setTouched] = useState(false);

	useEffect(() => {
		if (!touched && suggested) setPath(suggested);
	}, [suggested, touched]);

	useEffect(() => {
		const element = dialog.current;
		if (element && !element.open) element.showModal();
	}, []);

	if (!place) return null;

	const live = state.sessions.some((s) => s.placeKey === place.key);

	const pick = async () => {
		const result = await run("pick", () => post("/api/pick-folder", { start: path || state.projectsRoot }));
		if (result && result.path) {
			setTouched(true);
			setPath(result.path);
		}
	};

	const submit = async (e) => {
		e.preventDefault();
		const submitter = e.submitter;
		setError(null);
		setMapping(true);

		if (apiKey.trim()) {
			const saved = await run("key", () => post("/api/account/key", { apiKey }));
			if (!saved) return setMapping(false);
		}

		try {
			await post(`/api/places/${place.key}/map`, {
				path,
				template,
				importFromStudio: mode === "import",
				...options,
			});
		} catch (err) {
			setError(err.message);
			setMapping(false);
			return;
		}

		setMapping(false);
		setDone({ path, live });

		const rect = submitter ? submitter.getBoundingClientRect() : null;
		burst(rect ? rect.left + rect.width / 2 : innerWidth / 2, rect ? rect.top : innerHeight / 2);

		setTimeout(() => {
			toast(live ? `${place.name} is syncing` : `${place.name} is mapped`);
			onClose(false, `#/place/${place.key}`);
		}, 2600);
	};

	const dismiss = () => {
		post(`/api/places/${place.key}/dismiss`).catch(() => {});
		onClose(true);
	};

	const flip = (key) => setOptions({ ...options, [key]: !options[key] });

	return html`<dialog
		class="onboard"
		ref=${dialog}
		aria-labelledby="onboard-title"
		onCancel=${(e) => {
			e.preventDefault();
			dismiss();
		}}
	>
		${done
			? html`<div class="onboard-success">
					<img class="pop" src="assets/celebrate.webp" alt="" />
					<h2>Much folder. Very sync.</h2>
					<p>${place.name} now lives in <span class="mono">${done.path}</span>${done.live ? ". Studio is connecting right now." : "."}</p>
				</div>`
			: html`<form class="onboard-grid" onSubmit=${submit}>
			<aside class="onboard-side">
				<h2 id="onboard-title">${place.project ? "Change the folder" : "New place, new folder"}</h2>
				<p>
					${live
						? "Studio just opened this place. Pick where its code lives and sync starts the moment you hit Create."
						: "Pick where this place's code lives. The next time Studio opens it, sync starts on its own."}
				</p>
				<img src="assets/onboarding.webp" alt="" />
			</aside>

			<div class="onboard-main">
				<div class="place-card">
					<${Thumb} src=${place.iconUrl} />
					<div style="flex:1;min-width:0">
						<b style="font-size:15px">${place.name || "Untitled place"}</b>
						<div class="faint" style="font-size:13px">
							${place.ownerName || (place.resolving ? "Looking up owner…" : "Unknown owner")}
							${!place.isRoot && place.gameName ? ` · ${place.gameName}` : ""}
						</div>
						<div class="toolbar faint" style="gap:12px;margin-top:2px">
							${place.placeId ? html`<${Copyable} value=${place.placeId}>place ${place.placeId}<//>` : html`<span class="mono">local file</span>`}
							${place.universeId ? html`<${Copyable} value=${place.universeId}>universe ${place.universeId}<//>` : null}
						</div>
					</div>
					${live ? html`<span class="badge ok"><span class="dot ok live"></span>In Studio</span>` : null}
				</div>

				<label class="field">
					<span>Project folder</span>
					<div class="path-input">
						<input
							class="input"
							value=${path}
							onInput=${(e) => {
								setTouched(true);
								setPath(e.target.value);
							}}
							spellcheck="false"
							required
						/>
						<${Button} icon="folder" busy=${busy === "pick"} onClick=${pick}>Browse<//>
					</div>
					<small>Grouped by owner automatically. An existing project in this folder is reused as is.</small>
				</label>

				<div class="field">
					<span>Starting point</span>
					<div class="choice">
						<button type="button" aria-pressed=${mode === "import" ? "true" : "false"} onClick=${() => setMode("import")}>
							<b><${Icon} name="import" /> Import from Studio</b>
							<small>Your current scripts are written into the folder. Best for existing games.</small>
						</button>
						<button type="button" aria-pressed=${mode === "fresh" ? "true" : "false"} onClick=${() => setMode("fresh")}>
							<b><${Icon} name="file" /> Files win</b>
							<small>The folder is the source of truth. Studio asks before replacing anything.</small>
						</button>
					</div>
				</div>

				<div class="field">
					<span>Template & tools</span>
					<div class="toolbar" style="gap:18px">
						<select class="select" value=${template} onChange=${(e) => setTemplate(e.target.value)} aria-label="Template">
							${(templates.length ? templates : [template]).map((t) => html`<option value=${t}>${t}</option>`)}
						</select>
						<div class="toggles">
							${[
								["git", "Git"],
								["wally", "Wally"],
								["selene", "selene"],
								["docs", "Docs"],
							].map(
								([key, label]) => html`<label class="toggle">
									<${Switch} label=${label} checked=${options[key]} onChange=${() => flip(key)} />${label}
								</label>`,
							)}
						</div>
					</div>
				</div>

				${state.hasApiKey
					? null
					: html`<div class="field">
							${showKey
								? html`<span>Open Cloud API key <span class="faint">(optional)</span></span>
										<input
											class="input mono"
											type="password"
											autocomplete="off"
											placeholder="Paste a key to also load your private group games"
											value=${apiKey}
											onInput=${(e) => setApiKey(e.target.value)}
										/>
										<small>Stored in Windows Credential Manager. You can add or remove it later in Settings.</small>`
								: html`<button type="button" class="btn ghost sm" style="justify-self:start" onClick=${() => setShowKey(true)}>
										<${Icon} name="key" /> Add an API key for private games (optional)
									</button>`}
						</div>`}

				${error ? html`<p class="error-text">${error}</p>` : null}

				<div class="onboard-foot">
					<span class="muted">Esc to do this later</span>
					<${Button} className="ghost" onClick=${dismiss}>Later<//>
					<${Button} className="primary" type="submit" busy=${mapping || busy === "key"} disabled=${!path.trim()}>
						${place.project ? "Save folder" : live ? "Create & sync" : "Create folder"}
					<//>
				</div>
			</div>
		</form>`}
	</dialog>`;
}
