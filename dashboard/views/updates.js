// In-app updates: the corner card (new release → one click → progress → restart) and "What's new" after it
import { html, useState, useEffect, useRef } from "../vendor/preact-htm.js";
import { api, post, toast, fmtBytes, usePoll } from "../lib.js";
import { Icon, Button } from "../ui.js";
import { burst } from "../motion.js";

// Markdown (the subset release notes use) ------------------------------------------

function inline(text) {
	const out = [];
	const pattern = /\*\*(.+?)\*\*|`([^`]+)`|\[([^\]]+)\]\((https?:[^)]+)\)|\*([^*\s][^*]*?)\*/g;
	let last = 0;
	let match;

	while ((match = pattern.exec(text))) {
		if (match.index > last) out.push(text.slice(last, match.index));

		if (match[1]) out.push(html`<b>${match[1]}</b>`);
		else if (match[2]) out.push(html`<code>${match[2]}</code>`);
		else if (match[5]) out.push(html`<i>${match[5]}</i>`);
		else out.push(html`<a href=${match[4]} target="_blank" rel="noreferrer">${match[3]}</a>`);

		last = pattern.lastIndex;
	}

	if (last < text.length) out.push(text.slice(last));
	return out;
}

export function Markdown({ text }) {
	const blocks = [];
	let list = null;

	const lines = (text || "").replace(/\r/g, "").split("\n");

	for (let i = 0; i < lines.length; i++) {
		const line = lines[i];
		const trimmed = line.trim();

		if (!trimmed) {
			list = null;
			continue;
		}

		const heading = trimmed.match(/^(#{2,4})\s+(.*)$/);
		const bullet = line.match(/^(\s*)[-*]\s+(.*)$/);

		if (heading) {
			list = null;
			blocks.push(html`<h4>${inline(heading[2])}</h4>`);
		} else if (bullet) {
			const nested = bullet[1].length >= 2;

			if (nested && list && list.items.length) {
				const parent = list.items[list.items.length - 1];
				parent.children.push(bullet[2]);
			} else {
				if (!list) {
					list = { items: [] };
					blocks.push(list);
				}

				list.items.push({ text: bullet[2], children: [] });
			}
		} else if (list && list.items.length && /^\s{2,}/.test(line)) {
			// Wrapped bullet text
			const item = list.items[list.items.length - 1];
			if (item.children.length) item.children[item.children.length - 1] += " " + trimmed;
			else item.text += " " + trimmed;
		} else {
			list = null;
			blocks.push(html`<p>${inline(trimmed)}</p>`);
		}
	}

	return html`<div class="md">
		${blocks.map((block) =>
			block.items
				? html`<ul>
						${block.items.map(
							(item) => html`<li>
								${inline(item.text)}
								${item.children.length ? html`<ul>${item.children.map((c) => html`<li>${inline(c)}</li>`)}</ul>` : null}
							</li>`,
						)}
					</ul>`
				: block,
		)}
	</div>`;
}

/// The bold lead-ins of the notes' bullets: the release's headlines
function highlights(notes, max = 3) {
	const found = [];

	for (const line of (notes || "").split("\n")) {
		const match = line.match(/^[-*]\s+\*\*(.+?)\*\*/);
		if (match) found.push(match[1].replace(/[.:]$/, ""));
		if (found.length >= max) break;
	}

	return found;
}

// Corner card ----------------------------------------------------------------------

const WORKING = ["downloading", "verifying", "installing", "restarting"];

export function UpdateCard({ state }) {
	const summary = state.update;
	const [live, setLive] = useState(null);
	const [expanded, setExpanded] = useState(false);
	const [hidden, setHidden] = useState(false);
	const status = (live && live.status) || summary.status;
	const latest = summary.latest || (live && live.latest);
	const working = WORKING.includes(status);
	const failedInstall = status === "error" && live && live.downloaded > 0;

	// Fast progress while an update runs (the state long-poll only wakes on status changes)
	usePoll(
		async () => {
			if (!working && !failedInstall) return;
			setLive(await api("/api/update"));
		},
		working ? 200 : 2000,
		[working, failedInstall],
	);

	useEffect(() => {
		if (!working && live && live.status !== "error") setLive(null);
	}, [summary.status]);

	const available = status === "available" && latest && latest.version !== summary.skipped;

	if (hidden || !latest || (!available && !working && !failedInstall)) return null;

	const install = async () => {
		try {
			await post("/api/update/install");
			setLive({ ...summary, status: "downloading", downloaded: 0, total: latest.size, speed: 0 });
		} catch (err) {
			toast(err.message, "err");
		}
	};

	const later = () => {
		setHidden(true);
		post("/api/update/skip", { version: latest.version }).catch(() => {});
		toast(`Skipped ${latest.version}. Settings → Updates installs it any time`);
	};

	const progress = live && live.total ? Math.min(1, live.downloaded / live.total) : 0;
	const left = live && live.speed > 0 ? Math.max(0, (live.total - live.downloaded) / live.speed) : null;
	const points = highlights(latest.notes);

	let stage = null;

	if (status === "downloading") {
		stage = html`<b>${Math.round(progress * 100)}%</b> · ${fmtBytes(live && live.downloaded)} of ${fmtBytes(live && live.total)}
			${live && live.speed ? html` · ${fmtBytes(live.speed)}/s` : null}${left != null && left > 1 ? html` · ~${Math.ceil(left)}s left` : null}`;
	} else if (status === "verifying") {
		stage = html`Checking it's the real thing (SHA-256)…`;
	} else if (status === "installing") {
		stage = html`Installing…`;
	} else if (status === "restarting") {
		stage = html`Restarting Aragon. Studio reconnects by itself.`;
	}

	return html`<aside class=${`update-card ${working ? "working" : ""} ${status}`} role="status" aria-live="polite">
		<div class="update-top">
			<img src=${working ? "assets/laptop.webp" : "assets/celebrate.webp"} alt="" />
			<div class="update-title">
				<small>${working ? "Updating" : "New version"}</small>
				<b>Aragon ${latest.version}${working ? "" : " is here"}</b>
				${!working ? html`<span class="faint">You have ${summary.current} · ${fmtBytes(latest.size)}</span>` : null}
			</div>
			${!working ? html`<button type="button" class="update-x" aria-label="Later" title="Later" onClick=${later}><${Icon} name="x" /></button>` : null}
		</div>

		${working || failedInstall
			? html`<div class="update-progress">
					<div class=${`bar ${status === "downloading" ? "" : "full"}`} role="progressbar" aria-valuenow=${Math.round(progress * 100)} aria-valuemin="0" aria-valuemax="100">
						<span style=${`transform:scaleX(${status === "downloading" ? progress : 1})`}></span>
						<i class="paw" style=${`left:calc(${(status === "downloading" ? progress : 1) * 100}% - 11px)`}>🐾</i>
					</div>
					<p>${failedInstall ? html`<span class="error-text">${live.error}</span>` : stage}</p>
					${failedInstall
						? html`<div class="toolbar"><${Button} className="primary sm" icon="refresh" onClick=${install}>Try again<//>
								<${Button} className="ghost sm" onClick=${() => post("/api/open", { target: latest.url }).catch(() => window.open(latest.url))}>Release page<//></div>`
						: null}
				</div>`
			: html`
					${points.length && !expanded
						? html`<ul class="update-points">${points.map((p) => html`<li>${p}</li>`)}</ul>`
						: null}
					${expanded ? html`<div class="update-notes"><${Markdown} text=${latest.notes || "No notes for this release."} /></div>` : null}
					<div class="update-actions">
						<${Button} className="primary" icon="download" onClick=${install}>Update now<//>
						<${Button} className="ghost sm" onClick=${() => setExpanded(!expanded)}>${expanded ? "Less" : "What's new"}<//>
					</div>
				`}
	</aside>`;
}

// What's new (first start of a new version) ------------------------------------------

export function WhatsNew({ state }) {
	const dialog = useRef(null);
	const info = state.whatsNew;

	useEffect(() => {
		if (!dialog.current || dialog.current.open) return;

		dialog.current.showModal();
		setTimeout(() => burst(innerWidth / 2, innerHeight * 0.3, 80), 350);
	}, []);

	const close = () => {
		post("/api/prefs", { lastVersion: info.version }).catch(() => {});
		dialog.current && dialog.current.close();
	};

	return html`<dialog class="onboard whats-new" ref=${dialog} onCancel=${(e) => (e.preventDefault(), close())}>
		<div class="whats-new-head">
			<img src="assets/celebrate.webp" alt="" />
			<div>
				<small>Updated</small>
				<h2>What's new in Aragon ${info.version}</h2>
			</div>
		</div>
		<div class="whats-new-body"><${Markdown} text=${info.notes} /></div>
		<div class="onboard-foot">
			<span class="faint">Your Studio plugin was updated too.</span>
			<${Button} className="primary" icon="check" onClick=${close}>Much wow, let's go<//>
		</div>
	</dialog>`;
}
