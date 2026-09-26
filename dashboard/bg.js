// The living world behind the dashboard: the illustrated shiba village,
// clouds drifting at different depths and bees buzzing around.
//
// Kept deliberately cheap: everything is CSS transform animation handled by
// the compositor, parallax only updates when the pointer moves (CSS eases it),
// so an idle dashboard does almost no work and holds very little GPU memory.

const CLOUDS = [
	{ src: "cloud-a", top: 4, width: 260, duration: 140, delay: -30, opacity: 0.95 },
	{ src: "cloud-b", top: 14, width: 150, duration: 190, delay: -120, opacity: 0.8 },
	{ src: "cloud-a", top: 22, width: 120, duration: 230, delay: -60, opacity: 0.7 },
	{ src: "cloud-b", top: 8, width: 210, duration: 160, delay: -95, opacity: 0.9 },
	{ src: "cloud-a", top: 30, width: 180, duration: 175, delay: -150, opacity: 0.85 },
];

const BEES = [
	{ bottom: 18, delay: 0, duration: 26 },
	{ bottom: 32, delay: -13, duration: 34 },
];

function el(tag, className, parent) {
	const node = document.createElement(tag);
	node.className = className;
	if (parent) parent.appendChild(node);
	return node;
}

export function startBackground() {
	const world = el("div", "bg-world");
	const sky = el("div", "bg-layer");
	const veil = el("div", "bg-veil");

	for (const cloud of CLOUDS) {
		const img = el("img", "cloud", sky);
		img.src = `assets/${cloud.src}.webp`;
		img.alt = "";
		img.decoding = "async";
		img.style.cssText = `top:${cloud.top}vh;width:${cloud.width}px;opacity:${cloud.opacity};animation-duration:${cloud.duration}s;animation-delay:${cloud.delay}s`;
	}

	for (const bee of BEES) {
		const wrap = el("div", "bee", sky);
		wrap.style.cssText = `bottom:${bee.bottom}vh;left:0;animation-duration:${bee.duration}s;animation-delay:${bee.delay}s`;

		const img = el("img", "", wrap);
		img.src = "assets/bee.webp";
		img.alt = "";
	}

	document.body.prepend(world, sky, veil);

	// Parallax: set the target on pointer move, the CSS transition does the easing
	let pending = false;
	let x = 0.5;
	let y = 0.5;

	addEventListener(
		"pointermove",
		(e) => {
			x = e.clientX / innerWidth;
			y = e.clientY / innerHeight;

			if (pending) return;
			pending = true;

			requestAnimationFrame(() => {
				pending = false;
				world.style.transform = `translate3d(${(0.5 - x) * 18}px, ${(0.5 - y) * 10}px, 0) scale(1.05)`;
				sky.style.transform = `translate3d(${(0.5 - x) * 34}px, ${(0.5 - y) * 14}px, 0)`;
			});
		},
		{ passive: true },
	);

	// Freeze ambient animation while the window is hidden or minimized
	const sync = () => document.documentElement.classList.toggle("still", document.hidden);
	document.addEventListener("visibilitychange", sync);
	sync();
}
