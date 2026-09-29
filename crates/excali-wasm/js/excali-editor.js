
// <excali-editor> (ex-530): the custom element shim. scripts/web/build.sh
// appends this file to wasm-bindgen's excali_editor.js, so the module is one
// plain ES module and EditorCore, registerFonts and loadSceneFonts above are
// in scope. The API is the one on the term.hut integration page
// (site/content/architecture/termhut-integration.md); the editor itself is
// excali_wasm::web::EditorCore.

const EXCALI_FONTS_BASE = new URL("./fonts/", import.meta.url).href;

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
      (type, detail) => this.#emit(type, detail, type === "open-link"),
      EXCALI_FONTS_BASE,
    );
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
    if (name === "theme") this.#core.setTheme(value || "light");
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
      await loadSceneFonts(core.sceneJson());
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
