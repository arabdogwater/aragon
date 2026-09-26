import { html, useState, useEffect, useRef, useCallback } from "../vendor/preact-htm.js";
import { api, go, useAction, toast } from "../lib.js";
import { Icon, Button, Thumb } from "../ui.js";

// Monaco comes from the CDN; when offline the editor falls back to a plain textarea
const MONACO = "https://cdn.jsdelivr.net/npm/monaco-editor@0.52.2/min";
let monacoPromise = null;

function loadMonaco() {
	if (monacoPromise) return monacoPromise;

	monacoPromise = new Promise((resolve, reject) => {
		const script = document.createElement("script");
		script.src = `${MONACO}/vs/loader.js`;
		script.onload = () => {
			window.require.config({ paths: { vs: `${MONACO}/vs` } });
			window.require(["vs/editor/editor.main"], () => {
				const monaco = window.monaco;

				monaco.editor.defineTheme("aragon", {
					base: "vs",
					inherit: true,
					rules: [
						{ token: "keyword", foreground: "B35C00", fontStyle: "bold" },
						{ token: "string", foreground: "2E7D32" },
						{ token: "number", foreground: "C2457F" },
						{ token: "comment", foreground: "9A8A70", fontStyle: "italic" },
						{ token: "identifier", foreground: "2B1D0E" },
					],
					colors: {
						"editor.background": "#FFFDF6",
						"editor.foreground": "#2B1D0E",
						"editor.lineHighlightBackground": "#FFF4D8",
						"editorLineNumber.foreground": "#C9B48E",
						"editorLineNumber.activeForeground": "#5A4631",
						"editorCursor.foreground": "#E8A200",
						"editor.selectionBackground": "#FFC21A55",
						"editorIndentGuide.background1": "#F1E4C4",
						"minimap.background": "#FFF8E6",
					},
				});

				resolve(monaco);
			}, reject);
		};
		script.onerror = reject;
		document.head.appendChild(script);
	});

	return monacoPromise;
}

function languageOf(path) {
	if (/\.(luau|lua)$/.test(path)) return "lua";
	if (/\.(ts|tsx)$/.test(path)) return "typescript";
	if (/\.json$/.test(path)) return "json";
	if (/\.toml$/.test(path)) return "ini";
	if (/\.ya?ml$/.test(path)) return "yaml";
	if (/\.md$/.test(path)) return "markdown";
	return "plaintext";
}

function TreeNode({ node, depth, current, onOpen, open, toggle }) {
	const indent = `padding-left:${8 + depth * 14}px`;

	if (node.dir) {
		const expanded = open[node.path];

		return html`<div>
			<button type="button" style=${indent} onClick=${() => toggle(node.path)} title=${node.path}>
				<${Icon} name="chevron" size=${12} style=${expanded ? "" : "transform:rotate(-90deg)"} />
				<${Icon} name="folder" size=${14} />${node.name}
			</button>
			${expanded
				? node.children.map(
						(child) => html`<${TreeNode} key=${child.path} node=${child} depth=${depth + 1} current=${current} onOpen=${onOpen} open=${open} toggle=${toggle} />`,
					)
				: null}
		</div>`;
	}

	const script = /\.(luau|lua|ts|tsx)$/.test(node.name);

	return html`<button
		type="button"
		class=${script ? "luau" : ""}
		style=${`${indent};padding-left:${22 + depth * 14}px`}
		aria-current=${current === node.path ? "true" : "false"}
		onClick=${() => onOpen(node.path)}
		title=${node.path}
	>
		<${Icon} name=${script ? "code" : "file"} size=${14} />${node.name}
	</button>`;
}

export function Editor({ state, params }) {
	const mapped = state.places.filter((p) => p.project);
	const [key, ...rest] = params;
	const place = mapped.find((p) => p.key === key) || null;
	const requested = rest.length ? rest.join("/") : null;

	const [tree, setTree] = useState(null);
	const [open, setOpen] = useState({});
	const [file, setFile] = useState(null);
	const [dirty, setDirty] = useState(false);
	const [fallback, setFallback] = useState(false);
	const [busy, run] = useAction();

	const host = useRef(null);
	const editor = useRef(null);
	const text = useRef("");

	const loadTree = useCallback(() => {
		if (!place) return;

		api(`/api/places/${place.key}/tree`).then(
			(data) => {
				setTree(data);

				// Expand src/ by default
				const next = {};
				for (const child of data.children) if (child.dir && !child.collapsed) next[child.path] = child.name === "src";
				setOpen((prev) => ({ ...next, ...prev }));
			},
			(err) => toast(err.message, "err"),
		);
	}, [place && place.key]);

	useEffect(() => {
		setTree(null);
		setFile(null);
		loadTree();
	}, [loadTree]);

	const openFile = useCallback(
		async (path) => {
			if (dirty && !confirm("Discard unsaved changes?")) return;

			try {
				const data = await api(`/api/places/${place.key}/file?path=${encodeURIComponent(path)}`);
				text.current = data.content;
				setFile({ path, content: data.content });
				setDirty(false);

				// Reveal the file in the tree
				const parts = path.split("/");
				const expand = {};
				for (let i = 1; i < parts.length; i++) expand[parts.slice(0, i).join("/")] = true;
				setOpen((prev) => ({ ...prev, ...expand }));
			} catch (err) {
				toast(err.message, "err");
			}
		},
		[place && place.key, dirty],
	);

	// Deep link from Studio's "Open In Editor"
	useEffect(() => {
		if (place && requested && (!file || file.path !== requested)) openFile(requested);
	}, [place && place.key, requested]);

	// Mount Monaco (or the fallback) whenever a file is shown
	useEffect(() => {
		if (!file || fallback || !host.current) return;

		let disposed = false;

		loadMonaco().then(
			(monaco) => {
				if (disposed || !host.current) return;

				if (!editor.current) {
					editor.current = monaco.editor.create(host.current, {
						theme: "aragon",
						automaticLayout: true,
						fontFamily: "JetBrains Mono, Cascadia Code, Consolas, monospace",
						fontSize: 13,
						lineHeight: 21,
						minimap: { enabled: true, scale: 1 },
						smoothScrolling: true,
						cursorSmoothCaretAnimation: "on",
						tabSize: 4,
						insertSpaces: false,
						renderWhitespace: "selection",
						scrollBeyondLastLine: false,
						padding: { top: 12 },
					});

					editor.current.onDidChangeModelContent(() => {
						text.current = editor.current.getValue();
						setDirty(true);
					});
				}

				const model = monaco.editor.createModel(file.content, languageOf(file.path));
				const old = editor.current.getModel();
				editor.current.setModel(model);
				if (old) old.dispose();
				setDirty(false);
			},
			() => setFallback(true),
		);

		return () => {
			disposed = true;
		};
	}, [file, fallback]);

	useEffect(
		() => () => {
			if (editor.current) {
				editor.current.dispose();
				editor.current = null;
			}
		},
		[],
	);

	const save = useCallback(async () => {
		if (!file) return;

		const ok = await run(
			"save",
			() => api(`/api/places/${place.key}/file?path=${encodeURIComponent(file.path)}`, { method: "PUT", body: { content: text.current } }),
			"Saved, syncing to Studio",
		);

		if (ok) setDirty(false);
	}, [file, place && place.key]);

	useEffect(() => {
		const onKey = (e) => {
			if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
				e.preventDefault();
				save();
			}
		};

		addEventListener("keydown", onKey);
		return () => removeEventListener("keydown", onKey);
	}, [save]);

	const newFile = async () => {
		const name = prompt("New file path (relative to the project)", "src/Shared/NewModule.luau");
		if (!name) return;

		const ok = await run("new", () => api(`/api/places/${place.key}/file?path=${encodeURIComponent(name)}`, { method: "PUT", body: { content: "" } }), "File created");
		if (ok) {
			loadTree();
			openFile(name);
		}
	};

	const remove = async () => {
		if (!file || !confirm(`Move ${file.path} to the Recycle Bin?`)) return;

		const ok = await run("delete", () => api(`/api/places/${place.key}/file?path=${encodeURIComponent(file.path)}`, { method: "DELETE" }), "Moved to Recycle Bin");
		if (ok) {
			setFile(null);
			loadTree();
		}
	};

	if (!mapped.length) {
		return html`<div class="page">
			<div class="empty panel">
				<img src="assets/laptop.webp" alt="" />
				<h2>No mapped places yet</h2>
				<p>Map a place to a folder and edit its code right here. Saving syncs straight into Studio.</p>
				<a class="btn primary" href="#/">Go to places</a>
			</div>
		</div>`;
	}

	return html`<div class="page" style="max-width:none;gap:14px">
		<header class="page-head">
			<div class="toolbar">
				<img class="head-art mascot" src="assets/laptop.webp" alt="" style="width:74px" />
				<h1>Code</h1>
				<select
					class="select"
					value=${place ? place.key : ""}
					onChange=${(e) => go(`#/editor/${e.target.value}`)}
					aria-label="Place"
					style="min-width:240px"
				>
					${place ? null : html`<option value="">Choose a place…</option>`}
					${mapped.map((p) => html`<option value=${p.key}>${p.name}</option>`)}
				</select>
				${place && place.syncedClients > 0 ? html`<span class="badge ok"><span class="dot ok live"></span>Live in Studio</span>` : null}
			</div>
			${place
				? html`<div class="toolbar">
						<${Button} icon="plus" onClick=${newFile}>New file<//>
						<${Button} icon="refresh" className="icon" aria-label="Reload files" title="Reload files" onClick=${loadTree} />
					</div>`
				: null}
		</header>

		${place
			? html`<div class="editor-shell">
					<nav class="tree" aria-label="Project files">
						${tree
							? tree.children.map(
									(node) => html`<${TreeNode}
										key=${node.path}
										node=${node}
										depth=${0}
										current=${file && file.path}
										onOpen=${(path) => go(`#/editor/${place.key}/${path}`)}
										open=${open}
										toggle=${(path) => setOpen({ ...open, [path]: !open[path] })}
									/>`,
								)
							: html`<div style="display:grid;gap:6px;padding:6px">${[1, 2, 3, 4, 5].map(() => html`<div class="skeleton" style="height:20px"></div>`)}</div>`}
					</nav>
					<div class="code-pane">
						<div class="code-bar">
							${file
								? html`<span class="path mono">${file.path}${dirty ? " ●" : ""}</span>
										<${Button} className="ghost sm icon" icon="trash" aria-label="Delete file" title="Move to Recycle Bin" onClick=${remove} />
										<${Button} className="primary sm" icon="save" busy=${busy === "save"} disabled=${!dirty} onClick=${save}>Save<//>`
								: html`<span class="path">Pick a file. Ctrl S saves and syncs to Studio.</span>`}
						</div>
						<div class="code-host" ref=${fallback ? null : host}>
							${file && fallback
								? html`<textarea
										spellcheck="false"
										value=${file.content}
										onInput=${(e) => {
											text.current = e.target.value;
											setDirty(true);
										}}
									></textarea>`
								: null}
							${!file
								? html`<div class="empty" style="height:100%;align-content:center">
										<img src="assets/laptop.webp" alt="" />
										<h2>${place.name}</h2>
										<p>Pick a file on the left. Ctrl S saves it and Studio updates instantly.</p>
									</div>`
								: null}
						</div>
					</div>
				</div>`
			: html`<div class="empty panel"><p>Choose a place to browse its code.</p></div>`}
	</div>`;
}
