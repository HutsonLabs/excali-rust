// Shared by the top-picks drag and drop generators (color-top-picks-dnd.mjs,
// font-top-picks-dnd.mjs): the patch recording upstream's drag sessions,
// the -webkit-touch-callout style radix's ContextMenu.Trigger writes, and
// the fake clock and PointerEvent the scripted drags run on.

// Records each drag session begin() starts (after its early returns): the
// value and origin upstream's pickers pass.
const BEGIN_ANCHOR = `      window.addEventListener("pointermove", onPointerMove, true);
      window.addEventListener("pointerup", onPointerUp, true);`;

export const patchBegin = (source) => {
  const at = source.indexOf(BEGIN_ANCHOR);
  if (source.split(BEGIN_ANCHOR).length !== 2) {
    throw new Error("topPicksDnD.tsx: begin()'s listeners not found once");
  }
  return `${source.slice(0, at)}      globalThis.__dndBegin?.(value, origin);\n${source.slice(at)}`;
};

// radix's ContextMenu.Trigger styles the strip `WebkitTouchCallout: "none"`
// (react-context-menu's ContextMenuTrigger); jsdom's CSSStyleDeclaration
// has no such property, so React's write would be an expando: route it
// through setProperty, which the recorder notes, as WebKit keeps it.
export const installTouchCallout = (window) => {
  Object.defineProperty(window.CSSStyleDeclaration.prototype, "WebkitTouchCallout", {
    configurable: true,
    get() {
      return this.getPropertyValue("-webkit-touch-callout");
    },
    set(value) {
      this.setProperty("-webkit-touch-callout", value);
    },
  });
};

export const START = 1000;

export const installClock = (window, act) => {
  const clock = { now: START, seq: 0, timers: [], frames: [] };
  window.setTimeout = (fn, ms = 0, ...args) => {
    const id = ++clock.seq;
    clock.timers.push({ id, due: clock.now + Math.max(0, Number(ms) || 0), fn: () => fn(...args) });
    return id;
  };
  window.clearTimeout = (id) => {
    clock.timers = clock.timers.filter((t) => t.id !== id);
  };
  globalThis.requestAnimationFrame = (fn) => {
    clock.frames.push(fn);
    return clock.frames.length;
  };
  Object.defineProperty(globalThis.performance, "now", { value: () => clock.now, configurable: true, writable: true });
  clock.advance = async (ms) => {
    const target = clock.now + ms;
    for (;;) {
      const due = clock.timers.filter((t) => t.due <= target).sort((a, b) => a.due - b.due || a.id - b.id)[0];
      if (!due) break;
      clock.timers = clock.timers.filter((t) => t !== due);
      clock.now = due.due;
      await act(async () => due.fn());
    }
    clock.now = target;
  };
  clock.frame = async () => {
    const frames = clock.frames;
    clock.frames = [];
    await act(async () => frames.forEach((f) => f(clock.now)));
  };
  return clock;
};

export const pointerEventClass = (window) =>
  class PointerEvent extends window.MouseEvent {
    constructor(type, init = {}) {
      super(type, init);
      Object.defineProperty(this, "pointerId", { value: init.pointerId ?? 1 });
      Object.defineProperty(this, "pointerType", { value: "mouse" });
    }
  };
