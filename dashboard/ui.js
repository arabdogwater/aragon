// UI primitives: icons, thumbnails, switches, charts
import { html, useState, useRef } from "./vendor/preact-htm.js";
import { copy, fmtFull } from "./lib.js";

const PATHS = {
	home: "M3 10.5 12 3l9 7.5V20a1 1 0 0 1-1 1h-5v-6H9v6H4a1 1 0 0 1-1-1z",
	chart: "M4 20V10M10 20V4M16 20v-7M22 20H2",
	code: "m8 7-5 5 5 5M16 7l5 5-5 5M13.5 4l-3 16",
	gear: "M12 15.5a3.5 3.5 0 1 0 0-7 3.5 3.5 0 0 0 0 7zM19.4 13a7.9 7.9 0 0 0 0-2l2-1.6-2-3.4-2.4 1a7.7 7.7 0 0 0-1.7-1L15 3.5h-4l-.3 2.5a7.7 7.7 0 0 0-1.7 1l-2.4-1-2 3.4 2 1.6a7.9 7.9 0 0 0 0 2l-2 1.6 2 3.4 2.4-1c.5.4 1.1.8 1.7 1l.3 2.5h4l.3-2.5c.6-.2 1.2-.6 1.7-1l2.4 1 2-3.4z",
	search: "M11 18a7 7 0 1 0 0-14 7 7 0 0 0 0 14zm10 3-5-5",
	plus: "M12 5v14M5 12h14",
	refresh: "M20 11a8 8 0 0 0-14.9-3.9L3 9m0-5v5h5M4 13a8 8 0 0 0 14.9 3.9L21 15m0 5v-5h-5",
	folder: "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
	file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
	play: "M7 5v14l11-7z",
	studio: "M4 4h16v12H4zM8 20h8M12 16v4",
	external: "M14 4h6v6M20 4l-9 9M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5",
	chevron: "m6 9 6 6 6-6",
	back: "M15 18l-6-6 6-6",
	box: "M21 8 12 3 3 8v8l9 5 9-5zM3 8l9 5 9-5M12 13v8",
	pause: "M8 5v14M16 5v14",
	link: "M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1",
	download: "M12 4v11m0 0-4-4m4 4 4-4M4 20h16",
	key: "M15 7a4 4 0 1 1-3.9 5H8v2H6v2H3v-3l6.1-6.1A4 4 0 0 1 15 7zm1 1.5h.01",
	trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
	save: "M5 3h11l5 5v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM7 3v5h8M7 21v-7h10v7",
	x: "M6 6l12 12M18 6 6 18",
	check: "m5 12 5 5 9-10",
	import: "M12 3v12m0 0 4-4m-4 4-4-4M5 21h14",
	terminal: "m5 7 5 5-5 5M12 19h7",
	user: "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zm-8 9a8 8 0 0 1 16 0",
	users: "M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21a7 7 0 0 1 14 0M16 3.5a4 4 0 0 1 0 7.5M22 21a7 7 0 0 0-4-6.3",
	bolt: "M13 2 4 14h7l-1 8 9-12h-7z",
};

export function Icon({ name, size = 16, ...rest }) {
	return html`<svg
		width=${size}
		height=${size}
		viewBox="0 0 24 24"
		fill="none"
		stroke="currentColor"
		stroke-width="1.8"
		stroke-linecap="round"
		stroke-linejoin="round"
		aria-hidden="true"
		...${rest}
	>
		<path d=${PATHS[name] || ""} />
	</svg>`;
}

export function Thumb({ src, alt = "", className = "" }) {
	const [failed, setFailed] = useState(false);

	if (!src || failed) {
		return html`<div class=${`thumb-fallback ${className}`} aria-hidden="true"></div>`;
	}

	return html`<img class=${className} src=${src} alt=${alt} loading="lazy" onError=${() => setFailed(true)} />`;
}

export function Switch({ checked, onChange, label, disabled }) {
	return html`<button
		type="button"
		class="switch"
		role="switch"
		aria-checked=${checked ? "true" : "false"}
		aria-label=${label}
		disabled=${disabled}
		onClick=${() => onChange(!checked)}
	></button>`;
}

export function Seg({ value, options, onChange, label }) {
	return html`<div class="seg" role="group" aria-label=${label}>
		${options.map(
			(option) => html`<button
				type="button"
				aria-pressed=${value === option.value ? "true" : "false"}
				onClick=${() => onChange(option.value)}
			>
				${option.label}
			</button>`,
		)}
	</div>`;
}

export function Copyable({ value, children }) {
	return html`<button type="button" class="copy mono" title="Copy" onClick=${(e) => (e.stopPropagation(), copy(value))}>
		${children || value}
	</button>`;
}

export function Button({ busy, icon, children, className = "", ...rest }) {
	return html`<button type="button" class=${`btn ${className}`} disabled=${busy || rest.disabled} ...${rest}>
		${busy ? html`<span class="spin"></span>` : icon ? html`<${Icon} name=${icon} />` : null} ${children}
	</button>`;
}

// Charts ------------------------------------------------------------------------

/// Tiny single-series trend line (no axes; the row label names it)
export function Sparkline({ values, width = 100, height = 28 }) {
	if (!values.length || values.every((v) => v === 0)) {
		return html`<svg class="spark" width=${width} height=${height} aria-hidden="true">
			<line x1="0" x2=${width} y1=${height - 1} y2=${height - 1} stroke="var(--line)" stroke-width="1" />
		</svg>`;
	}

	const max = Math.max(...values, 1);
	const step = width / Math.max(values.length - 1, 1);
	const points = values.map((v, i) => [i * step, height - 2 - (v / max) * (height - 4)]);
	const line = points.map(([x, y], i) => `${i ? "L" : "M"}${x.toFixed(1)},${y.toFixed(1)}`).join("");
	const area = `${line}L${width},${height}L0,${height}Z`;

	return html`<svg class="spark" width=${width} height=${height} aria-hidden="true">
		<path class="area" d=${area} />
		<path d=${line} />
	</svg>`;
}

/// Daily bar chart with hover tooltip. `points`: [{ date, value, entry }]
export function BarChart({ points, height = 170, unit = "lines", describe }) {
	const [hover, setHover] = useState(null);
	const ref = useRef(null);

	const width = 720;
	const pad = { top: 10, right: 8, bottom: 22, left: 36 };
	const innerW = width - pad.left - pad.right;
	const innerH = height - pad.top - pad.bottom;
	const max = Math.max(...points.map((p) => p.value), 1);
	const niceMax = niceCeil(max);
	const slot = innerW / points.length;
	const barW = Math.max(2, Math.min(18, slot - 2));
	const ticks = [0, niceMax / 2, niceMax];

	const y = (v) => pad.top + innerH - (v / niceMax) * innerH;

	// Rounded top only, anchored flat on the baseline
	const barPath = (x, top, w, bottom) => {
		const r = Math.min(4, w / 2, bottom - top);
		return `M${x},${bottom}V${top + r}Q${x},${top} ${x + r},${top}H${x + w - r}Q${x + w},${top} ${x + w},${top + r}V${bottom}Z`;
	};

	const labelEvery = Math.ceil(points.length / 6);
	const hovered = hover != null ? points[hover] : null;

	return html`<div class="chart" ref=${ref}>
		<svg viewBox=${`0 0 ${width} ${height}`} role="img" aria-label=${`${unit} per day`} onMouseLeave=${() => setHover(null)}>
			<g class="grid">
				${ticks.map((t) => html`<line x1=${pad.left} x2=${width - pad.right} y1=${y(t)} y2=${y(t)} />`)}
			</g>
			<g class="axis">
				${ticks.map(
					(t) => html`<text x=${pad.left - 8} y=${y(t) + 4} text-anchor="end">${fmtShort(t)}</text>`,
				)}
				${points.map((p, i) =>
					i % labelEvery === 0
						? html`<text x=${pad.left + i * slot + slot / 2} y=${height - 6} text-anchor="middle">
								${p.date.toLocaleDateString(undefined, { month: "short", day: "numeric" })}
							</text>`
						: null,
				)}
			</g>
			${points.map((p, i) => {
				const x = pad.left + i * slot + (slot - barW) / 2;
				const top = y(p.value);
				const bottom = pad.top + innerH;

				return html`<g>
					${p.value > 0
						? html`<path class=${`bar ${hover === i ? "hover" : ""}`} style=${`--i:${i}`} d=${barPath(x, Math.min(top, bottom - 2), barW, bottom)} />`
						: null}
					<rect class="hit" x=${pad.left + i * slot} y=${pad.top} width=${slot} height=${innerH} onMouseEnter=${() => setHover(i)} />
				</g>`;
			})}
		</svg>
		${hovered
			? html`<div
					class="tooltip"
					style=${`left:${((pad.left + hover * slot + slot / 2) / width) * 100}%;top:${(y(hovered.value) / height) * 100}%`}
				>
					<b class="num">${fmtFull(hovered.value)} ${unit}</b>
					<span class="muted">${hovered.date.toLocaleDateString(undefined, { weekday: "short", month: "short", day: "numeric" })}</span>
					${describe ? html`<div class="muted">${describe(hovered)}</div>` : null}
				</div>`
			: null}
	</div>`;
}

function niceCeil(value) {
	if (value <= 4) return 4;

	const magnitude = 10 ** Math.floor(Math.log10(value));
	const normalized = value / magnitude;
	const nice = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;

	return nice * magnitude;
}

function fmtShort(value) {
	return value >= 1000 ? `${(value / 1000).toFixed(value % 1000 ? 1 : 0)}k` : String(Math.round(value));
}

/// Languages: one stacked bar + legend with direct values (identity never color-alone)
export function Languages({ languages }) {
	const entries = Object.entries(languages || {}).sort((a, b) => b[1] - a[1]);
	const total = entries.reduce((s, [, v]) => s + v, 0);

	if (!total) {
		return html`<p class="muted" style="padding:0 16px 16px">No scripts found yet.</p>`;
	}

	const color = (i) => `var(--cat-${Math.min(i + 1, 3)})`;

	return html`<div class="langs">
		<div class="lang-bar" role="img" aria-label="Lines of code per language">
			${entries.map(([, v], i) => html`<span style=${`width:${(v / total) * 100}%;background:${color(i)}`}></span>`)}
		</div>
		<div class="lang-list">
			${entries.map(
				([name, v], i) => html`<div>
					<i style=${`background:${color(i)}`}></i>${name}
					<span class="num">${fmtFull(v)} · ${Math.round((v / total) * 100)}%</span>
				</div>`,
			)}
		</div>
	</div>`;
}
