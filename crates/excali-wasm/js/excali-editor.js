
// <excali-editor> (ex-530): the custom element shim. scripts/web/build.sh
// appends this file to wasm-bindgen's excali_editor.js, so the module is one
// plain ES module and EditorCore and registerFonts above are
// in scope. The API is the one on the term.hut integration page
// (site/content/architecture/termhut-integration.md); the editor itself is
// excali_wasm::web::EditorCore.

const EXCALI_FONTS_BASE = new URL("./fonts/", import.meta.url).href;

const CANCELABLE = new Set(["open-link", "library-publish", "open-request", "save-as-request"]);

class ExcaliEditorElement extends HTMLElement {
  static get observedAttributes() {
    return ["theme", "ui"];
  }

  #core = null;
  #resize = null;

  connectedCallback() {
    if (this.#core) return;
    try {
      registerFonts(EXCALI_FONTS_BASE);
    } catch {
      // no document.fonts: text falls back to the browser's fonts
    }
    this.#core = new EditorCore(
      this,
      // a host answers open-link, library-publish, open-request and
      // save-as-request with preventDefault()
      (type, detail) => {
        const handled = !this.#emit(type, detail, CANCELABLE.has(type));
        if (!handled && type === "open-request") queueMicrotask(() => this.#openFile());
        if (!handled && type === "save-as-request") {
          queueMicrotask(() => this.#downloadFile(detail.name));
        }
        return !handled;
      },
      EXCALI_FONTS_BASE,
    );
    // a theme attribute makes the theme the host's, as upstream's theme
    // prop without onThemeChange (index.tsx:142-147): no theme toggle
    this.#core.setThemeControlled(this.hasAttribute("theme"));
    this.#core.setTheme(this.getAttribute("theme") || "light");
    this.#core.setUi(this.getAttribute("ui") || "full");
    this.#resize = new ResizeObserver(() => this.#core?.resize());
    this.#resize.observe(this);
  }

  disconnectedCallback() {
    this.#resize?.disconnect();
    this.#resize = null;
    if (this.#core) {
      this.#core.destroy();
      this.#core.free();
      this.#core = null;
    }
  }

  attributeChangedCallback(name, _old, value) {
    if (!this.#core) return;
    if (name === "theme") {
      // null when the attribute was removed: the toggle comes back. Set
      // again with the same value, it still repaints, which reads the
      // host's --excali-canvas-background anew.
      this.#core.setThemeControlled(value !== null);
      this.#core.setTheme(value || "light");
    }
    if (name === "ui") this.#core.setUi(value || "full");
  }

  #emit(type, detail, cancelable = false) {
    return this.dispatchEvent(
      new CustomEvent(type, { detail, bubbles: true, composed: true, cancelable }),
    );
  }

  #editor() {
    if (!this.#core) throw new Error("The <excali-editor> is not in a document.");
    return this.#core;
  }

  /** Loads .excalidraw JSON text; rejects with a one-sentence reason. */
  async load(text) {
    const core = this.#editor();
    core.load(String(text));
    try {
      await core.loadFonts();
      core.repaint();
    } catch {
      // a family that fails to load leaves the fallback font
    }
  }

  /** The scene as .excalidraw JSON text (2-space, upstream key order). */
  save() {
    return this.#editor().save();
  }

  /** "png": a Promise of an image/png Blob; "svg": the SVG text. */
  export(type, options = {}) {
    const core = this.#editor();
    if (type === "svg") return core.exportSvg(options);
    if (type === "png") {
      try {
        return Promise.resolve(new Blob([core.exportPng(options)], { type: "image/png" }));
      } catch (e) {
        return Promise.reject(e);
      }
    }
    throw new Error(`Unknown export type "${type}"; use "png" or "svg".`);
  }

  /**
   * Imports a .excalidrawlib (its text, or an allow-listed URL the host
   * fetches through library-fetch). Resolves to the library's item count.
   */
  async importLibrary(textOrUrl, { merge = true } = {}) {
    const core = this.#editor();
    const input = String(textOrUrl);
    const url = core.libraryUrl(input);
    const text = url === undefined ? input : await this.#fetchLibrary(url);
    return core.importLibrary(text, merge);
  }

  #fetchLibrary(url) {
    return new Promise((resolve, reject) => {
      let answered = false;
      const detail = {
        url,
        respond: (text) => {
          answered = true;
          Promise.resolve(text).then((t) => resolve(String(t)), reject);
        },
        reject: (error) => {
          answered = true;
          reject(error instanceof Error ? error : new Error(String(error)));
        },
      };
      const handled = !this.#emit("library-fetch", detail, true);
      if (!handled && !answered) {
        reject(new Error(`The host did not handle library-fetch for ${url}.`));
      }
    });
  }

  // open-request unanswered: loadFromJSON's fileOpen (data/json.ts:101-112)
  // with the browser's file input, as over-permissive as upstream's (no
  // extensions); the file's text loads as load(text) does, a file that does
  // not load leaving the scene as it was.
  #openFile() {
    const input = document.createElement("input");
    input.type = "file";
    input.addEventListener("change", async () => {
      const file = input.files?.[0];
      if (!file) return;
      try {
        await this.load(await file.text());
      } catch (e) {
        console.warn(e);
      }
    });
    input.click();
  }

  // save-as-request unanswered: saveAsJSON's fileSave (data/json.ts:76-99) as
  // browser-fs-access's legacy download, `${name}.excalidraw`.
  #downloadFile(name) {
    const blob = new Blob([this.#editor().save()], { type: "application/vnd.excalidraw+json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `${name}.excalidraw`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 30_000);
  }

  /** The personal library as .excalidrawlib JSON text. */
  exportLibrary() {
    return this.#editor().libraryJson();
  }

  /** { dirty, elementCount, zoom, selectionCount, activeTool } */
  getState() {
    return this.#editor().getState();
  }
}

/** Registers <excali-editor> (once). */
export function defineExcaliEditor() {
  if (!customElements.get("excali-editor")) {
    customElements.define("excali-editor", ExcaliEditorElement);
  }
  return customElements.get("excali-editor");
}
