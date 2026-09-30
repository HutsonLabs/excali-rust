// The browser host of the integration guide (ex-607,
// site/content/architecture/integration.md): the module loaded with no
// bundler, under the guide's Content-Security-Policy header
// (specs/csp.spec.mjs). The host's size is set through the CSSOM, which
// style-src does not govern.
import init, { defineExcaliEditor } from "/excali_editor.js";

try {
  await init();
} catch (e) {
  window.initError = String(e);
  throw e;
}
defineExcaliEditor();
const host = document.getElementById("host");
host.style.width = "1000px";
host.style.height = "700px";
const editor = document.createElement("excali-editor");
host.append(editor);
window.editor = editor;
