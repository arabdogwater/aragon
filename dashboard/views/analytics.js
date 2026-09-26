import { html, useState, useMemo } from "../vendor/preact-htm.js";
import { go, fmt, fmtMinutes, series, sum, dayKey } from "../lib.js";
import { BarChart, Seg, Thumb, Sparkline } from "../ui.js";
import { CountUp } from "../motion.js";

export function Analytics({ state }) {
	const [range, setRange] = useState(30);
	const [metric, setMetric] = useState("added");

	const mapped = state.places.filter((p) => p.project || p.stats.minutes > 0);

	const totals = useMemo(() => {
		const byDay = new Map();

		for (const place of mapped) {
			for (const point of series(place.stats.daily, range, metric)) {
				byDay.set(point.day, (byDay.get(point.day) || 0) + point.value);
			}
		}

		const points = [];

		for (let i = range - 1; i >= 0; i--) {
			const date = new Date();
			date.setDate(date.getDate() - i);
			points.push({ date, day: dayKey(date), value: byDay.get(dayKey(date)) || 0 });
		}

		return points;
	}, [mapped, range, metric]);

	const board = mapped
		.map((place) => {
			const values = series(place.stats.daily, range, metric).map((p) => p.value);
			return { place, values, total: sum(values) };
		})
		.sort((a, b) => b.total - a.total || b.place.stats.loc - a.place.stats.loc);

	const best = Math.max(...board.map((b) => b.total), 1);
	const totalLoc = sum(mapped.map((p) => p.stats.loc));
	const periodTotal = sum(totals.map((p) => p.value));
	const activeDays = totals.filter((p) => p.value > 0).length;
	const unit = metric === "added" ? "lines" : "minutes";
	const format = metric === "added" ? fmt : fmtMinutes;

	return html`<div class="page">
		<header class="page-head">
			<div class="toolbar" style="flex-wrap:nowrap;gap:14px">
				<img class="head-art mascot" src="assets/chart.webp" alt="" />
				<div>
					<h1>Analytics</h1>
					<p>What you actually built, across every mapped place.</p>
				</div>
			</div>
			<div class="toolbar">
				<${Seg}
					label="Metric"
					value=${metric}
					onChange=${setMetric}
					options=${[
						{ value: "added", label: "Lines written" },
						{ value: "minutes", label: "Time synced" },
					]}
				/>
				<${Seg}
					label="Range"
					value=${range}
					onChange=${setRange}
					options=${[
						{ value: 7, label: "7d" },
						{ value: 30, label: "30d" },
						{ value: 90, label: "90d" },
					]}
				/>
			</div>
		</header>

		<div class="stats-row num stagger">
			<div class="stat-cell"><span>${metric === "added" ? "Lines written" : "Time synced"} · ${range}d</span><b><${CountUp} value=${periodTotal} format=${format} /></b><small>${activeDays} active days</small></div>
			<div class="stat-cell"><span>Lines of code</span><b><${CountUp} value=${totalLoc} format=${fmt} /></b><small>across ${mapped.length} places</small></div>
			<div class="stat-cell"><span>All-time written</span><b>${fmt(sum(mapped.map((p) => p.stats.linesAdded)))}</b><small>${fmt(sum(mapped.map((p) => p.stats.linesRemoved)))} removed</small></div>
			<div class="stat-cell"><span>All-time synced</span><b>${fmtMinutes(sum(mapped.map((p) => p.stats.minutes)))}</b><small>${fmt(sum(mapped.map((p) => p.stats.syncs)))} syncs</small></div>
		</div>

		<section class="panel">
			<div class="panel-head">
				<div>
					<h2>${metric === "added" ? "Lines written per day" : "Minutes synced per day"}</h2>
					<p>All places combined</p>
				</div>
			</div>
			<${BarChart} points=${totals} unit=${unit} height=${200} />
		</section>

		<section class="panel">
			<div class="panel-head">
				<h2>By place</h2>
				<span class="faint">${range} days</span>
			</div>
			${board.length
				? html`<table class="board">
						<thead>
							<tr>
								<th>Place</th>
								<th>${metric === "added" ? "Lines written" : "Time synced"}</th>
								<th class="r">Total</th>
								<th>Trend</th>
								<th class="r">LOC</th>
							</tr>
						</thead>
						<tbody class="stagger">
							${board.map(
								({ place, values, total }, index) => html`<tr key=${place.key} style=${`--i:${index}`} onClick=${() => go(`#/place/${place.key}`)}>
									<td>
										<div class="toolbar" style="flex-wrap:nowrap">
											<${Thumb} src=${place.iconUrl} className="board-thumb" />
											<div>
												<div>${place.name}</div>
												<div class="faint" style="font-size:12px">${place.ownerName || ""}</div>
											</div>
										</div>
									</td>
									<td style="width:32%">
										<div class="meter" role="img" aria-label=${`${format(total)} ${unit}`}>
											<span style=${`width:${(total / best) * 100}%`}></span>
										</div>
									</td>
									<td class="r num">${format(total)}</td>
									<td><${Sparkline} values=${values} width=${110} /></td>
									<td class="r num">${fmt(place.stats.loc)}</td>
								</tr>`,
							)}
						</tbody>
					</table>`
				: html`<div class="empty">
						<img src="assets/chart.webp" alt="" />
						<h2>No data yet</h2>
						<p>Map a place to a folder and Aragon starts counting the code you write.</p>
					</div>`}
		</section>
	</div>`;
}
