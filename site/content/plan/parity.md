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
| copy-as-png | Shift+Alt+C puts the selection's PNG export on the clipboard (the canvas when nothing is selected), with the default padding. | `excalidraw/actions/actionClipboard.tsx:193-250` (`actionCopyAsPng`), `excalidraw/data/index.ts:48-96,194-214` (`prepareElementsForExport`, `exportCanvas("clipboard")`), `excalidraw/clipboard.ts:557` (`copyBlobToClipboardAsPng`) | ex-542 | pass |
| host-file-dialogs | Ctrl/Cmd+O and Ctrl/Cmd+Shift+S open the host's file dialogs: the element fires `open-request` and `save-as-request`. | `excalidraw/actions/actionExport.tsx:329-430` (`actionSaveFileToDisk`, `actionLoadScene`), `excalidraw/data/json.ts:76-112` (`saveAsJSON`, `loadFromJSON`) | ex-542 | pass |

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
| view-interactive | Selecting an element paints its selection outline and transform handles on the interactive canvas. | `excalidraw/renderer/interactiveScene.ts:1614` (`_renderInteractiveScene`), `excalidraw/renderer/interactiveScene.ts:1328` (`renderTransformHandles`) | ex-713 | pass |

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
| tool-arrow-points | A, a click, a move, a click, a move, a click and Enter draw a three-point arrow point by point. | `excalidraw/tests/multiPointCreate.test.tsx:88-128`, `excalidraw/components/App.tsx:10211` (`handleLinearElementOnPointerDown`), `excalidraw/actions/actionFinalize.tsx:54` | ex-713 | pass |
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
| move-alt-duplicate | Alt while dragging leaves the element where it was and drags a copy. | `excalidraw/tests/move.test.tsx:147-195`, `excalidraw/components/App.duplicate.ts:163` (`duplicateDraggedSelection`) | ex-713 | pass |
| move-snap | With object snapping on (Alt+S) a dragged element snaps to another's edge within 8 px. | `excalidraw/snapping.ts:692` (`snapDraggedElements`), `excalidraw/actions/actionToggleObjectsSnapMode.tsx:9` | ex-713 | pass |
| move-into-frame | Dragging an element into a frame makes it the frame's child. | `element/tests/frame.test.tsx:689-695`, `element/src/frame.ts:544` (`addElementsToFrame`), `excalidraw/components/App.tsx:12092-12181` | ex-713 | pass |
| linear-editor | Double-clicking a line opens its editor; dragging a segment's midpoint adds a point there. | `element/tests/linearElementEditor.test.tsx:411-418`, `element/tests/linearElementEditor.test.tsx:500-530`, `element/src/linearElementEditor.ts:1781` (`addMidpoint`) | ex-713 | pass |
| resize-handle | Dragging the south-east handle of a 100 × 100 rectangle by (50, 30) makes it 150 × 130. | `element/src/transformHandles.ts:133` (`getTransformHandlesFromCoords`), `element/src/resizeElements.ts` | ex-508 | pass |
| rotate-handle | Dragging the rotation handle rotates the element. | `element/src/transformHandles.ts:133`, `element/src/resizeElements.ts:210` (`rotateSingleElement`) | ex-508 | pass |

## Editing

| Row | Behaviour (upstream at the pin) | Upstream | Issue | Status |
|---|---|---|---|---|
| edit-delete | Delete removes the selected elements. | `excalidraw/actions/actionDeleteSelected.tsx:208` | ex-514 | pass |
| edit-duplicate | Ctrl/Cmd+D duplicates the selection 10 px right and down. | `excalidraw/actions/actionDuplicateSelection.tsx:34`, `excalidraw/actions/actionDuplicateSelection.tsx:78-79` | ex-514 | pass |
| edit-group | Ctrl/Cmd+G gives the selected elements one shared group id. | `excalidraw/actions/actionGroup.tsx:86` | ex-514 | pass |
| edit-zorder | Ctrl+Shift+] brings the selection to the front. On a Mac its key, Cmd+Alt+], also passes bring forward's key test, so the action manager cancels it and Cmd+] brings the selection forward instead. | `excalidraw/actions/actionZindex.tsx:60-70`, `excalidraw/actions/actionZindex.tsx:120-141`, `excalidraw/actions/manager.tsx:114-119` | ex-514 | pass |
| edit-undo-redo | Ctrl/Cmd+Z undoes a move and Ctrl/Cmd+Shift+Z redoes it. | `excalidraw/history.ts`, `element/src/store.ts` | ex-513 | pass |
| edit-copy-paste | Ctrl/Cmd+C then Ctrl/Cmd+V pastes a copy of the selection. | `excalidraw/actions/actionClipboard.tsx:23`, `excalidraw/actions/actionClipboard.tsx:55` | ex-514 | pass |

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
| ui-main-menu | The hamburger (`main-menu-trigger`) opens the main menu. | `excalidraw/components/main-menu/MainMenu.tsx:51` | ex-520 | pass |
| ui-styles-panel | Selecting an element shows the styles panel (`.App-menu__left`) with the stroke colour picker's trigger (`.color-picker__button`, labelled Stroke). | `excalidraw/components/Actions.tsx` (`SelectedShapeActions`), `excalidraw/components/LayerUI.tsx`, `excalidraw/actions/actionProperties.tsx:413` (`ColorPicker`), `excalidraw/components/ColorPicker/ColorPicker.tsx:306` | ex-523 | pass |
| ui-footer-zoom | The footer has the zoom actions (`.zoom-actions`) showing 100%. | `excalidraw/components/Actions.tsx:877` (`ZoomActions`), `common/src/constants.ts:114` | ex-521 | pass |
| ui-help-dialog | `?` opens the help dialog (`.HelpDialog`, in the modal container on the body) with the Tools, View and Editor islands; Esc closes it. | `excalidraw/actions/actionMenu.tsx:34`, `excalidraw/components/HelpDialog.tsx:141-516`, `excalidraw/hooks/useCreatePortalContainer.ts:31-43`, `excalidraw/components/Modal.tsx` | ex-522 | pass |
| ui-context-menu | A right-click on the canvas opens the context menu (`.context-menu`). | `excalidraw/components/App.tsx:13363` (`handleCanvasContextMenu`), `excalidraw/components/ContextMenu.tsx:68` | ex-525 | pass |
| ui-library | The library trigger (`.default-sidebar-trigger`) opens the library sidebar (`.default-sidebar`) on its library tab (`.layer-ui__library`). | `excalidraw/components/DefaultSidebar.tsx:37`, `excalidraw/components/LibraryMenu.tsx:60` | ex-526 | pass |
| ui-command-palette | Ctrl/Cmd+/ opens the command palette (`.command-palette-dialog`, in the modal container on the body) with the categories App, Export, Editor, Tools, Elements and Links in that order; typing searches, Enter runs the selected command and closes it; Ctrl/Cmd+Shift+P opens it again with that command under "Recently used"; Esc closes it. | `excalidraw/components/CommandPalette/CommandPalette.tsx:97-114, 141-186, 645-671, 771-931`, `excalidraw-app/App.tsx:1111-1290` | ex-527 | pass |
| ui-search | Ctrl/Cmd+F opens the default sidebar on its search tab (`.layer-ui__search`) with the field focused; typing searches the texts and frame names, the matches grouped under Frames then Texts with the first visible one focused (`1 / 3 results`); Enter focuses the next; Esc closes the sidebar. | `excalidraw/actions/actionToggleSearchMenu.ts:26-57`, `excalidraw/components/SearchMenu.tsx:279-338, 340-519, 776-866` | ex-708 | pass |
| ui-welcome-screen | An empty scene shows the welcome screen (`.welcome-screen-center`). | `excalidraw/components/App.tsx:4342`, `excalidraw/components/welcome-screen/WelcomeScreen.Center.tsx:95` | ex-528 | pass |
| ui-hints | With a tool active the hint viewer (`.HintViewer`) shows upstream's hint. | `excalidraw/components/HintViewer.tsx:305`, `excalidraw/locales/en.json:377` | ex-528 | pass |
| ui-stats | Alt+/ opens the stats panel (`.exc-stats`). | `excalidraw/actions/actionToggleStats.tsx:26`, `excalidraw/components/Stats/index.tsx:186` | ex-529 | pass |
| ui-stats-edit | In the stats panel a typed X moves the selected element and a W label dragged to the right widens it, each undone in one step. | `excalidraw/components/Stats/DragInput.tsx:122`, `excalidraw/components/Stats/Position.tsx:30`, `excalidraw/components/Stats/Dimension.tsx:44` | ex-539 | pass |
