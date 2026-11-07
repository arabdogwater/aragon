import { html, useState, useMemo, useEffect, useRef } from "../vendor/preact-htm.js";
import { go, post, api, fmt, fmtMinutes, ago, agoMs, series, sum, placeStatus, useAction, toast, dayKey } from "../lib.js";
import { Icon, Thumb, Seg, Copyable, Sparkline, Button, openMenu } from "../ui.js";
import { placeMenu, groupOf, newGroup } from "./placemenu.js";
import { CountUp, useTilt } from "../motion.js";

function loadCollapsed() {
	try {
		return JSON.parse(localStorage.getItem("aragon.collapsed") || "{}");
	} catch {
		return {};
	}
}

function saveCollapsed(value) {
	try {
		localStorage.setItem("aragon.collapsed", JSON.stringify(value));
	} catch {}
}

/// Groups on the Places page: the user's own groups, then by owner (you, your groups, everyone else), in the
/// order the user dragged them. Places the user moved sit in the group they were dropped in, in that order
function groupPlaces(state, places, withEmpty) {
	const custom = new Map(state.groups.map((group) => [group.id, group]));
	const groups = new Map();

	for (const place of places) {
		const key = groupOf(state, place);
		if (!groups.has(key)) groups.set(key, []);
		groups.get(key).push(place);
	}

	// Empty groups the user made stay visible: they're drop targets
	if (withEmpty) for (const group of state.groups) if (!groups.has(group.id)) groups.set(group.id, []);

	const ranked = [...groups.entries()].map(([key, list]) => {
		const owner = state.owners[key];
		const group = custom.get(key);
		const rank = group ? -1 : !owner ? (key === "unsorted" ? 4 : 5) : owner.mine ? (owner.kind === "User" ? 0 : 1) : 3;

		list.sort(
			(a, b) =>
				(a.order ?? 1e9) - (b.order ?? 1e9) ||
				b.score - a.score ||
				(b.project ? 1 : 0) - (a.project ? 1 : 0) ||
				(b.isRoot ? 1 : 0) - (a.isRoot ? 1 : 0) ||
				a.name.localeCompare(b.name),
		);

		return {
			key,
			owner,
			custom: group,
			list,
			rank,
			name: group ? group.name : owner ? owner.name : key === "local" ? "Local files" : "Resolving…",
		};
	});

	const position = (group) => {
		const index = state.groupOrder.indexOf(group.key);
		return index >= 0 ? index : 10000 + group.rank * 100;
	};

	ranked.sort((a, b) => position(a) - position(b) || a.name.localeCompare(b.name));
	return ranked;
}

function greeting() {
	const hour = new Date().getHours();

	if (hour < 5) return "Burning the midnight oil";
	if (hour < 12) return "Good morning";
	if (hour < 18) return "Good afternoon";
	return "Good evening";
}

/// Consecutive days (ending today or yesterday) with any lines written or time synced
function streak(places) {
	const active = new Set();

	for (const place of places) {
		for (const day of place.stats.daily || []) {
			if (day.added > 0 || day.minutes > 0) active.add(day.day);
		}
	}

	const date = new Date();
	if (!active.has(dayKey(date))) date.setDate(date.getDate() - 1);

	let count = 0;

	while (active.has(dayKey(date))) {
		count++;
		date.setDate(date.getDate() - 1);
	}

	return count;
}

function Welcome({ state }) {
	const account = state.account;
	const name = account ? account.user.displayName || account.user.name : null;
	const today = dayKey(new Date());
	const todayStats = (field) =>
		sum(state.places.map((p) => ((p.stats.daily || []).find((d) => d.day === today) || {})[field] || 0));

	const live = new Set(state.sessions.map((s) => s.placeKey)).size;
	const lines = todayStats("added");
	const minutes = todayStats("minutes");
	const days = streak(state.places);

	const line = live
		? `${live === 1 ? "A place is" : `${live} places are`} open in Studio and syncing. Much code, very wow.`
		: lines
			? "Welcome back. Your code is right where you left it."
			: "Open a place in Roblox Studio and it connects on its own.";

	return html`<section class="welcome">
		<div class="copy-block">
			<h1>${greeting()}${name ? `, ${name}` : ""}</h1>
			<p>${line}</p>
			<div class="chips">
				<span class="chip"><span class=${`dot ${live ? "ok live" : ""}`}></span><b><${CountUp} value=${live} /></b> live</span>
				<span class="chip"><img src="assets/bone.webp" alt="" /><b><${CountUp} value=${lines} format=${fmt} /></b> lines today</span>
				<span class="chip"><${Icon} name="bolt" size=${15} /><b>${fmtMinutes(minutes)}</b> synced today</span>
				<span class="chip"><img src="assets/coin.webp" alt="" /><b><${CountUp} value=${days} /></b> day streak</span>
			</div>
		</div>
		<img class="mascot pop" src="assets/wave.webp" alt="" />
	</section>`;
}

function LiveStrip({ state }) {
	const sessions = state.sessions;

	if (!sessions.length) return null;

	const places = new Map(state.places.map((p) => [p.key, p]));
	const unique = [...new Map(sessions.map((s) => [s.placeKey, s])).values()];

	return html`<section>
		<div class="section-title">
			<h2>Open in Studio</h2>
			<span class="badge ok"><span class="dot ok live"></span>${unique.length} live</span>
			<span class="line"></span>
		</div>
		<div class="live-strip stagger">
			${unique.map((session, index) => {
				const place = places.get(session.placeKey);
				const status = place ? placeStatus(place, sessions) : { kind: "warn", label: "Connecting" };
				const needs = session.state === "Onboarding";

				return html`<button
					type="button"
					class=${`live-item ${needs ? "needs" : ""}`}
					key=${session.placeKey}
					style=${`--i:${index}`}
					onContextMenu=${(e) => place && openMenu(e, placeMenu(state, place))}
					onClick=${() => (needs ? go(`#/setup/${session.placeKey}`) : go(`#/place/${session.placeKey}`))}
				>
					<${Thumb} src=${place && place.iconUrl} />
					<div class="meta">
						<b>${(place && place.name) || session.placeName}</b>
						<small><span class=${`dot ${status.kind} live`}></span>${status.label}</small>
						${place && place.lastActivity
							? html`<small class="live-activity" title=${place.lastActivity.path}>
									${place.lastActivity.dir === "toStudio" ? "→ Studio" : "→ disk"} ${place.lastActivity.path.split("/").pop()} · ${agoMs(place.lastActivity.t)}
								</small>`
							: null}
						${place && (place.problems || []).length
							? html`<small class="live-problem" onClick=${(e) => (e.stopPropagation(), go(`#/place/${place.key}/problems`))}>⚠ ${place.problems.length} sync problem${place.problems.length === 1 ? "" : "s"}</small>`
							: null}
					</div>
					${needs ? html`<span class="btn primary sm">Set up</span>` : html`<${Icon} name="chevron" style="transform:rotate(-90deg)" />`}
				</button>`;
			})}
		</div>
	</section>`;
}

function TopPlaces({ places, state }) {
	const top = places
		.filter((p) => p.score > 0)
		.sort((a, b) => b.score - a.score)
		.slice(0, 5);

	if (!top.length) return null;

	return html`<section>
		<div class="section-title">
			<h2>Most worked on</h2>
			<span class="faint">last 14 days</span>
			<span class="line"></span>
		</div>
		<div class=${`top-grid stagger n${top.length}`}>
			${top.map((place, index) => html`<${TopTile} key=${place.key} place=${place} index=${index} state=${state} />`)}
		</div>
	</section>`;
}

function TopTile({ place, index, state }) {
	const tilt = useTilt(5);
	const week = series(place.stats.daily, 14, "added");
	const today = series(place.stats.daily, 1, "minutes")[0].value;

	return html`<button
		type="button"
		class="top-tile tilt"
		style=${`--i:${index}`}
		...${tilt}
		onClick=${() => go(`#/place/${place.key}`)}
		onContextMenu=${(e) => openMenu(e, placeMenu(state, place))}
	>
					<div class="art" style=${place.thumbnailUrl ? `background-image:url("${place.thumbnailUrl}")` : ""}></div>
					<span class="rank">#${index + 1}</span>
					<div class="body">
						<div class="title">
							<${Thumb} src=${place.iconUrl} />
							<div>
								<b>${place.name}</b>
								<span class="faint">${place.ownerName || "Unknown owner"}</span>
							</div>
						</div>
						<${Sparkline} values=${week.map((p) => p.value)} width=${index === 0 ? 300 : 180} height=${index === 0 ? 44 : 28} />
						<div class="kpis num">
							<span><b>${fmt(sum(week.map((p) => p.value)))}</b> lines written</span>
							<span><b>${fmtMinutes(today)}</b> today</span>
							${index === 0 ? html`<span><b>${fmt(place.stats.loc)}</b> total LOC</span>` : null}
						</div>
					</div>
				</button>`;
}

/// Folder shown relative to the projects root when it lives inside it
function shortPath(workspace, root) {
	if (!workspace) return "";

	const normalize = (p) => p.split("\\").join("/").replace(/\/+$/, "");
	const full = normalize(workspace);
	const base = normalize(root || "");

	return base && full.toLowerCase().startsWith(base.toLowerCase() + "/") ? full.slice(base.length + 1) : full;
}

function PlaceRow({ place, sessions, root, index, state, drag }) {
	const status = placeStatus(place, sessions);
	const trend = series(place.stats.daily, 14, "added").map((p) => p.value);
	const target = drag && drag.over && drag.over.key === place.key ? (drag.over.after ? "drop-after" : "drop-before") : "";

	return html`<div
		class=${`row ${drag && drag.dragging === place.key ? "dragging" : ""} ${target}`}
		style=${`--i:${index}`}
		role="link"
		tabindex="0"
		draggable=${drag ? "true" : "false"}
		onDragStart=${drag && ((e) => drag.start(e, place))}
		onDragEnd=${drag && drag.end}
		onDragOver=${drag && ((e) => drag.overRow(e, place))}
		onClick=${() => go(`#/place/${place.key}`)}
		onContextMenu=${(e) => openMenu(e, placeMenu(state, place))}
		onKeyDown=${(e) => {
			if (e.key === "Enter") go(`#/place/${place.key}`);
			if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) openMenu(e, placeMenu(state, place));
		}}
	>
		<${Thumb} src=${place.iconUrl} />
		<div class="name">
			<b>
				${status.live ? html`<span class=${`dot ${status.kind} live`} title=${status.label}></span>` : null}
				${place.name || "Untitled place"}
				${!place.isRoot && place.gameName ? html`<span class="faint">· ${place.gameName}</span>` : null}
				${place.privacy === "Private" ? html`<span class="badge">Private</span>` : null}
				${(place.problems || []).length
					? html`<span
							class="badge warn"
							title="Sync problems, click to see them"
							onClick=${(e) => (e.stopPropagation(), go(`#/place/${place.key}/problems`))}
						>⚠ ${place.problems.length}</span>`
					: null}
				${place.outputErrors ? html`<span class="badge err" title="Errors in Studio's output, last 10 minutes">${place.outputErrors} error${place.outputErrors === 1 ? "" : "s"}</span>` : null}
			</b>
			<div class="ids">
				${place.placeId ? html`<${Copyable} value=${place.placeId}>place ${place.placeId}<//>` : html`<span class="mono">local file</span>`}
				${place.universeId ? html`<${Copyable} value=${place.universeId}>universe ${place.universeId}<//>` : null}
			</div>
		</div>
		${place.project
			? html`<span class="path" title=${place.workspace}><${Icon} name="folder" size=${13} /> ${shortPath(place.workspace, root)}</span>`
			: html`<span>
					<button
						type="button"
						class="btn sm"
						onClick=${(e) => {
							e.stopPropagation();
							go(`#/setup/${place.key}`);
						}}
					>
						<${Icon} name="folder" /> Map folder
					</button>
				</span>`}
		<div class="stat num">
			<b>${place.project ? fmt(place.stats.loc) : "—"}</b>
			<small>LOC</small>
		</div>
		<div class="spark-cell" title="Lines written, last 14 days"><${Sparkline} values=${trend} width=${100} /></div>
		<div class="stat">
			<span class=${`badge ${status.kind}`}>${status.label}</span>
			<small style="display:block;margin-top:3px">${place.stats.lastActive ? ago(place.stats.lastActive) : ""}</small>
		</div>
	</div>`;
}

function AddPlace() {
	const [open, setOpen] = useState(false);
	const [value, setValue] = useState("");
	const [busy, run] = useAction();
	const input = useRef(null);

	useEffect(() => {
		if (open && input.current) input.current.focus();
	}, [open]);

	const submit = async (e) => {
		e.preventDefault();

		const id = Number(String(value).replace(/\D/g, ""));
		if (!id) return toast("Paste a place ID or a roblox.com/games link", "err");

		const result = await run("add", () => post("/api/places", { placeId: id }), "Place added, resolving…");
		if (result) {
			setOpen(false);
			setValue("");
		}
	};

	if (!open) {
		return html`<${Button} icon="plus" onClick=${() => setOpen(true)}>Add place<//>`;
	}

	return html`<form class="toolbar" onSubmit=${submit}>
		<input
			ref=${input}
			class="input mono"
			style="width:230px"
			placeholder="Place ID or game link"
			value=${value}
			onInput=${(e) => setValue(e.target.value.match(/games\/(\d+)/)?.[1] || e.target.value)}
			onKeyDown=${(e) => e.key === "Escape" && setOpen(false)}
		/>
		<${Button} className="primary" busy=${busy === "add"} type="submit">Add<//>
		<${Button} className="ghost icon" icon="x" aria-label="Cancel" onClick=${() => setOpen(false)} />
	</form>`;
}

export function Home({ state }) {
	const [query, setQuery] = useState("");
	const [filter, setFilter] = useState("all");
	const [collapsed, setCollapsed] = useState(loadCollapsed);
	const [busy, run] = useAction();
	const searchRef = useRef(null);

	// "/" focuses the search (Ctrl K is the command palette, everywhere)
	useEffect(() => {
		const onKey = (e) => {
			const typing = /input|textarea|select/i.test(document.activeElement && document.activeElement.tagName);

			if (e.key === "/" && !typing) {
				e.preventDefault();
				searchRef.current && searchRef.current.focus();
			}
		};

		addEventListener("keydown", onKey);
		return () => removeEventListener("keydown", onKey);
	}, []);

	const places = useMemo(() => {
		const q = query.trim().toLowerCase();

		return state.places.filter((p) => {
			if (filter === "mapped" && !p.project) return false;
			if (filter === "unmapped" && p.project) return false;
			if (filter === "live" && !state.sessions.some((s) => s.placeKey === p.key)) return false;
			if (!q) return true;

			return [p.name, p.gameName, p.ownerName, String(p.placeId), String(p.universeId || "")].some(
				(v) => v && v.toLowerCase().includes(q),
			);
		});
	}, [state.places, state.sessions, query, filter]);

	const arranging = filter === "all" && !query.trim();
	const groups = useMemo(() => groupPlaces(state, places, arranging), [places, state, arranging]);

	// Drag and drop: places between groups (and inside one), whole groups up and down
	const [dragging, setDragging] = useState(null);
	const [over, setOver] = useState(null);

	const endDrag = () => {
		setDragging(null);
		setOver(null);
	};

	const startDrag = (e, kind, key) => {
		e.stopPropagation();
		e.dataTransfer.effectAllowed = "move";
		e.dataTransfer.setData("text/plain", key);
		setDragging({ kind, key });
	};

	const overRow = (group, e, place) => {
		if (!dragging || dragging.kind !== "place") return;

		e.preventDefault();
		e.stopPropagation();

		const rect = e.currentTarget.getBoundingClientRect();
		const after = e.clientY > rect.top + rect.height / 2;

		if (!over || over.group !== group.key || over.key !== place.key || over.after !== after) setOver({ group: group.key, key: place.key, after });
	};

	const overGroup = (group, e) => {
		if (!dragging) return;
		e.preventDefault();

		if (dragging.kind === "place") {
			if (!over || over.group !== group.key || over.key) setOver({ group: group.key, key: null });
			return;
		}

		const rect = e.currentTarget.getBoundingClientRect();
		const after = e.clientY > rect.top + Math.min(rect.height / 2, 40);
		if (!over || over.groupKey !== group.key || over.after !== after) setOver({ groupKey: group.key, after });
	};

	const drop = (e) => {
		e.preventDefault();

		if (dragging && over && dragging.kind === "place" && over.group) {
			const target = groups.find((g) => g.key === over.group);
			const keys = target ? target.list.map((p) => p.key).filter((k) => k !== dragging.key) : [];
			let index = over.key ? keys.indexOf(over.key) + (over.after ? 1 : 0) : keys.length;

			if (index < 0) index = keys.length;
			keys.splice(index, 0, dragging.key);

			post("/api/places/arrange", { group: over.group, keys }).catch((err) => toast(err.message, "err"));
		} else if (dragging && over && dragging.kind === "group" && over.groupKey && over.groupKey !== dragging.key) {
			const keys = groups.map((g) => g.key).filter((k) => k !== dragging.key);
			keys.splice(keys.indexOf(over.groupKey) + (over.after ? 1 : 0), 0, dragging.key);

			post("/api/groups/order", { order: keys }).catch((err) => toast(err.message, "err"));
		}

		endDrag();
	};

	const groupMenu = (group) => [
		group.custom
			? {
					label: "Rename…",
					icon: "gear",
					run: () => {
						const name = prompt("Rename the group", group.name);
						if (name && name.trim()) post(`/api/groups/${encodeURIComponent(group.key)}`, { name }).catch((err) => toast(err.message, "err"));
					},
				}
			: null,
		group.custom
			? {
					label: "Delete group",
					icon: "trash",
					danger: true,
					run: () =>
						confirm(`Delete the group "${group.name}"? Its places go back under their owners.`) &&
						api(`/api/groups/${encodeURIComponent(group.key)}`, { method: "DELETE" }).catch((err) => toast(err.message, "err")),
				}
			: null,
		"-",
		{ label: collapsed[group.key] ? "Expand" : "Collapse", icon: "chevron", run: () => toggle(group.key) },
		{ label: "New group…", icon: "plus", run: () => newGroup() },
	];
	const mapped = state.places.filter((p) => p.project).length;
	const known = !!(state.account || state.studioUserId);

	const toggle = (key) => {
		const next = { ...collapsed, [key]: !collapsed[key] };
		setCollapsed(next);
		saveCollapsed(next);
	};

	return html`<div class="page">
		<${Welcome} state=${state} />

		<header class="page-head">
			<div>
				<h2>Places</h2>
				<p>
					${state.places.length} places · ${mapped} mapped to folders · drag to organize, right-click for more
					${state.refresh.running ? html` · <span class="muted">${state.refresh.stage}…</span>` : null}
				</p>
			</div>
			<div class="toolbar">
				<label class="search">
					<${Icon} name="search" />
					<input
						ref=${searchRef}
						class="input"
						type="search"
						placeholder="Search places, IDs, owners"
						value=${query}
						onInput=${(e) => setQuery(e.target.value)}
						aria-label="Search places"
					/>
					${query ? null : html`<kbd>/</kbd>`}
				</label>
				<${Seg}
					label="Filter places"
					value=${filter}
					onChange=${setFilter}
					options=${[
						{ value: "all", label: "All" },
						{ value: "live", label: "Live" },
						{ value: "mapped", label: "Mapped" },
						{ value: "unmapped", label: "Unmapped" },
					]}
				/>
				<${AddPlace} />
				<${Button} icon="folder" title="Make a group and drag places into it" onClick=${() => newGroup()}>New group<//>
				${known
					? html`<${Button}
							className="icon"
							icon="refresh"
							title="Refresh games from Roblox"
							aria-label="Refresh games from Roblox"
							busy=${busy === "refresh" || state.refresh.running}
							onClick=${() => run("refresh", () => post("/api/account/refresh"))}
						/>`
					: null}
			</div>
		</header>

		${!known
			? html`<section class="panel callout">
					<img src="assets/key.webp" alt="" />
					<div style="flex:1">
						<h2>Open any place in Roblox Studio</h2>
						<p>
							The Aragon plugin tells the dashboard who you are, then your games, groups and their places get
							mapped here automatically. Add an Open Cloud key in Settings to also pull in private group games.
						</p>
					</div>
					<a class="btn" href="#/settings/account">Account settings</a>
				</section>`
			: null}

		<${LiveStrip} state=${state} />
		${filter === "all" && !query ? html`<${TopPlaces} places=${state.places} state=${state} />` : null}

		${groups.length
			? groups.map(
					(group) => html`<section
						class=${`owner ${over && over.group === group.key && !over.key ? "drop-into" : ""} ${over && over.groupKey === group.key ? (over.after ? "drop-after" : "drop-before") : ""} ${dragging && dragging.key === group.key ? "dragging" : ""}`}
						key=${group.key}
						onDragOver=${arranging ? (e) => overGroup(group, e) : undefined}
						onDrop=${arranging ? drop : undefined}
					>
						<button
							type="button"
							class=${`owner-head ${group.owner && group.owner.kind === "User" ? "user" : ""} ${group.custom ? "custom" : ""}`}
							aria-expanded=${collapsed[group.key] ? "false" : "true"}
							draggable=${arranging ? "true" : "false"}
							onDragStart=${arranging ? (e) => startDrag(e, "group", group.key) : undefined}
							onDragEnd=${endDrag}
							onClick=${() => toggle(group.key)}
							onContextMenu=${(e) => openMenu(e, groupMenu(group))}
						>
							<${Icon} name="chevron" className="chev" />
							${group.custom
								? html`<span class="group-icon"><${Icon} name="folder" /></span>`
								: group.owner && group.owner.iconUrl
									? html`<img src=${group.owner.iconUrl} alt="" />`
									: html`<span class="avatar-fallback"></span>`}
							<h2>${group.name}</h2>
							${group.owner && group.owner.role ? html`<span class="badge">${group.owner.role}</span>` : null}
							<span class="faint">${group.list.length}</span>
						</button>
						${collapsed[group.key]
							? null
							: html`<div class="rows stagger">
									${group.list.map(
										(place, index) => html`<${PlaceRow}
											key=${place.key}
											index=${index}
											place=${place}
											state=${state}
											sessions=${state.sessions}
											root=${state.projectsRoot}
											drag=${arranging
												? {
														dragging: dragging && dragging.kind === "place" ? dragging.key : null,
														over: over && over.group === group.key ? over : null,
														start: (e, p) => startDrag(e, "place", p.key),
														end: endDrag,
														overRow: (e, p) => overRow(group, e, p),
													}
												: null}
										/>`,
									)}
									${!group.list.length ? html`<div class="drop-zone">Drag places here</div>` : null}
								</div>`}
					</section>`,
				)
			: html`<div class="empty panel">
					<img src=${state.places.length ? "assets/search.webp" : "assets/sleepy.webp"} alt="" />
					<h2>${state.places.length ? "Nothing matches" : "No places yet"}</h2>
					<p>
						${state.places.length
							? "Try another search or filter."
							: "Open a place in Roblox Studio with the Aragon plugin installed. It connects on its own and shows up here."}
					</p>
				</div>`}
	</div>`;
}
