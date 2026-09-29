// The independent reading of excali-raster's fixture display lists
// (ex-401): every call goes to a real CanvasRenderingContext2D as the list
// describes it, and the browser applies its own rules. It shares no code
// with the port.
//
// Used by scripts/fixtures/raster_references.html (the Chrome references of
// crates/excali-raster/tests/fixtures/chrome/) and by the Canvas 2D
// backend's fixture check (tests/web/canvas2d, ex-502), which draws each
// fixture with it next to excali-canvas2d's painting.
//
// Fixture vocabulary: crates/excali-raster/tests/fixtures/README.md.
// Numbers may be "NaN", "Infinity" or "-Infinity".

const num = (v) => (typeof v === "string" ? Number(v) : v);

// A path call the canvas throws on (arc and roundRect with a negative
// radius) does nothing, as the port drops it; the rest of the path goes on.
function trace(ctx, path) {
  ctx.beginPath();
  for (const call of path) {
    const [op, ...args] = call;
    const n = args.map(num);
    try {
      switch (op) {
        case "M": ctx.moveTo(n[0], n[1]); break;
        case "L": ctx.lineTo(n[0], n[1]); break;
        case "Q": ctx.quadraticCurveTo(n[0], n[1], n[2], n[3]); break;
        case "C": ctx.bezierCurveTo(n[0], n[1], n[2], n[3], n[4], n[5]); break;
        case "A": ctx.arc(n[0], n[1], n[2], n[3], n[4], args[5] === true); break;
        case "Z": ctx.closePath(); break;
        case "rect": ctx.rect(n[0], n[1], n[2], n[3]); break;
        case "roundRect": ctx.roundRect(n[0], n[1], n[2], n[3], n[4]); break;
        default: throw new Error(`unknown path call ${op}`);
      }
    } catch (e) {
      if (!(e instanceof DOMException || e instanceof RangeError)) throw e;
    }
  }
}

function draw(ctx, item, images) {
  switch (item.type) {
    case "fill":
      ctx.save();
      ctx.fillStyle = item.color;
      trace(ctx, item.path);
      ctx.fill(item.rule || "nonzero");
      ctx.restore();
      break;
    case "fillRect": {
      ctx.save();
      ctx.fillStyle = item.color;
      const r = item.rect.map(num);
      ctx.fillRect(r[0], r[1], r[2], r[3]);
      ctx.restore();
      break;
    }
    case "stroke":
      ctx.save();
      ctx.strokeStyle = item.color;
      if ("width" in item) ctx.lineWidth = num(item.width);
      if ("cap" in item) ctx.lineCap = item.cap;
      if ("join" in item) ctx.lineJoin = item.join;
      if ("miterLimit" in item) ctx.miterLimit = num(item.miterLimit);
      if ("dash" in item) ctx.setLineDash(item.dash.map(num));
      if ("dashOffset" in item) ctx.lineDashOffset = num(item.dashOffset);
      trace(ctx, item.path);
      ctx.stroke();
      ctx.restore();
      break;
    case "image": {
      const img = images[item.id];
      if (!img) break;
      ctx.save();
      ctx.imageSmoothingEnabled = item.smoothing !== false;
      if (item.filter === "dark") ctx.filter = "invert(93%) hue-rotate(180deg)";
      else if (item.filter) throw new Error(`unknown filter ${item.filter}`);
      const d = item.dest.map(num);
      if (item.source) {
        const s = item.source.map(num);
        ctx.drawImage(img, s[0], s[1], s[2], s[3], d[0], d[1], d[2], d[3]);
      } else {
        ctx.drawImage(img, d[0], d[1], d[2], d[3]);
      }
      ctx.restore();
      break;
    }
    case "group":
      ctx.save();
      if (item.transform) {
        const t = item.transform.map(num);
        ctx.transform(t[0], t[1], t[2], t[3], t[4], t[5]);
      }
      if ("opacity" in item) ctx.globalAlpha = ctx.globalAlpha * num(item.opacity);
      if (item.clip) {
        trace(ctx, item.clip.path);
        ctx.clip(item.clip.rule || "nonzero");
      }
      for (const child of item.items) draw(ctx, child, images);
      ctx.restore();
      break;
    default:
      throw new Error(`unknown item type ${item.type}`);
  }
}

// A recorded canvas call: ["set", property, value], or a method and its
// arguments, with drawImage's image as {file: id} or {placeholder: kind}.
function call(ctx, c, images, placeholders) {
  const [name, ...args] = c;
  if (name === "set") {
    ctx[args[0]] = typeof args[1] === "string" ? args[1] : num(args[1]);
    return;
  }
  if (name === "drawImage") {
    const [ref, ...n] = args;
    const img = "file" in ref ? images[ref.file] : placeholders[ref.placeholder];
    if (!img) throw new Error(`no image ${JSON.stringify(ref)}`);
    ctx.drawImage(img, ...n.map(num));
    return;
  }
  if (typeof ctx[name] !== "function") throw new Error(`unknown canvas call ${name}`);
  ctx[name](...args.map(num));
}

// Data URL images, created while the page loads so the load event waits
// for them.
function loadImage(url) {
  const img = document.createElement("img");
  img.style.display = "none";
  img.src = url;
  document.body.appendChild(img);
  return img;
}

function bitmap(spec) {
  if ("dataUrl" in spec) return loadImage(spec.dataUrl);
  const c = document.createElement("canvas");
  c.width = spec.width;
  c.height = spec.height;
  const data = new ImageData(new Uint8ClampedArray(spec.rgba), spec.width, spec.height);
  c.getContext("2d").putImageData(data, 0, 0);
  return c;
}
