// The right-click menu of a place (Places rows, top tiles, the live strip, the place page's "..." button)
import { api, post, go, toast, copy } from "../lib.js";

/// Every group a place can be moved to: the user's own groups, then the owners on the page
export function groupChoices(state) {
	const choices = state.groups.map((group) => ({ key: group.id, name: group.name, custom: true }));
	const seen = new Set();

	for (const place of state.places) {
		const key = place.ownerKey;
		if (!key || seen.has(key) || !state.owners[key]) continue;

		seen.add(key);
		choices.push({ key, name: state.owners[key].name, custom: false });
	}

	return choices;
}

/// The group a place shows in on the Places page
export function groupOf(state, place) {
	const natural = place.ownerKey || (place.placeId === 0 ? "local" : "unsorted");
	const group = place.group;

	if (!group) return natural;
	if (state.groups.some((g) => g.id === group) || state.owners[group] || group === "local" || group === "unsorted") return group;

	return natural;
}

export async function newGroup(name) {
	const label = name || prompt("Name of the new group", "");
	if (!label || !label.trim()) return null;

	try {
		const result = await post("/api/groups", { name: label.trim() });
		return result.id;
	} catch (err) {
		toast(err.message, "err");
		return null;
	}
}

export function movePlace(place, group) {
	return post(`/api/places/${place.key}/group`, { group }).catch((err) => toast(err.message, "err"));
}

export function removePlace(place, after) {
	const ok = confirm(
		`Remove ${place.name || "this place"} from Aragon?\n\nIt stops syncing and stays gone: Studio opening it or a Roblox refresh won't bring it back. Files on disk are not touched.\n\nAdd it back any time in Settings → Removed places.`,
	);

	if (!ok) return;

	api(`/api/places/${place.key}`, { method: "DELETE" }).then(
		() => {
			toast(`Removed ${place.name || "the place"}`);
			if (after) after();
		},
		(err) => toast(err.message, "err"),
	);
}

export function placeMenu(state, place) {
	const action = (name, label) => () =>
		post(`/api/places/${place.key}/action`, { action: name }).then(
			() => label && toast(label),
			(err) => toast(err.message, "err"),
		);

	const editor = state.editors.find((e) => e.id === state.prefs.externalEditor) || (state.prefs.externalEditor === "builtin" ? state.editors[0] : null);
	const current = groupOf(state, place);
	const name = place.name || "Untitled place";

	const groups = groupChoices(state).map((group) => ({
		label: group.name,
		icon: group.custom ? "folder" : "users",
		disabled: group.key === current,
		hint: group.key === current ? "here" : null,
		run: () => movePlace(place, group.key),
	}));

	return [
		{ label: "Open", icon: "chevron", run: () => go(`#/place/${place.key}`) },
		place.placeId ? { label: "Open in Studio", icon: "studio", run: action("openStudio", "Opening Roblox Studio…") } : null,
		place.project ? { label: "Open folder", icon: "folder", run: action("openFolder") } : null,
		place.project && editor
			? {
					label: `Open in ${editor.name}`,
					icon: "external",
					run: () => post(`/api/places/${place.key}/open`, { editor: editor.id }).catch((err) => toast(err.message, "err")),
				}
			: null,
		place.project ? { label: "Code", icon: "code", run: () => go(`#/editor/${place.key}`) } : null,
		"-",
		{
			label: "Move to group",
			icon: "users",
			items: [
				...groups,
				groups.length ? "-" : null,
				{
					label: "New group…",
					icon: "plus",
					run: async () => {
						const id = await newGroup();
						if (id) movePlace(place, id);
					},
				},
				place.group ? { label: "Back under its owner", icon: "back", run: () => movePlace(place, null) } : null,
			],
		},
		"-",
		place.placeId ? { label: "Copy place ID", icon: "link", run: () => copy(place.placeId) } : null,
		place.universeId ? { label: "Copy universe ID", icon: "link", run: () => copy(place.universeId) } : null,
		place.project ? { label: "Copy folder path", icon: "link", run: () => copy(place.workspace) } : null,
		"-",
		place.project
			? {
					label: place.syncEnabled ? "Pause sync" : "Resume sync",
					icon: place.syncEnabled ? "pause" : "play",
					run: () => post(`/api/places/${place.key}/options`, { syncEnabled: !place.syncEnabled }).catch((err) => toast(err.message, "err")),
				}
			: null,
		{ label: place.project ? "Change folder…" : "Set up a folder…", icon: "folder", run: () => go(`#/setup/${place.key}`) },
		place.project
			? {
					label: "Unmap folder",
					icon: "x",
					run: () =>
						confirm(`Unmap ${name}? The folder and its files stay on disk; the place needs a new setup.`) &&
						post(`/api/places/${place.key}/unmap`).then(() => toast("Unmapped"), (err) => toast(err.message, "err")),
				}
			: null,
		{ label: "Remove from dashboard", icon: "trash", danger: true, run: () => removePlace(place) },
	];
}
