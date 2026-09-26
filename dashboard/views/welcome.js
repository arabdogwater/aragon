import { html, useState, useEffect } from "../vendor/preact-htm.js";
import { post, useAction } from "../lib.js";
import { Button, Icon } from "../ui.js";
import { burst } from "../motion.js";

const STEPS = ["hello", "plugin", "studio", "done"];

function Dots({ step }) {
	return html`<div class="welcome-dots" aria-label=${`Step ${STEPS.indexOf(step) + 1} of ${STEPS.length}`}>
		${STEPS.map((s) => html`<span class=${s === step ? "on" : STEPS.indexOf(s) < STEPS.indexOf(step) ? "done" : ""}></span>`)}
	</div>`;
}

/// First run: welcome, install the Studio plugin, restart Studio, done
export function Welcome({ state, onFinish }) {
	const [step, setStep] = useState("hello");
	const [busy, run] = useAction();
	const [installedNow, setInstalledNow] = useState(false);
	const [restartRequested, setRestartRequested] = useState(false);

	const hub = state.hub;
	const pluginReady = hub.pluginInstalled && (installedNow || hub.pluginInstalledVersion === hub.pluginBundledVersion);
	const restarting = !!hub.studioRestart;
	// Studio only counts as ready once it restarted after the plugin install
	const studioReady = hub.studioRunning && !restarting && (restartRequested || !installedNow);

	// Once Studio has restarted with the plugin, move on by itself
	useEffect(() => {
		if (step === "studio" && restartRequested && !restarting && hub.studioRunning) {
			const id = setTimeout(() => setStep("done"), 1200);
			return () => clearTimeout(id);
		}
	}, [step, restartRequested, restarting, hub.studioRunning]);

	useEffect(() => {
		if (step === "done") burst(innerWidth / 2, innerHeight / 2 + 40, 90);
	}, [step]);

	const install = async (e) => {
		// Read the button position now, currentTarget is gone after the await
		const rect = e.currentTarget ? e.currentTarget.getBoundingClientRect() : null;
		const ok = await run("install", () => post("/api/plugin/install"));

		if (ok) {
			setInstalledNow(true);
			burst(rect ? rect.left + rect.width / 2 : innerWidth / 2, rect ? rect.top : innerHeight / 2, 40);
			setTimeout(() => setStep("studio"), 900);
		}
	};

	const restart = async () => {
		const ok = await run("restart", () => post("/api/studio/restart"));
		if (ok) setRestartRequested(true);
	};

	const openStudio = async () => {
		const ok = await run("open", () => post("/api/studio/open"));
		if (ok) setTimeout(() => setStep("done"), 1500);
	};

	const finish = () => {
		post("/api/prefs", { welcomed: true }).catch(() => {});
		onFinish();
	};

	let art = "wave";
	let body = null;

	if (step === "hello") {
		art = "wave";
		body = html`
			<h1>Welcome to Aragon!</h1>
			<p class="lead">Much sync. Very Roblox. Aragon keeps your Roblox games and their code on your PC in sync, with no editor and no terminal.</p>
			<ul class="perks">
				<li><img src="assets/nav-places.webp" alt="" /><span><b>Every game gets its own folder.</b> Open a place in Studio and it's set up and syncing.</span></li>
				<li><img src="assets/nav-code.webp" alt="" /><span><b>All your games in one window.</b> No more one editor window per game.</span></li>
				<li><img src="assets/bone.webp" alt="" /><span><b>Light as a treat.</b> Aragon naps in the tray while it syncs.</span></li>
			</ul>
			<div class="welcome-actions">
				<${Button} className="primary big" onClick=${() => setStep(pluginReady ? "studio" : "plugin")}>Let's go<//>
			</div>
		`;
	} else if (step === "plugin") {
		art = "tools";
		body = html`
			<h1>Install the Studio plugin</h1>
			<p class="lead">
				Aragon needs its little helper inside Roblox Studio. It only connects Studio to this app; everything else
				happens here. It's already packed inside Aragon, so one click does it.
			</p>
			<div class="install-card">
				<img src="assets/mark.webp" alt="" />
				<div>
					<b>Aragon Studio plugin</b>
					<span class="faint">v${hub.pluginBundledVersion} · goes into your Roblox Plugins folder</span>
				</div>
				${pluginReady
					? html`<span class="badge ok"><${Icon} name="check" size=${14} /> Installed</span>`
					: null}
			</div>
			<div class="welcome-actions">
				${pluginReady
					? html`<${Button} className="primary big" onClick=${() => setStep("studio")}>Next<//>`
					: html`<${Button} className="primary big" icon="download" busy=${busy === "install"} disabled=${!hub.pluginBundled} onClick=${install}>
							Install plugin
						<//>`}
			</div>
			${hub.pluginBundled ? null : html`<p class="error-text">This build has no bundled plugin. Rebuild Aragon with the plugin (see README).</p>`}
		`;
	} else if (step === "studio") {
		art = hub.studioRunning || restarting ? "search" : "laptop";
		body = html`
			<h1>${hub.studioRunning || restarting ? "Restart Roblox Studio" : "Open Roblox Studio"}</h1>
			<p class="lead">
				${hub.studioRunning || restarting
					? "Studio is open, but it only loads new plugins when it starts. Aragon can restart it for you. Studio still asks to save anything unsaved first."
					: "Open any place in Roblox Studio. The plugin connects on its own and the place shows up here, ready to map to a folder."}
			</p>
			<ul class="checklist">
				<li class=${hub.pluginInstalled ? "ok" : ""}>
					<span class="tick">${hub.pluginInstalled ? html`<${Icon} name="check" size=${16} />` : null}</span>
					Aragon plugin installed
					<span class="faint">${hub.pluginInstalled ? `v${hub.pluginInstalledVersion || hub.pluginBundledVersion}` : "missing"}</span>
				</li>
				<li class=${studioReady ? "ok" : ""}>
					<span class="tick">${studioReady ? html`<${Icon} name="check" size=${16} />` : null}</span>
					Roblox Studio
					<span class="faint">${restarting ? hub.studioRestart : hub.studioRunning ? (restartRequested ? "Restarted" : "Running (needs a restart)") : "Not running"}</span>
				</li>
				<li class=${state.sessions.length ? "ok" : ""}>
					<span class="tick">${state.sessions.length ? html`<${Icon} name="check" size=${16} />` : null}</span>
					A place connected
					<span class="faint">${state.sessions.length ? `${new Set(state.sessions.map((s) => s.placeKey)).size} live` : "Open any place"}</span>
				</li>
			</ul>
			${restarting ? html`<div class="restart-status"><span class="btn-spin"></span>${hub.studioRestart}…</div>` : null}
			<div class="welcome-actions">
				${hub.studioRunning || restarting
					? html`<${Button} className="primary big" icon="refresh" busy=${busy === "restart" || restarting} onClick=${restart}>Restart Studio<//>`
					: html`<${Button} className="primary big" icon="studio" busy=${busy === "open"} onClick=${openStudio}>Open Roblox Studio<//>`}
				<${Button} className="ghost" onClick=${() => setStep("done")}>I'll do it myself<//>
			</div>
		`;
	} else {
		art = "celebrate";
		body = html`
			<h1>You're all set!</h1>
			<p class="lead">
				Open a place in Studio and Aragon greets it here. Your games, groups and places fill in on their own. Add an
				Open Cloud key in Settings any time to include private group games.
			</p>
			<div class="welcome-actions">
				<${Button} className="primary big" onClick=${finish}>Take me home<//>
			</div>
		`;
	}

	return html`<div class="welcome-screen" role="dialog" aria-modal="true" aria-labelledby="welcome-title">
		<div class="welcome-card" key=${step}>
			<div class="welcome-art">
				<img class="mascot pop" src=${`assets/${art}.webp`} alt="" />
			</div>
			<div class="welcome-body" id="welcome-title">
				<${Dots} step=${step} />
				${body}
			</div>
		</div>
		${step !== "done" ? html`<button type="button" class="welcome-skip" onClick=${finish}>Skip setup</button>` : null}
	</div>`;
}
