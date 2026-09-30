// Sticky notes for the rendering goldens (ex-703): the scenes static-scene.mjs
// draws on its recording canvas and svg-export.mjs exports, covering what
// getStickyNotePathCommands (packages/element/src/stickyNote.ts:275-370) and
// getStickyNoteFooter (:471-500) branch on: roughness 0 (no jitter), 1 and 2
// (the lifted corner, one note per corner), roundness (quadratic corners), a
// note narrower than the footer's year bucket (short date), one under
// STICKY_NOTE_MIN_SIZE (no footer), one without a timestamp, one created in
// the current year, opacity, rotation, colours and a bound label.

/**
 * Date.now() while the goldens render: getStickyNoteFooter drops the year of
 * a note created in the current year. The fixtures record it as `now`.
 */
export const NOW = Date.UTC(2026, 8, 29, 12, 0, 0);
/** A timestamp in NOW's year, late in the day (UTC). */
export const CREATED_THIS_YEAR = Date.UTC(2026, 2, 5, 23, 30, 0);

/** Runs fn with Date.now() pinned to NOW. */
export const withPinnedNow = async (fn) => {
  const now = Date.now;
  Date.now = () => NOW;
  try {
    return await fn();
  } finally {
    Date.now = now;
  }
};

/** The sticky notes, laid out left to right from (x, y). */
export const stickyNotes = (up, { x = 0, y = 0 } = {}) => {
  const note = (id, dx, dy, props) =>
    up.newStickyNoteElement({ type: "stickynote", id, x: x + dx, y: y + dy, width: 200, height: 200, ...props });
  const labelled = note("sn-labelled", 0, 480, { seed: 30, created: CREATED_THIS_YEAR, angle: 0.15 });
  const label = up.newTextElement({
    id: "sn-label",
    x: x + 16,
    y: y + 496,
    text: "a label",
    containerId: labelled.id,
    textAlign: "left",
    verticalAlign: "top",
    fontSize: 28,
    seed: 31,
  });
  labelled.boundElements = [{ type: "text", id: label.id }];
  return [
    note("sn-smooth", 0, 0, { seed: 11, roughness: 0 }),
    note("sn-rough", 240, 0, { seed: 12 }),
    note("sn-round", 480, 0, { seed: 13, roundness: { type: 3 }, created: CREATED_THIS_YEAR }),
    note("sn-smooth-round", 720, 0, { seed: 14, roughness: 0, roundness: { type: 3 } }),
    // roughness 2: seeds 23, 21, 22 and 20 lift corners 0, 1, 2 and 3
    note("sn-lift-0", 0, 240, { seed: 23, roughness: 2 }),
    note("sn-lift-1", 240, 240, { seed: 21, roughness: 2, roundness: { type: 3 }, angle: -0.2 }),
    note("sn-lift-2", 480, 240, { seed: 22, roughness: 2, strokeColor: "#e03131", backgroundColor: "#a5d8ff" }),
    note("sn-lift-3", 720, 240, { seed: 20, roughness: 2, opacity: 40 }),
    labelled,
    label,
    // body under minBodyWidthForYear: the short date
    note("sn-narrow", 240, 480, { seed: 15, width: 100, height: 150 }),
    // under STICKY_NOTE_MIN_SIZE: no footer
    note("sn-small", 380, 480, { seed: 16, width: 60, height: 90 }),
    // no timestamp: no footer
    note("sn-undated", 480, 480, { seed: 17, created: null }),
    note("sn-thin", 720, 480, { seed: 18, width: 300, height: 4, roughness: 2 }),
  ];
};
