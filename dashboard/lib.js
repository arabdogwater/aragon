// Shared plumbing: hub API, live state store, router, formatting, toasts
import { html, useEffect, useState, useCallback, useRef } from "./vendor/preact-htm.js";

// API ----------------------------------------------------------------------

export async function api(path, { method = "GET", body } = {}) {
	const response = await fetch(path, {
		method,
		headers: body !== undefined ? { "Content-Type": "application/json" } : undefined,
		body: body !== undefined ? JSON.stringify(body) : undefined,
	});

	let data = null;

	try {
		data = await response.json();
	} catch {
		data = null;
	}

	if (!response.ok || (data && data.error)) {
		throw new Error((data && data.error) || `Request failed (${response.status})`);
	}

	return data;
}

export const post = (path, body = {}) => api(path, { method: "POST", body });

// Live state ------------------------------------------------------------------
// The hub long-polls /api/state?v=<version>: it answers immediately when the
// version moved, otherwise holds the request until something changes.

const listeners = new Set();
let current = null;
let online = false;

function publish() {
	for (const listener of listeners) listener();
}

async function pollLoop() {
	let backoff = 500;

	for (;;) {
		try {
			const version = current ? current.version : null;
			const next = await api(`/api/state${version ? `?v=${version}` : ""}`);

			current = next;
			online = true;
			backoff = 500;
			publish();
		} catch {
			if (online) {
				online = false;
				publish();
			}

			await new Promise((resolve) => setTimeout(resolve, backoff));
			backoff = Math.min(backoff * 2, 5000);
		}
	}
}

pollLoop();

export function refreshState() {
	return api("/api/state").then((state) => {
		current = state;
		publish();
	});
}

export function useHub() {
	const [, force] = useState(0);

	useEffect(() => {
		const listener = () => force((n) => n + 1);
		listeners.add(listener);

		// State may have arrived before this component subscribed
		if (current) listener();

		return () => listeners.delete(listener);
	}, []);

	return { state: current, online };
}

// Router --------------------------------------------------------------------

export function useRoute() {
	const [hash, setHash] = useState(location.hash || "#/");

	useEffect(() => {
		const onChange = () => setHash(location.hash || "#/");
		addEventListener("hashchange", onChange);
		return () => removeEventListener("hashchange", onChange);
	}, []);

	const parts = hash.replace(/^#\/?/, "").split("/").map(decodeURIComponent);
	return { name: parts[0] || "home", params: parts.slice(1) };
}

export const go = (path) => {
	location.hash = path;
};

// Toasts ----------------------------------------------------------------------

const toastListeners = new Set();
let toasts = [];

export function toast(message, kind = "ok") {
	const id = Math.random().toString(36).slice(2);
	toasts = [...toasts, { id, message, kind }];
	toastListeners.forEach((l) => l());

	setTimeout(() => {
		toasts = toasts.filter((t) => t.id !== id);
		toastListeners.forEach((l) => l());
	}, 4200);
}

export function Toasts() {
	const [, force] = useState(0);

	useEffect(() => {
		const listener = () => force((n) => n + 1);
		toastListeners.add(listener);
		return () => toastListeners.delete(listener);
	}, []);

	return html`<div class="toasts" role="status" aria-live="polite">
		${toasts.map(
			(t) => html`<div class="toast" key=${t.id}>
				<span class=${`dot ${t.kind}`}></span>${t.message}
			</div>`,
		)}
	</div>`;
}

/// Runs an async action with busy state + error toast
export function useAction() {
	const [busy, setBusy] = useState(null);

	const run = useCallback(async (name, fn, success) => {
		setBusy(name);

		try {
			const result = await fn();
			if (success) toast(success);
			return result;
		} catch (err) {
			toast(err.message || String(err), "err");
			return undefined;
		} finally {
			setBusy(null);
		}
	}, []);

	return [busy, run];
}

// Formatting ------------------------------------------------------------------

const compact = new Intl.NumberFormat(undefined, { notation: "compact", maximumFractionDigits: 1 });
const full = new Intl.NumberFormat();

export const fmt = (n) => (n == null ? "—" : n >= 10000 ? compact.format(n) : full.format(n));
export const fmtFull = (n) => (n == null ? "—" : full.format(n));

export function fmtMinutes(minutes) {
	if (!minutes) return "0m";
	if (minutes < 60) return `${minutes}m`;

	const hours = Math.floor(minutes / 60);
	const rest = minutes % 60;

	return hours < 24 ? `${hours}h${rest ? ` ${rest}m` : ""}` : `${fmt(hours)}h`;
}

export function ago(timestamp) {
	if (!timestamp) return "never";

	const seconds = Math.max(0, Date.now() / 1000 - timestamp);

	if (seconds < 45) return "just now";
	if (seconds < 3600) return `${Math.round(seconds / 60)}m ago`;
	if (seconds < 86400) return `${Math.round(seconds / 3600)}h ago`;
	if (seconds < 86400 * 30) return `${Math.round(seconds / 86400)}d ago`;

	return new Date(timestamp * 1000).toLocaleDateString();
}

export function dayKey(date) {
	const pad = (n) => String(n).padStart(2, "0");
	return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
}

/// Continuous series for the last `days` days from the sparse daily stats
export function series(daily, days, field) {
	const byDay = new Map((daily || []).map((d) => [d.day, d]));
	const result = [];

	for (let i = days - 1; i >= 0; i--) {
		const date = new Date();
		date.setDate(date.getDate() - i);

		const key = dayKey(date);
		const entry = byDay.get(key);

		result.push({ day: key, date, value: entry ? entry[field] || 0 : 0, entry });
	}

	return result;
}

export function sum(values) {
	return values.reduce((a, b) => a + b, 0);
}

export function copy(text) {
	navigator.clipboard.writeText(String(text)).then(
		() => toast(`Copied ${text}`),
		() => toast("Couldn't copy", "err"),
	);
}

/// Place status for dots/badges
export function placeStatus(place, sessions) {
	const session = sessions.find((s) => s.placeKey === place.key);

	if (session) {
		if (session.state === "Mapped") return { kind: "ok", label: "Syncing", live: true };
		if (session.state === "Onboarding") return { kind: "warn", label: "Needs setup", live: true };
		if (session.state === "Connecting") return { kind: "warn", label: "Connecting", live: true };
		if (session.state === "Paused") return { kind: "", label: "Paused", live: true };
		return { kind: "err", label: "Error", live: true, message: session.message };
	}

	if (!place.project) return { kind: "", label: "Not mapped" };
	if (!place.projectExists) return { kind: "err", label: "Folder missing" };
	if (!place.syncEnabled) return { kind: "", label: "Paused" };

	return { kind: "", label: "Mapped" };
}

export function useInterval(fn, ms) {
	const saved = useRef(fn);
	saved.current = fn;

	useEffect(() => {
		const id = setInterval(() => saved.current(), ms);
		return () => clearInterval(id);
	}, [ms]);
}
