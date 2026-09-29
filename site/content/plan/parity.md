+++
title = "Parity checklist"
description = "Each behaviour of upstream Excalidraw the v1 editor must match, as a row the Playwright parity suite tests on the <excali-editor> element in Chromium."
weight = 6
+++

Every row below is one test of `tests/web/specs/parity.spec.mjs`, named by the row's id. The suite loads the release of `scripts/web/build.sh` into the page the [term.hut integration](@/architecture/termhut-integration.md) snippet describes (`tests/web/page/editor.html`), mounts `<excali-editor>` from plain JS, drives it with Chromium's own pointer and keyboard events, and checks what upstream does at the pinned commit. Upstream paths are under `packages/` at that commit. The `web-runtime` job of `.github/workflows/rust.yml` runs it on every pull request.

The status is the row's state on `main`:

- **pass**: the element does what upstream does, and the test holds it there.
- **gap**: the behaviour is not wired into the element yet. The test still runs the gesture and checks upstream's result, as a Playwright expected failure (`test.fail`). When the behaviour lands the test passes, which fails the suite until the row is marked `pass`, so the checklist cannot fall behind the code. `ex-712` takes every row to `pass`; the issue column names the port of the feature itself.

The suite's first test fails when a row has no test, a test has no row, or a status is neither `pass` nor `gap`.

## File and host API

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| file-roundtrip | `load` then `save` keeps every element: saving the saved file again gives the same text. | `excalidraw/data/json.ts:52` (`serializeAsJSON`), `excalidraw/data/restore.ts` | ex-530 | pass |
| file-envelope | The saved file is `{ type: "excalidraw", version: 2, source, elements, appState, files }`, 2-space JSON, with only the `export: true` app state keys. | `excalidraw/data/json.ts:52-74`, `excalidraw/appState.ts:221-286` | ex-530 | pass |
| file-legacy-indices | A file whose elements have no `index` loads with fractional indices `a0`, `a1`, … in scene order. | `excalidraw/data/restore.ts` (`restoreElements`), `element/src/fractionalIndex.ts:223` (`syncInvalidIndices`) | ex-530 | pass |
| library-v1 | `importLibrary` takes a version 1 `.excalidrawlib` (`library: [[…]]`), and `exportLibrary` writes version 2 with `libraryItems`. | `excalidraw/data/blob.ts:218` (`parseLibraryJSON`), `excalidraw/data/json.ts:137` (`serializeLibraryAsJSON`) | ex-530 | pass |
| export-svg | `export("svg")` gives the SVG document upstream saves (the XML preamble, then an `<svg>` carrying `svg-source:excalidraw`, and with `embedScene` the `payload-type:application/vnd.excalidraw+json` comment). | `common/src/constants.ts:410` (`SVG_DOCUMENT_PREAMBLE`), `excalidraw/data/index.ts:143`, `excalidraw/scene/export.ts:368`, `excalidraw/scene/export.ts:523` | ex-530 | pass |
| export-png-size | `export("png")` of one 100 × 100 rectangle is a 120 × 120 PNG: the bounds plus the default padding of 10 on each side. | `common/src/constants.ts:402` (`DEFAULT_EXPORT_PADDING`), `excalidraw/scene/export.ts` (`exportToCanvas`) | ex-530 | pass |
| host-events | An edit fires `change` with `{ dirty: true }`; Ctrl/Cmd+S fires `save-request` instead of the browser's save. | `excalidraw/components/App.tsx` (`onChange`), `excalidraw/actions/actionExport.tsx:253` (`actionSaveToActiveFile`) | ex-530 | pass |

## Canvas and view

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| view-theme | The `theme` attribute toggles `theme--dark` on the `.excalidraw` container. | `excalidraw/components/App.tsx` (`theme--dark` class) | ex-532 | pass |
| view-device-pixels | At device pixel ratio 2 each canvas's backing store is twice its CSS size. | `excalidraw/components/canvases/StaticCanvas.tsx`, `excalidraw/renderer/helpers.ts` | ex-503 | pass |
| view-zoom-keys | Ctrl/Cmd+= zooms in by `ZOOM_STEP` (0.1) and Ctrl/Cmd+0 resets to 100%. | `common/src/constants.ts:362`, `excalidraw/actions/actionCanvas.tsx:129` (`actionZoomIn`), `excalidraw/actions/actionCanvas.tsx:233` (`actionResetZoom`) | ex-505 | pass |
| view-wheel-zoom | Ctrl+wheel zooms the canvas. | `excalidraw/components/App.wheel.ts` (`AppWheel`) | ex-505 | pass |
| view-wheel-scroll | The wheel scrolls the canvas: after scrolling down 100 px an element 100 px lower is under the pointer. | `excalidraw/components/App.wheel.ts` (`AppWheel`) | ex-505 | pass |
| view-hand-pan | With the hand tool (H), dragging pans the canvas. | `excalidraw/components/App.pan.ts` (`AppPan`) | ex-505 | pass |
| view-space-pan | Holding Space while dragging pans the canvas. | `excalidraw/components/App.tsx:5989`, `excalidraw/components/App.pan.ts` | ex-505 | pass |

## Tools

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| tool-toolbar-order | The toolbar's buttons are upstream's desktop toolbar, in order (the `default` case of `crates/excali-ui/tests/fixtures/toolbar.json`, rendered from upstream's `Toolbar`). | `excalidraw/components/Toolbar.tsx` | ex-518 | pass |
| tool-toolbar-click | Clicking a toolbar button makes its tool active. | `excalidraw/components/Toolbar.tsx`, `excalidraw/components/App.tsx` (`setActiveTool`) | ex-518 | pass |
| tool-letters | The tool letters select their tools: V, R, D, O, A, L, P, T, E and H. | `excalidraw/components/App.tsx` (`onKeyDown`, `findShapeByKey`) | ex-515 | pass |
| tool-rectangle | R and a drag from (100, 100) to (250, 200) create a 150 × 100 rectangle with upstream's defaults (stroke `#1e1e1e`, transparent background, solid fill, width 2, roughness 1, opacity 100, adaptive roundness), selected, with the selection tool back. | `element/src/newElement.ts:87` (`_newElementBase`), `common/src/constants.ts:514` (`DEFAULT_ELEMENT_PROPS`), `element/src/typeChecks.ts:357`, `excalidraw/components/App.tsx:10534` | ex-506 | pass |
| tool-diamond | D and a drag create a diamond with proportional roundness. | `element/src/typeChecks.ts:357`, `excalidraw/components/App.tsx:10534` | ex-506 | pass |
| tool-ellipse | O and a drag create an ellipse; its roundness is the current item's, proportional by default (`getCurrentItemRoundness` makes no exception for the ellipse, whose shape does not draw it). | `excalidraw/components/App.tsx:10508` (`getCurrentItemRoundness`), `excalidraw/appState.ts:44`, `excalidraw/components/App.tsx:10534` | ex-506 | pass |
| tool-arrow | A and a drag create a two-point arrow ending in an `arrow` arrowhead. | `excalidraw/appState.ts:33` (`currentItemEndArrowhead`), `excalidraw/components/App.tsx:10198`, `element/src/newElement.ts:604` | ex-506 | pass |
| tool-line | L and a drag create a two-point line with no arrowheads. | `excalidraw/components/App.tsx:10198`, `element/src/newElement.ts:604` | ex-506 | pass |
| tool-freedraw | P and a drag create a freedraw element through the pointer's points. | `excalidraw/components/App.tsx:9988`, `element/src/newElement.ts:583` | ex-506 | pass |
| tool-text | T, a click, typing and Escape create a text element at font size 20 in Excalifont (5). | `common/src/constants.ts:223` (`DEFAULT_FONT_SIZE`), `common/src/constants.ts:268` (`DEFAULT_FONT_FAMILY`), `excalidraw/components/App.tsx:7044` (`startTextEditing`) | ex-512 | pass |
| tool-eraser | E and a drag across an element delete it. | `excalidraw/components/App.tsx:8530` (`handleEraser`) | ex-506 | pass |
| tool-frame | F and a drag create a frame. | `excalidraw/components/App.tsx:10603`, `element/src/newElement.ts:263` | ex-506 | pass |
| tool-lock | With the tool locked (Q), the tool stays active after drawing. | `excalidraw/components/App.tsx` (`onPointerUpFromPointerDownHandler`, `activeTool.locked`) | ex-506 | pass |

## Selection and transforms

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| select-click | A click selects the element under the pointer; a click on empty canvas clears the selection. | `excalidraw/components/App.tsx:9515` (`handleSelectionOnPointerDown`) | ex-507 | pass |
| select-shift | Shift+click adds an element to the selection. | `excalidraw/components/App.tsx:9515` | ex-507 | pass |
| select-box | A drag from empty canvas selects the elements the box encloses. | `excalidraw/components/App.tsx:8683` (`handleCanvasPointerDown`), `element/src/selection.ts:70` (`getElementsWithinSelection`) | ex-508 | pass |
| select-all | Ctrl/Cmd+A selects every element. | `excalidraw/actions/actionSelectAll.ts:21` | ex-514 | pass |
| move-drag | Dragging a selected element moves it by the pointer's offset. | `element/src/dragElements.ts:39` (`dragSelectedElements`) | ex-530 | pass |
| move-nudge | The arrow keys move the selection by 1, and by 5 with Shift. | `common/src/constants.ts:31-32`, `excalidraw/components/App.tsx` (`onKeyDown`) | ex-515 | pass |
| resize-handle | Dragging the south-east handle of a 100 × 100 rectangle by (50, 30) makes it 150 × 130. | `element/src/transformHandles.ts:133` (`getTransformHandlesFromCoords`), `element/src/resizeElements.ts` | ex-508 | pass |
| rotate-handle | Dragging the rotation handle rotates the element. | `element/src/transformHandles.ts:133`, `element/src/resizeElements.ts:210` (`rotateSingleElement`) | ex-508 | pass |

## Editing

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| edit-delete | Delete removes the selected elements. | `excalidraw/actions/actionDeleteSelected.tsx:208` | ex-514 | gap |
| edit-duplicate | Ctrl/Cmd+D duplicates the selection 10 px right and down. | `excalidraw/actions/actionDuplicateSelection.tsx:34`, `excalidraw/actions/actionDuplicateSelection.tsx:78-79` | ex-514 | gap |
| edit-group | Ctrl/Cmd+G gives the selected elements one shared group id. | `excalidraw/actions/actionGroup.tsx:86` | ex-514 | gap |
| edit-zorder | Ctrl/Cmd+Shift+] brings the selection to the front. | `excalidraw/actions/actionZindex.tsx:120` | ex-514 | gap |
| edit-undo-redo | Ctrl/Cmd+Z undoes a move and Ctrl/Cmd+Shift+Z redoes it. | `excalidraw/history.ts`, `element/src/store.ts` | ex-513 | pass |
| edit-copy-paste | Ctrl/Cmd+C then Ctrl/Cmd+V pastes a copy of the selection. | `excalidraw/actions/actionClipboard.tsx:23`, `excalidraw/actions/actionClipboard.tsx:55` | ex-514 | gap |

## Bound text and arrows

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| bound-label-follows | Dragging a container moves its label with it. | `element/src/dragElements.ts:39` (`dragSelectedElements`) | ex-530 | pass |
| bound-arrow-follows | Dragging a shape an arrow is bound to moves the arrow's end with it. | `element/src/binding.ts:1321` (`updateBoundElements`) | ex-510 | pass |
| bound-arrow-create | An arrow drawn from inside one shape to inside another binds both ends. | `element/src/binding.ts:151` (`bindOrUnbindBindingElement`), `excalidraw/components/App.tsx:10198` | ex-510 | pass |
| text-dblclick-edit | Double-clicking a text element opens its editor: a `dir="auto"`, `wrap="off"` textarea. | `excalidraw/components/App.tsx:7340` (`handleCanvasDoubleClick`), `excalidraw/wysiwyg/textWysiwyg.tsx` | ex-512 | pass |
| text-dblclick-label | Double-clicking inside a shape opens a textarea for its label. | `excalidraw/components/App.tsx:7340` | ex-512 | pass |

## Chrome

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| ui-main-menu | The hamburger (`main-menu-trigger`) opens the main menu. | `excalidraw/components/main-menu/MainMenu.tsx:51` | ex-520 | gap |
| ui-styles-panel | Selecting an element shows the styles panel (`.App-menu__left` with the stroke colour picker). | `excalidraw/components/Actions.tsx` (`SelectedShapeActions`), `excalidraw/components/LayerUI.tsx` | ex-519 | gap |
| ui-footer-zoom | The footer has the zoom actions (`.zoom-actions`) showing 100%. | `excalidraw/components/Actions.tsx:877` (`ZoomActions`), `common/src/constants.ts:114` | ex-521 | pass |
| ui-help-dialog | `?` opens the help dialog (`.HelpDialog`). | `excalidraw/actions/actionMenu.tsx:34`, `excalidraw/components/HelpDialog.tsx:141` | ex-522 | gap |
| ui-context-menu | A right-click on the canvas opens the context menu (`.context-menu`). | `excalidraw/components/App.tsx:13363` (`handleCanvasContextMenu`), `excalidraw/components/ContextMenu.tsx:68` | ex-525 | gap |
| ui-library | The library trigger (`.default-sidebar-trigger`) opens the library sidebar (`.library-menu`). | `excalidraw/components/DefaultSidebar.tsx:37`, `excalidraw/components/LibraryMenuHeaderContent.tsx:203` | ex-526 | gap |
| ui-command-palette | Ctrl/Cmd+/ opens the command palette (`.command-palette-dialog`). | `excalidraw/components/CommandPalette/CommandPalette.tsx:146-180`, `excalidraw/components/CommandPalette/CommandPalette.tsx:888` | ex-527 | gap |
| ui-welcome-screen | An empty scene shows the welcome screen (`.welcome-screen-center`). | `excalidraw/components/App.tsx:4342`, `excalidraw/components/welcome-screen/WelcomeScreen.Center.tsx:95` | ex-528 | gap |
| ui-hints | With a tool active the hint viewer (`.HintViewer`) shows upstream's hint. | `excalidraw/components/HintViewer.tsx:305`, `excalidraw/locales/en.json:377` | ex-528 | gap |
| ui-stats | Alt+/ opens the stats panel (`.exc-stats`). | `excalidraw/actions/actionToggleStats.tsx:26`, `excalidraw/components/Stats/index.tsx:186` | ex-529 | gap |
