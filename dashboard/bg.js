// The world behind the dashboard: the illustrated shiba village, drawn once.
//
// Clouds and bees are baked into the picture, and nothing here moves on its
// own, so an idle dashboard paints zero frames and holds no extra GPU layers.
// Motion only happens in response to the user (hover, page changes, confetti).

export function startBackground() {
	const world = document.createElement("div");
	world.className = "bg-world";

	const veil = document.createElement("div");
	veil.className = "bg-veil";

	document.body.prepend(world, veil);
}
