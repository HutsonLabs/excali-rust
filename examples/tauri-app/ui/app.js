// The example app's frontend (ex-606): <excali-editor> from the web
// runtime, with tauri-plugin-excali doing what the editor leaves to its host
// (site/content/architecture/tauri.md): the file dialogs, reading and
// writing files, headless export and fetching libraries. No bundler: this
// file and the runtime are plain ES modules served by Tauri.
import init, { defineExcaliEditor } from "./excali/excali_editor.js";

const { invoke } = window.__TAURI__.core;

// CSP violations and uncaught errors, for the status line and smoke mode.
const problems = [];
document.addEventListener("securitypolicyviolation", (e) => {
  problems.push(`CSP ${e.violatedDirective}: ${e.blockedURI || "inline"}`);
});
window.addEventListener("error", (e) => problems.push(`error: ${e.message}`));
window.addEventListener("unhandledrejection", (e) => problems.push(`rejection: ${e.reason}`));

const fileLabel = document.getElementById("file");
const statusLine = document.getElementById("status");

function status(text, error = false) {
  statusLine.textContent = text;
  statusLine.classList.toggle("error", error);
}

// Smoke mode (EXCALI_EXAMPLE_SMOKE, scripts/smoke.sh) reports a runtime
// that fails to load too.
const smoke = await invoke("smoke");
try {
  await init();
} catch (e) {
  if (smoke) await invoke("smoke_report", { report: { problems, error: `init: ${e}` } });
  throw e;
}
defineExcaliEditor();
const editor = document.createElement("excali-editor");
editor.setAttribute("theme", matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light");
document.getElementById("host").append(editor);

// The open file: its path (a save writes back there) and name without the
// extension (what the save and export dialogs suggest).
let current = null;

function setCurrent(path, name) {
  current = { path, name: name.replace(/\.(excalidraw|json|png|svg)$/i, "").replace(/\.excalidraw$/i, "") };
  fileLabel.textContent = name;
  fileLabel.title = path;
  document.title = `${name} - Excali Example`;
}

async function openScene() {
  const file = await invoke("plugin:excali|open");
  if (!file) return null;
  await editor.load(file.text);
  setCurrent(file.path, file.name);
  status(`Opened ${file.path}`);
  return file.path;
}

async function saveScene({ as = false } = {}) {
  const path = await invoke("plugin:excali|save", {
    text: editor.save(),
    path: as ? null : (current?.path ?? null),
    name: current?.name ?? null,
  });
  if (!path) return null;
  setCurrent(path, path.split(/[\\/]/).pop());
  status(`Saved ${path}`);
  return path;
}

async function exportScene(format) {
  const path = await invoke("plugin:excali|export", {
    scene: editor.save(),
    format,
    options: { darkMode: editor.getAttribute("theme") === "dark" },
    save: { name: current?.name ?? null },
  });
  if (path) status(`Exported ${path}`);
  return path;
}

async function importLibrary() {
  const file = await invoke("plugin:excali|open", { kind: "library" });
  if (!file) return null;
  const count = await editor.importLibrary(file.text);
  status(`Imported ${count} library items from ${file.name}`);
  return count;
}

// Library URLs (#addLibrary links) are fetched natively, allow-listed.
editor.addEventListener("library-fetch", (e) => {
  e.preventDefault();
  e.detail.respond(invoke("plugin:excali|library_fetch", { url: e.detail.url }));
});

const actions = {
  open: openScene,
  save: () => saveScene(),
  "save-as": () => saveScene({ as: true }),
  "export-png": () => exportScene("png"),
  "export-svg": () => exportScene("svg"),
  "import-library": importLibrary,
};

async function run(action) {
  try {
    await actions[action]();
  } catch (e) {
    status(String(e?.message ?? e), true);
  }
}

document.querySelector(".bar").addEventListener("click", (e) => {
  const action = e.target.closest("button[data-action]")?.dataset.action;
  if (action) run(action);
});
// Cmd+S, Cmd+O and Cmd+Shift+S inside the editor.
editor.addEventListener("save-request", () => run("save"));
editor.addEventListener("open-request", (e) => {
  e.preventDefault();
  run("open");
});
editor.addEventListener("save-as-request", (e) => {
  e.preventDefault();
  run("save-as");
});
document.addEventListener("keydown", (e) => {
  if (!(e.metaKey || e.ctrlKey) || e.altKey) return;
  const key = e.key.toLowerCase();
  const action = key === "o" ? "open" : key === "s" ? (e.shiftKey ? "save-as" : "save") : null;
  // inside the editor these arrive as save-request, open-request and
  // save-as-request
  if (!action || editor.contains(e.target)) return;
  e.preventDefault();
  run(action);
});

// Smoke mode: the same handlers, with dialogs that answer paths in the
// smoke directory.
if (smoke) {
  const report = { problems };
  try {
    report.mounted = editor.querySelector("canvas") !== null;
    report.opened = await openScene();
    report.stateAfterOpen = editor.getState();
    report.saved = await saveScene();
    report.savedAs = await saveScene({ as: true });
    report.png = await exportScene("png");
    report.svg = await exportScene("svg");
    report.scene = JSON.parse(editor.save());
  } catch (e) {
    report.error = String(e?.message ?? e);
  }
  await invoke("smoke_report", { report });
}
