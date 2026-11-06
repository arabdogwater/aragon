// What happened to a place: live sync activity, Studio's output, sync problems and the safety net (history)
import { html, useState, useEffect, useRef, useMemo } from "../vendor/preact-htm.js";
import { api, post, toast, agoMs, fmtBytes, usePoll, openFile, copy } from "../lib.js";
import { Icon, Button, Seg } from "../ui.js";
import { loadMonaco, languageOf } from "./editor.js";

// Activity -------------------------------------------------------------------------

const KIND_LABEL = {
	created: "created",
	updated: "edited",
	removed: "deleted",
	renamed: "renamed",
	changed: "edited",
};

export function ActivityPanel({ place, state }) {
	const [items, setItems] = useState(null);

	usePoll(async () => setItems(await api(`/api/places/${place.key}/activity`)), 1200, [place.key]);

	const lastTo = (dir) => items && items.find((item) => item.dir === dir);
	const toStudio = lastTo("toStudio");
	const toDisk = lastTo("toDisk");

	return html`<div style="display:grid;gap:14px">
		<div class="stats-row stagger">
			<div class="stat-cell">
				<span>Files → Studio</span>
				<b>${toStudio ? agoMs(toStudio.t) : "—"}</b>
				<small>${toStudio ? toStudio.path : "Edit a file (or let your AI edit one): it shows up here"}</small>
			</div>
			<div class="stat-cell">
				<span>Studio → files</span>
				<b>${toDisk ? agoMs(toDisk.t) : "—"}</b>
				<small>${toDisk ? toDisk.path : "Edit a script in Studio: it shows up here"}</small>
			</div>
			<div class="stat-cell">
				<span>Studio</span>
				<b>${place.syncedClients > 0 ? "Connected" : "Not syncing"}</b>
				<small>${place.syncedClients > 0 ? "Changes flow both ways live" : "Open the place in Studio to sync"}</small>
			</div>
		</div>
		<section class="panel">
			<div class="panel-head">
				<div>
					<h2>Live sync activity</h2>
					<p>Every file synced this session, in which direction. Answers "did that change reach Studio?" at a glance.</p>
				</div>
			</div>
			${!items
				? html`<div style="padding:0 16px 16px;display:grid;gap:6px">${[1, 2, 3].map(() => html`<div class="skeleton" style="height:30px"></div>`)}</div>`
				: !items.length
					? html`<div class="empty"><img src="assets/sleepy.webp" alt="" /><h2>Quiet so far</h2><p>Nothing synced since Aragon started. Save a file or edit a script in Studio and it lands here instantly.</p></div>`
					: html`<ul class="activity">
							${items.map(
								(item) => html`<li key=${item.t + item.path} class=${item.dir}>
									<span class="activity-dir" title=${item.dir === "toStudio" ? "Disk → Studio" : "Studio → disk"}>
										<${Icon} name=${item.dir === "toStudio" ? "studio" : "folder"} size=${14} />
									</span>
									<button
										type="button"
										class="activity-path mono"
										disabled=${item.kind === "removed"}
										onClick=${() => openFile(state, place.key, item.path, 1)}
										title=${item.kind === "removed" ? "Deleted" : "Open"}
									>
										${item.path}
									</button>
									<span class=${`badge ${item.kind === "removed" ? "err" : item.kind === "created" ? "ok" : ""}`}>
										${item.dir === "toStudio" ? "→ Studio" : "→ disk"} · ${KIND_LABEL[item.kind] || item.kind}${item.count > 1 ? ` ×${item.count}` : ""}
									</span>
									<span class="faint num">${agoMs(item.t)}</span>
								</li>`,
							)}
						</ul>`}
		</section>
	</div>`;
}

// Output ---------------------------------------------------------------------------

function OutputText({ line, onLink }) {
	if (!line.links.length) return line.text;

	const parts = [];
	let at = 0;

	for (const link of [...line.links].sort((a, b) => a.start - b.start)) {
		if (link.start < at) continue;

		parts.push(line.text.slice(at, link.start));
		parts.push(html`<button type="button" class="out-link" title=${`${link.file}:${link.line}`} onClick=${() => onLink(link)}>
			${line.text.slice(link.start, link.end)}
		</button>`);
		at = link.end;
	}

	parts.push(line.text.slice(at));
	return parts;
}

function clock(t) {
	return new Date(t).toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

/// Errors with the files they point at, ready to paste into an AI chat
function forAI(place, lines) {
	const errors = lines.filter((l) => l.kind === "error" || l.kind === "warn").slice(-25);
	if (!errors.length) return null;

	const body = errors
		.map((l) => {
			const where = l.links[0] ? ` (${l.links[0].file}:${l.links[0].line})` : "";
			return `- [${l.context}] ${l.kind.toUpperCase()}: ${l.text.trim()}${where}`;
		})
		.join("\n");

	return `Roblox Studio output for "${place.name}" (latest ${errors.length} errors and warnings, file paths are relative to the project):\n\n${body}`;
}

export function useOutput(place, active = true) {
	const [lines, setLines] = useState([]);
	const last = useRef(0);

	useEffect(() => {
		setLines([]);
		last.current = 0;
	}, [place.key]);

	usePoll(
		async () => {
			if (!active) return;

			const next = await api(`/api/places/${place.key}/output?since=${last.current}`);
			if (!next.length) return;

			last.current = next[next.length - 1].id;
			setLines((prev) => [...prev, ...next].slice(-2000));
		},
		700,
		[place.key, active],
	);

	const clear = () => {
		post(`/api/places/${place.key}/output/clear`).catch(() => {});
		setLines([]);
	};

	return [lines, clear];
}

export function OutputLines({ place, state, lines, filter = "all", context = "all", query = "", compact = false }) {
	const box = useRef(null);
	const stick = useRef(true);

	const shown = useMemo(() => {
		const q = query.trim().toLowerCase();

		return lines.filter(
			(line) =>
				(filter === "all" || (filter === "errors" ? line.kind === "error" : line.kind === "error" || line.kind === "warn")) &&
				(context === "all" || line.context === context) &&
				(!q || line.text.toLowerCase().includes(q)),
		);
	}, [lines, filter, context, query]);

	// Follow new lines unless the user scrolled up to read
	useEffect(() => {
		if (box.current && stick.current) box.current.scrollTop = box.current.scrollHeight;
	}, [shown.length]);

	const onScroll = () => {
		const el = box.current;
		stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 40;
	};

	return html`<div class=${`output ${compact ? "compact" : ""}`} ref=${box} onScroll=${onScroll}>
		${shown.length
			? shown.map(
					(line) => html`<div class=${`out-line ${line.kind}`} key=${line.id}>
						<span class="out-time">${clock(line.t)}</span>
						<span class=${`out-ctx ${line.context.toLowerCase()}`}>${line.context}</span>
						<span class="out-text"><${OutputText} line=${line} onLink=${(link) => openFile(state, place.key, link.file, link.line)} /></span>
					</div>`,
				)
			: html`<div class="out-empty">${lines.length ? "Nothing matches the filter" : "Waiting for Studio output…"}</div>`}
	</div>`;
}

export function OutputPanel({ place, state }) {
	const [lines, clear] = useOutput(place);
	const [filter, setFilter] = useState("all");
	const [context, setContext] = useState("all");
	const [query, setQuery] = useState("");

	const errors = lines.filter((l) => l.kind === "error").length;
	const warnings = lines.filter((l) => l.kind === "warn").length;
	const contexts = [...new Set(lines.map((l) => l.context))];

	const copyForAI = () => {
		const text = forAI(place, lines);
		if (!text) return toast("No errors or warnings to copy");

		navigator.clipboard.writeText(text).then(
			() => toast("Copied the errors with their files, paste them to your AI"),
			() => toast("Couldn't copy", "err"),
		);
	};

	return html`<section class="panel">
		<div class="panel-head">
			<div>
				<h2>Studio output</h2>
				<p>
					Prints, warnings and errors from Studio, in edit mode and playtests. Click a script reference to open the file at
					that line. Also saved to <span class="mono">.aragon/studio-output.log</span> so your AI agent can read it.
				</p>
			</div>
			<div class="toolbar" style="flex-wrap:nowrap">
				<span class=${`badge ${errors ? "err" : ""}`}>${errors} error${errors === 1 ? "" : "s"}</span>
				<span class=${`badge ${warnings ? "warn" : ""}`}>${warnings} warning${warnings === 1 ? "" : "s"}</span>
			</div>
		</div>
		<div class="toolbar out-tools">
			<${Seg}
				label="Show"
				value=${filter}
				onChange=${setFilter}
				options=${[
					{ value: "all", label: "All" },
					{ value: "problems", label: "Warnings" },
					{ value: "errors", label: "Errors" },
				]}
			/>
			${contexts.length > 1
				? html`<${Seg} label="Context" value=${context} onChange=${setContext} options=${[{ value: "all", label: "Everywhere" }, ...contexts.map((c) => ({ value: c, label: c }))]} />`
				: null}
			<input class="input" placeholder="Filter text" value=${query} onInput=${(e) => setQuery(e.target.value)} aria-label="Filter output" style="flex:1;min-width:140px" />
			<${Button} className="sm" icon="bolt" onClick=${copyForAI} title="Copy the latest errors with their file paths">Copy for AI<//>
			${place.workspace
				? html`<${Button} className="ghost sm" icon="folder" onClick=${() => post("/api/open", { target: `${place.workspace}\\.aragon` }).catch((e) => toast(e.message, "err"))}>Log folder<//>`
				: null}
			<${Button} className="ghost sm" icon="trash" onClick=${clear}>Clear<//>
		</div>
		<${OutputLines} place=${place} state=${state} lines=${lines} filter=${filter} context=${context} query=${query} />
	</section>`;
}

// Problems -------------------------------------------------------------------------

export function ProblemsPanel({ place, state }) {
	const [busy, setBusy] = useState(null);
	const problems = place.problems || [];

	const fix = async (problem, fixItem) => {
		setBusy(problem.id + fixItem.action);

		try {
			if (fixItem.action === "openFile") openFile(state, place.key, fixItem.arg, 1);
			else {
				await post(`/api/places/${place.key}/fix`, { action: fixItem.action, arg: fixItem.arg });

				toast(
					fixItem.action === "selectInStudio"
						? "Selected in Studio, check the Explorer"
						: fixItem.action === "reconnect"
							? "Reconnecting Studio…"
							: "Done",
				);
			}
		} catch (err) {
			toast(err.message, "err");
		} finally {
			setBusy(null);
		}
	};

	const dismiss = (id) => post(`/api/places/${place.key}/problems/dismiss`, { id }).catch((err) => toast(err.message, "err"));

	if (!problems.length) {
		return html`<section class="panel">
			<div class="empty">
				<img src="assets/celebrate.webp" alt="" />
				<h2>No sync problems</h2>
				<p>When something goes wrong between Studio and your files (a duplicate, a locked file, missing packages) it shows up here in plain words, with a fix. Your AI agent can read the same list in <span class="mono">.aragon/sync-problems.md</span>.</p>
			</div>
		</section>`;
	}

	return html`<div style="display:grid;gap:12px">
		<div class="toolbar" style="justify-content:space-between">
			<span class="faint" style="font-weight:800">${problems.length} problem${problems.length === 1 ? "" : "s"} · also in <span class="mono">.aragon/sync-problems.md</span> for your AI</span>
			<${Button} className="ghost sm" icon="check" onClick=${() => dismiss(null)}>Dismiss all<//>
		</div>
		${problems.map(
			(problem) => html`<article class=${`problem ${problem.severity}`} key=${problem.id}>
				<div class="problem-icon">${problem.severity === "error" ? "!" : "?"}</div>
				<div class="problem-body">
					<div class="toolbar" style="justify-content:space-between">
						<h3>${problem.title}</h3>
						<span class="faint num">${problem.count > 1 ? `${problem.count}× · ` : ""}${agoMs(problem.last)}</span>
					</div>
					<p>${problem.detail}</p>
					${problem.file ? html`<code class="problem-file">${problem.file}</code>` : null}
					<div class="toolbar">
						${problem.fixes.map(
							(item, index) => html`<${Button}
								className=${index === 0 ? "primary sm" : "sm"}
								busy=${busy === problem.id + item.action}
								disabled=${item.action === "selectInStudio" && place.syncedClients === 0}
								title=${item.action === "selectInStudio" && place.syncedClients === 0 ? "Open the place in Studio first" : undefined}
								onClick=${() => fix(problem, item)}
							>
								${item.label}
							<//>`,
						)}
						<${Button} className="ghost sm" onClick=${() => copy(`${problem.title}\n${problem.detail}\nLog: ${problem.raw}`)}>Copy<//>
						<${Button} className="ghost sm" onClick=${() => dismiss(problem.id)}>Dismiss<//>
					</div>
				</div>
			</article>`,
		)}
	</div>`;
}

// History (safety net) -------------------------------------------------------------------

function batchTitle(entries) {
	const deleted = entries.filter((e) => e.action === "delete").length;
	const overwritten = entries.filter((e) => e.action === "overwrite").length;
	const restored = entries.length - deleted - overwritten;
	const parts = [];

	if (deleted) parts.push(`deleted ${deleted} file${deleted === 1 ? "" : "s"}`);
	if (overwritten) parts.push(`overwrote ${overwritten} file${overwritten === 1 ? "" : "s"}`);
	if (restored) parts.push(`${restored} replaced by a restore`);

	return parts.join(", ");
}

function when(t) {
	const date = new Date(t);
	const today = new Date();
	const yesterday = new Date();
	yesterday.setDate(today.getDate() - 1);

	const time = date.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

	if (date.toDateString() === today.toDateString()) return `Today ${time}`;
	if (date.toDateString() === yesterday.toDateString()) return `Yesterday ${time}`;

	return `${date.toLocaleDateString(undefined, { month: "short", day: "numeric" })} ${time}`;
}

function DiffDialog({ place, entry, onClose, onRestore }) {
	const dialog = useRef(null);
	const host = useRef(null);
	const [data, setData] = useState(null);
	const [fallback, setFallback] = useState(false);

	useEffect(() => {
		dialog.current.showModal();
		api(`/api/places/${place.key}/history/${entry.id}`).then(setData, (err) => {
			toast(err.message, "err");
			onClose();
		});
	}, []);

	useEffect(() => {
		if (!data || data.binary || !host.current) return;

		let diff = null;

		loadMonaco().then(
			(monaco) => {
				if (!host.current) return;

				diff = monaco.editor.createDiffEditor(host.current, {
					theme: "aragon",
					readOnly: true,
					automaticLayout: true,
					renderSideBySide: true,
					fontFamily: "JetBrains Mono, Cascadia Code, Consolas, monospace",
					fontSize: 12.5,
					minimap: { enabled: false },
					scrollBeyondLastLine: false,
				});

				const language = languageOf(entry.path);
				diff.setModel({
					original: monaco.editor.createModel(data.content || "", language),
					modified: monaco.editor.createModel(data.current || "", language),
				});
			},
			() => setFallback(true),
		);

		return () => {
			if (diff) {
				const model = diff.getModel();
				diff.dispose();
				if (model) {
					model.original.dispose();
					model.modified.dispose();
				}
			}
		};
	}, [data]);

	return html`<dialog class="onboard diff-dialog" ref=${dialog} onCancel=${(e) => (e.preventDefault(), onClose())}>
		<div class="diff-head">
			<div>
				<small class="faint">${when(entry.t)} · ${entry.action === "delete" ? "deleted by Studio" : entry.action === "overwrite" ? "overwritten by Studio" : entry.action}</small>
				<h2 class="mono" style="font-size:15px">${entry.path}</h2>
			</div>
			<div class="toolbar">
				<span class="faint">Kept copy ← → ${data && !data.exists ? "file is gone now" : "file now"}</span>
				<${Button} className="primary" icon="refresh" onClick=${() => onRestore([entry])}>Restore this copy<//>
				<${Button} className="ghost icon" icon="x" aria-label="Close" onClick=${onClose} />
			</div>
		</div>
		<div class="diff-host" ref=${host}>
			${!data ? html`<div class="skeleton" style="height:100%"></div>` : null}
			${data && data.binary ? html`<div class="empty"><h2>Binary file</h2><p>${fmtBytes(entry.size)}. It can be restored, not compared.</p></div>` : null}
			${data && !data.binary && fallback
				? html`<div class="diff-fallback"><pre>${data.content}</pre><pre>${data.current || "(deleted)"}</pre></div>`
				: null}
		</div>
	</dialog>`;
}

export function HistoryPanel({ place }) {
	const [entries, setEntries] = useState(null);
	const [query, setQuery] = useState("");
	const [open, setOpen] = useState({});
	const [diff, setDiff] = useState(null);
	const [busy, setBusy] = useState(null);

	const load = async () => setEntries(await api(`/api/places/${place.key}/history`));
	usePoll(load, 5000, [place.key]);

	const batches = useMemo(() => {
		const q = query.trim().toLowerCase();
		const groups = new Map();

		for (const entry of entries || []) {
			if (q && !entry.path.toLowerCase().includes(q)) continue;
			if (!groups.has(entry.batch)) groups.set(entry.batch, []);
			groups.get(entry.batch).push(entry);
		}

		return [...groups.entries()].sort((a, b) => b[0] - a[0]);
	}, [entries, query]);

	const restore = async (list, label) => {
		const latest = new Map();

		// One copy per file: the newest in the selection
		for (const entry of list) if (!latest.has(entry.path)) latest.set(entry.path, entry);

		const count = latest.size;
		if (count > 1 && !confirm(`Put back ${count} files as they were? Whatever is there now is kept in this history too.`)) return;

		setBusy(label);

		try {
			const result = await post(`/api/places/${place.key}/history/restore`, { ids: [...latest.values()].map((e) => e.id) });
			toast(`Restored ${result.restored} file${result.restored === 1 ? "" : "s"}${place.syncedClients ? ", syncing to Studio" : ""}`);
			setDiff(null);
			load();
		} catch (err) {
			toast(err.message, "err");
		} finally {
			setBusy(null);
		}
	};

	return html`<section class="panel">
		<div class="panel-head">
			<div>
				<h2>Safety net</h2>
				<p>
					Every file Studio overwrites or deletes on disk is kept for 14 days, even when nothing is committed to git. Put back one
					file or a whole batch: it syncs straight into Studio.
				</p>
			</div>
			<input class="input" placeholder="Find a file" value=${query} onInput=${(e) => setQuery(e.target.value)} aria-label="Find a file" style="width:200px" />
		</div>
		${!entries
			? html`<div style="padding:0 16px 16px;display:grid;gap:6px">${[1, 2, 3].map(() => html`<div class="skeleton" style="height:44px"></div>`)}</div>`
			: !batches.length
				? html`<div class="empty">
						<img src="assets/bone.webp" alt="" />
						<h2>${query ? "No copies of that file" : "Nothing to restore yet"}</h2>
						<p>${query ? "Try another name." : "When Studio overwrites or deletes a file on disk, a copy lands here first. Nothing has been touched yet."}</p>
					</div>`
				: html`<div class="batches">
						${batches.map(([batch, list]) => {
							const expanded = open[batch] ?? batches.length === 1;

							return html`<div class=${`batch ${expanded ? "open" : ""}`} key=${batch}>
								<div class="batch-head">
									<button type="button" class="batch-toggle" onClick=${() => setOpen({ ...open, [batch]: !expanded })}>
										<${Icon} name="chevron" style=${expanded ? "" : "transform:rotate(-90deg)"} />
										<b>${when(batch)}</b>
										<span>Studio ${batchTitle(list)}</span>
									</button>
									<${Button} className="sm" icon="refresh" busy=${busy === batch} onClick=${() => restore(list, batch)}>
										${list.length > 1 ? `Restore all ${new Set(list.map((e) => e.path)).size}` : "Restore"}
									<//>
								</div>
								${expanded
									? html`<ul class="batch-files">
											${list.map(
												(entry) => html`<li key=${entry.id}>
													<span class=${`badge ${entry.action === "delete" ? "err" : entry.action === "overwrite" ? "warn" : ""}`}>
														${entry.action === "delete" ? "deleted" : entry.action === "overwrite" ? "overwritten" : "replaced"}
													</span>
													<span class="mono batch-path" title=${entry.path}>${entry.path}</span>
													<span class="faint num">${fmtBytes(entry.size)}</span>
													<${Button} className="ghost sm" onClick=${() => setDiff(entry)}>Compare<//>
													<${Button} className="ghost sm" busy=${busy === entry.id} onClick=${() => restore([entry], entry.id)}>Restore<//>
												</li>`,
											)}
										</ul>`
									: null}
							</div>`;
						})}
					</div>`}
		${diff ? html`<${DiffDialog} place=${place} entry=${diff} onClose=${() => setDiff(null)} onRestore=${(list) => restore(list, diff.id)} />` : null}
	</section>`;
}
