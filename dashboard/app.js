import { html, render, useEffect, useState, useRef, useLayoutEffect } from "./vendor/preact-htm.js";
import { useHub, useRoute, go, Toasts } from "./lib.js";
import { Icon } from "./ui.js";
import { startBackground } from "./bg.js";
import { Home } from "./views/home.js";
import { Place } from "./views/place.js";
import { Analytics } from "./views/analytics.js";
import { Editor } from "./views/editor.js";
import { Settings } from "./views/settings.js";
import { Onboarding } from "./views/onboarding.js";
import { Welcome } from "./views/welcome.js";

function Rail({ state, online, route }) {
	const liveCount = state ? new Set(state.sessions.map((s) => s.placeKey)).size : 0;
	const pending = state ? state.onboarding.length : 0;
	const account = state && state.account;

	const nav = useRef(null);
	const [pill, setPill] = useState(null);
	const active = route.name === "place" || route.name === "setup" ? "home" : route.name;

	// Sliding indicator follows the active link (transform only)
	useLayoutEffect(() => {
		const link = nav.current && nav.current.querySelector('a[aria-current="page"]');
		setPill(link ? { y: link.offsetTop, h: link.offsetHeight } : null);
	}, [active]);

	const link = (name, href, icon, label, extra) => html`<a href=${href} aria-current=${active === name ? "page" : undefined}>
		<img src=${`assets/nav-${icon}.webp`} alt="" /><span>${label}</span>${extra || null}
	</a>`;

	return html`<aside class="rail">
		<div class="brand">
			<img src="assets/mark.webp" alt="" />
			<div>
				<strong>Aragon</strong>
				<span>Much sync. Very Roblox.</span>
			</div>
		</div>
		<nav class="nav" aria-label="Main" ref=${nav}>
			${pill ? html`<span class="nav-pill" style=${`--y:${pill.y}px;height:${pill.h}px`} aria-hidden="true"></span>` : null}
			${link("home", "#/", "places", "Places", pending ? html`<span class="count" title="Places waiting for setup">${pending}</span>` : null)}
			${link("analytics", "#/analytics", "analytics", "Analytics")}
			${link("editor", "#/editor", "code", "Code")}
			${link("settings", "#/settings", "settings", "Settings")}
		</nav>
		<div class="rail-foot">
			<div class="hub-card">
				<div class="hub-line">
					<span class=${`dot ${online ? "ok" : "err"}`}></span>
					${online ? html`Hub <b>live</b> on :${state ? state.hub.port : "…"}` : html`<b>Hub offline</b>`}
				</div>
				<div class="hub-line">
					<span class=${`dot ${liveCount ? "ok live" : ""}`}></span>
					${liveCount ? html`<b>${liveCount}</b> place${liveCount === 1 ? "" : "s"} in Studio` : state && state.hub.studioRunning ? "Studio open, no place synced" : "Studio not connected"}
				</div>
				${state && !state.hub.pluginInstalled
					? html`<div class="hub-line"><span class="dot warn"></span><a href="#/settings/plugin">Install the Studio plugin</a></div>`
					: null}
			</div>
			<a class="account-chip" href="#/settings/account">
				${account && account.avatarUrl ? html`<img src=${account.avatarUrl} alt="" />` : html`<span class="avatar-fallback"></span>`}
				<div>
					${account ? account.user.displayName || account.user.name : "Not linked"}
					<small>${account ? `@${account.user.name}` : "Open a place in Studio"}</small>
				</div>
			</a>
		</div>
	</aside>`;
}

function Loading({ online }) {
	return html`<div class="page">
		${online
			? html`<div class="skeleton" style="height:34px;width:220px"></div>
					<div class="skeleton" style="height:70px"></div>
					<div class="skeleton" style="height:260px"></div>`
			: html`<div class="empty panel">
					<img src="assets/sleepy.webp" alt="" />
					<h2>Connecting to the hub…</h2>
					<p>The Aragon hub runs inside this app. If this stays here, restart Aragon.</p>
				</div>`}
	</div>`;
}

function App() {
	const { state, online } = useHub();
	const route = useRoute();
	const [dismissed, setDismissed] = useState(() => new Set());
	const [welcomeDone, setWelcomeDone] = useState(false);
	const lastOpen = useRef(null);

	// Studio "Open In Editor" → jump to the file in the built-in editor
	useEffect(() => {
		const request = state && state.openRequest;
		if (!request || request.nonce === lastOpen.current) return;

		const first = lastOpen.current === null;
		lastOpen.current = request.nonce;

		// Ignore a stale request that was already there when the app loaded
		if (first && Date.now() - request.nonce > 10000) return;

		go(`#/editor/${request.key}/${request.path}`);
	}, [state && state.openRequest && state.openRequest.nonce]);

	// Title shows pending setups so it's visible from the taskbar
	useEffect(() => {
		const pending = state ? state.onboarding.length : 0;
		document.title = pending ? `(${pending}) Aragon` : "Aragon";
	}, [state && state.onboarding.length]);

	let onboardingKey = null;

	if (state) {
		if (route.name === "setup") onboardingKey = route.params[0];
		else onboardingKey = state.onboarding.find((key) => !dismissed.has(key)) || null;
	}

	const closeOnboarding = (dismiss, next) => {
		if (onboardingKey && dismiss) setDismissed(new Set([...dismissed, onboardingKey]));

		if (next) go(next);
		else if (route.name === "setup") history.length > 1 ? history.back() : go("#/");
	};

	let view = null;

	if (!state) view = html`<${Loading} online=${online} />`;
	else if (route.name === "place") view = html`<${Place} state=${state} params=${route.params} />`;
	else if (route.name === "analytics") view = html`<${Analytics} state=${state} />`;
	else if (route.name === "editor") view = html`<${Editor} state=${state} params=${route.params} />`;
	else if (route.name === "settings") view = html`<${Settings} state=${state} params=${route.params} />`;
	else view = html`<${Home} state=${state} />`;

	return html`<div class="shell">
		<${Rail} state=${state} online=${online} route=${route} />
		<main class="main" id="main">
			<div class="view" key=${route.name + "/" + (route.params[0] || "")}>${view}</div>
		</main>
		${state && !state.prefs.welcomed && !welcomeDone
			? html`<${Welcome} state=${state} onFinish=${() => setWelcomeDone(true)} />`
			: null}
		${onboardingKey && (state.prefs.welcomed || welcomeDone) ? html`<${Onboarding} key=${onboardingKey} state=${state} placeKey=${onboardingKey} onClose=${closeOnboarding} />` : null}
		<${Toasts} />
	</div>`;
}

startBackground();
render(html`<${App} />`, document.getElementById("app"));
