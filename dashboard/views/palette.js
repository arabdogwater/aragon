// Ctrl+K: jump to any place, page, file (across every project) or action, from anywhere
import { html, useState, useEffect, useRef, useMemo } from "../vendor/preact-htm.js";
import { api, post, go, toast, openFile, editorName, placeStatus } from "../lib.js";
import { Icon, Thumb } from "../ui.js";

const listeners = new Set();

export function openPalette(query = "") {
	listeners.forEach((listener) => listener(query));
}

/// Fuzzy score of `query` against `text` (subsequence, word starts and prefixes win), null when no match
function score(query, text) {
	if (!query) return 0;

	const q = query.toLowerCase();
	const t = text.toLowerCase();
	const index = t.indexOf(q);

	if (index === 0) return 1000 - t.length;
	if (index > 0) return (t[index - 1] === " " || t[index - 1] === "/" ? 800 : 600) - t.length;

	let at = 0;
	let points = 0;

	for (const char of q) {
		const found = t.indexOf(char, at);
		if (found < 0) return null;

		points += found === at ? 3 : found > 0 && " /.-_".includes(t[found - 1]) ? 2 : 1;
		at = found + 1;
	}

	return points * 10 - t.length;
}

function buildCommands(state) {
	const commands = [
		{ group: "Go to", icon: "home", label: "Places", run: () => go("#/") },
		{ group: "Go to", icon: "chart", label: "Analytics", run: () => go("#/analytics") },
		{ group: "Go to", icon: "code", label: "Code editor", run: () => go("#/editor") },
		{ group: "Go to", icon: "gear", label: "Settings", run: () => go("#/settings") },
		{
			group: "Aragon",
			icon: "download",
			label: "Check for updates",
			run: () =>
				post("/api/update/check").then(
					(update) => toast(update.status === "available" ? `Aragon ${update.latest.version} is available` : "Aragon is up to date"),
					(err) => toast(err.message, "err"),
				),
		},
		{ group: "Aragon", icon: "studio", label: "Open Roblox Studio", run: () => post("/api/studio/open").catch((err) => toast(err.message, "err")) },
		{ group: "Aragon", icon: "box", label: "Reinstall the Studio plugin", run: () => post("/api/plugin/install").then(() => toast("Studio plugin installed"), (err) => toast(err.message, "err")) },
	];

	const editor = editorName(state);
	const action = (place, name, label) => () =>
		post(`/api/places/${place.key}/action`, { action: name }).then(
			() => label && toast(label),
			(err) => toast(err.message, "err"),
		);

	// Live and most-worked-on places first
	const places = [...state.places].sort((a, b) => b.live - a.live || b.score - a.score);

	for (const place of places) {
		const status = placeStatus(place, state.sessions);
		const name = place.name || "Untitled place";
		const thumb = place.iconUrl;

		commands.push({ group: "Places", thumb, label: name, hint: status.label, run: () => go(`#/place/${place.key}`) });

		if (place.placeId) commands.push({ group: "Places", icon: "studio", label: `Open ${name} in Studio`, run: action(place, "openStudio", "Opening Roblox Studio…") });

		if (!place.project) continue;

		commands.push(
			{ group: "Places", icon: "folder", label: `${name}: open folder`, run: action(place, "openFolder") },
			{ group: "Places", icon: "terminal", label: `${name}: Studio output`, run: () => go(`#/place/${place.key}/output`) },
			{ group: "Places", icon: "bolt", label: `${name}: live sync activity`, run: () => go(`#/place/${place.key}/activity`) },
			{ group: "Places", icon: "refresh", label: `${name}: history (restore files)`, run: () => go(`#/place/${place.key}/history`) },
			{ group: "Places", icon: "refresh", label: `${name}: reconnect Studio`, run: action(place, "reconnect", "Reconnecting…") },
			{ group: "Places", icon: "box", label: `${name}: build .rbxl`, run: action(place, "build", "Built") },
		);

		if (editor) {
			commands.push({
				group: "Places",
				icon: "external",
				label: `Open ${name} in ${editor}`,
				run: () => post(`/api/places/${place.key}/open`, {}).catch((err) => toast(err.message, "err")),
			});
		}
	}

	return commands;
}

export function CommandPalette({ state }) {
	const [open, setOpen] = useState(false);
	const [query, setQuery] = useState("");
	const [files, setFiles] = useState([]);
	const [active, setActive] = useState(0);
	const input = useRef(null);
	const list = useRef(null);

	useEffect(() => {
		const show = (initial) => {
			setQuery(initial || "");
			setActive(0);
			setOpen(true);
		};

		listeners.add(show);

		const onKey = (e) => {
			const key = e.key.toLowerCase();

			// Ctrl K inside the code editor stays Monaco's (Ctrl K Ctrl C comments a line); Ctrl P works there
			const inCode = e.target && e.target.closest && e.target.closest(".monaco-editor");

			if ((e.ctrlKey || e.metaKey) && ((key === "k" && !inCode) || (key === "p" && !e.shiftKey))) {
				e.preventDefault();
				setOpen((was) => !was);
				setQuery("");
				setActive(0);
			}
		};

		addEventListener("keydown", onKey, true);

		return () => {
			listeners.delete(show);
			removeEventListener("keydown", onKey, true);
		};
	}, []);

	useEffect(() => {
		if (open && input.current) input.current.focus();
	}, [open]);

	// Files across every mapped project, as you type
	useEffect(() => {
		if (!open || query.trim().length < 2) {
			setFiles([]);
			return;
		}

		let stale = false;
		const timer = setTimeout(() => {
			api(`/api/files?q=${encodeURIComponent(query.trim())}`).then(
				(hits) => !stale && setFiles(hits),
				() => {},
			);
		}, 120);

		return () => {
			stale = true;
			clearTimeout(timer);
		};
	}, [query, open]);

	const commands = useMemo(() => (open ? buildCommands(state) : []), [open, state]);

	const results = useMemo(() => {
		const q = query.trim();
		const matched = commands
			.map((command) => ({ command, score: score(q, command.label) }))
			.filter((entry) => entry.score !== null)
			.sort((a, b) => b.score - a.score)
			.slice(0, q ? 12 : 9)
			.map((entry) => entry.command);

		const fileItems = files.slice(0, 12).map((hit) => ({
			group: "Files",
			icon: "file",
			label: hit.path.split("/").pop(),
			hint: `${hit.place} · ${hit.path}`,
			run: () => openFile(state, hit.key, hit.path, 1),
		}));

		return q ? [...matched.slice(0, 8), ...fileItems] : matched;
	}, [commands, files, query]);

	useEffect(() => {
		const item = list.current && list.current.querySelector('[aria-selected="true"]');
		if (item) item.scrollIntoView({ block: "nearest" });
	}, [active, results]);

	if (!open) return null;

	const run = (item) => {
		setOpen(false);
		item.run();
	};

	const onKeyDown = (e) => {
		if (e.key === "Escape") {
			e.preventDefault();
			setOpen(false);
		} else if (e.key === "ArrowDown") {
			e.preventDefault();
			setActive((i) => Math.min(results.length - 1, i + 1));
		} else if (e.key === "ArrowUp") {
			e.preventDefault();
			setActive((i) => Math.max(0, i - 1));
		} else if (e.key === "Enter" && results[active]) {
			e.preventDefault();
			run(results[active]);
		}
	};

	let lastGroup = null;

	return html`<div class="palette-backdrop" onMouseDown=${(e) => e.target === e.currentTarget && setOpen(false)}>
		<div class="palette" role="dialog" aria-label="Command palette">
			<label class="palette-input">
				<${Icon} name="search" size=${18} />
				<input
					ref=${input}
					value=${query}
					placeholder="Jump to a place, file or action…"
					aria-label="Search"
					onInput=${(e) => {
						setQuery(e.target.value);
						setActive(0);
					}}
					onKeyDown=${onKeyDown}
				/>
				<kbd>Esc</kbd>
			</label>
			<div class="palette-list" role="listbox" ref=${list}>
				${results.length
					? results.map((item, index) => {
							const header = item.group !== lastGroup ? html`<div class="palette-group">${item.group}</div>` : null;
							lastGroup = item.group;

							return html`${header}<button
									type="button"
									role="option"
									aria-selected=${index === active ? "true" : "false"}
									class="palette-item"
									onMouseMove=${() => index !== active && setActive(index)}
									onClick=${() => run(item)}
								>
									${item.thumb !== undefined ? html`<${Thumb} src=${item.thumb} />` : html`<span class="palette-icon"><${Icon} name=${item.icon} /></span>`}
									<span class="palette-label">${item.label}</span>
									${item.hint ? html`<span class="palette-hint">${item.hint}</span>` : null}
								</button>`;
						})
					: html`<div class="palette-empty">${query.trim().length < 2 ? "Type to search places, actions and files" : "Nothing matches"}</div>`}
			</div>
			<div class="palette-foot"><span><kbd>↑</kbd><kbd>↓</kbd> move</span><span><kbd>Enter</kbd> open</span><span><kbd>Ctrl</kbd><kbd>K</kbd> anywhere</span></div>
		</div>
	</div>`;
}
