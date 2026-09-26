// Motion helpers. Everything animates transform/opacity (compositor-only) or
// draws on a canvas, so it stays smooth on the GPU.
import { html, useEffect, useRef, useState } from "./vendor/preact-htm.js";

const reduce = () => matchMedia("(prefers-reduced-motion: reduce)").matches;
const easeOutExpo = (t) => (t >= 1 ? 1 : 1 - 2 ** (-10 * t));

/// Number that counts up to its value when it appears or changes
export function CountUp({ value, format = String, duration = 900 }) {
	const [shown, setShown] = useState(reduce() ? value : 0);
	const from = useRef(reduce() ? value : 0);

	useEffect(() => {
		if (typeof value !== "number" || reduce()) {
			setShown(value);
			return;
		}

		const start = performance.now();
		const origin = from.current;
		let frame;

		const tick = (now) => {
			const t = Math.min(1, (now - start) / duration);
			const current = origin + (value - origin) * easeOutExpo(t);
			setShown(t >= 1 ? value : Math.round(current));

			if (t < 1) frame = requestAnimationFrame(tick);
			else from.current = value;
		};

		frame = requestAnimationFrame(tick);
		return () => cancelAnimationFrame(frame);
	}, [value]);

	return html`<span class="num">${typeof shown === "number" ? format(shown) : format(value)}</span>`;
}

/// Pointer-driven 3D tilt: returns props to spread on the element
export function useTilt(max = 6) {
	const ref = useRef(null);

	const onPointerMove = (e) => {
		const element = ref.current;
		if (!element || reduce()) return;

		const rect = element.getBoundingClientRect();
		const x = (e.clientX - rect.left) / rect.width - 0.5;
		const y = (e.clientY - rect.top) / rect.height - 0.5;

		element.style.setProperty("--rx", `${(-y * max).toFixed(2)}deg`);
		element.style.setProperty("--ry", `${(x * max).toFixed(2)}deg`);
		element.style.setProperty("--mx", `${((x + 0.5) * 100).toFixed(1)}%`);
		element.style.setProperty("--my", `${((y + 0.5) * 100).toFixed(1)}%`);
	};

	const onPointerLeave = () => {
		const element = ref.current;
		if (!element) return;

		element.style.setProperty("--rx", "0deg");
		element.style.setProperty("--ry", "0deg");
	};

	return { ref, onPointerMove, onPointerLeave };
}

// Confetti -------------------------------------------------------------------

const sprites = {};

function sprite(name) {
	if (!sprites[name]) {
		const image = new Image();
		image.src = `assets/${name}.webp`;
		sprites[name] = image;
	}

	return sprites[name];
}

function pawPath(ctx, size) {
	// Main pad + four toes
	ctx.beginPath();
	ctx.ellipse(0, size * 0.18, size * 0.32, size * 0.26, 0, 0, Math.PI * 2);

	for (const [x, y, r] of [
		[-0.34, -0.14, 0.13],
		[-0.12, -0.32, 0.14],
		[0.12, -0.32, 0.14],
		[0.34, -0.14, 0.13],
	]) {
		ctx.moveTo(x * size + r * size, y * size);
		ctx.ellipse(x * size, y * size, r * size, r * size * 1.2, 0, 0, Math.PI * 2);
	}

	ctx.fill();
}

/// Bursts coins, bones and paw prints from a point on screen
export function burst(x = innerWidth / 2, y = innerHeight / 2, count = 70) {
	if (reduce()) return;

	const canvas = document.createElement("canvas");
	canvas.className = "confetti";
	canvas.width = innerWidth * devicePixelRatio;
	canvas.height = innerHeight * devicePixelRatio;
	document.body.appendChild(canvas);

	// A popover lives in the top layer, above any open modal dialog
	if (canvas.showPopover) {
		canvas.popover = "manual";
		canvas.showPopover();
	}

	const ctx = canvas.getContext("2d");
	ctx.scale(devicePixelRatio, devicePixelRatio);

	const images = [sprite("coin"), sprite("bone")];
	const particles = Array.from({ length: count }, () => {
		const angle = -Math.PI / 2 + (Math.random() - 0.5) * Math.PI * 1.1;
		const speed = 7 + Math.random() * 10;

		return {
			x,
			y,
			vx: Math.cos(angle) * speed,
			vy: Math.sin(angle) * speed,
			spin: (Math.random() - 0.5) * 0.3,
			angle: Math.random() * Math.PI,
			size: 16 + Math.random() * 18,
			kind: Math.random() < 0.45 ? "paw" : Math.random() < 0.7 ? 0 : 1,
			life: 1,
		};
	});

	const start = performance.now();

	const tick = (now) => {
		const elapsed = now - start;
		ctx.clearRect(0, 0, innerWidth, innerHeight);

		for (const p of particles) {
			p.vy += 0.32;
			p.vx *= 0.985;
			p.x += p.vx;
			p.y += p.vy;
			p.angle += p.spin;
			p.life = Math.max(0, 1 - elapsed / 2200);

			ctx.save();
			ctx.globalAlpha = p.life;
			ctx.translate(p.x, p.y);
			ctx.rotate(p.angle);

			if (p.kind === "paw") {
				ctx.fillStyle = "#FFB81C";
				pawPath(ctx, p.size * 0.8);
			} else {
				const image = images[p.kind];
				if (image.complete) ctx.drawImage(image, -p.size / 2, -p.size / 2, p.size, p.size);
			}

			ctx.restore();
		}

		if (elapsed < 2200) requestAnimationFrame(tick);
		else canvas.remove();
	};

	requestAnimationFrame(tick);
}

// Preload confetti sprites so the first burst isn't empty
sprite("coin");
sprite("bone");
