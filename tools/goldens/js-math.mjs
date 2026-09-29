// Inputs for goldens/js-math.json: the doubles V8's Math functions return,
// for excali-math's js module (ex-009), which must return the same doubles on
// every platform.
//
// V8 computes Math.sin, cos, tan, asin, acos, atan, atan2, exp, log, log2,
// log10 and cbrt with its own copy of fdlibm (src/base/ieee754.cc), not the
// platform's libm, and Math.hypot with its own algorithm
// (src/builtins/math.tq). The platform's libm differs from it in the last bit
// on a few percent of arguments (macOS and glibc differently). fdlibm is
// compiled by clang, which contracts a * b + c into a fused multiply-add on
// arm64 (-ffp-contract=on), so arm64 V8, where these goldens are generated
// (tools/goldens/README.md), differs from x64 V8 in the last bit on about
// 0.5% of sin/cos arguments too. TRICKY holds arguments found by a
// differential search (fdlibm with and without contraction, and the
// platform's libm, over random arguments) where exactly that happens.
//
// Math.pow is the platform's pow in V8 (src/numbers/ieee754.cc, flag
// use_std_math_pow), after the special cases ES requires and x ** 2 = x * x,
// x ** 0.5 = sqrt(x + 0); only those cases and results that are exact are
// platform independent, so only they are here.
//
// Every double is written as its IEEE 754 bits (16 hex digits), so -0 and
// the last bit survive JSON; a NaN result is the string "NaN" (JS does not
// expose NaN payloads).
//   { id, fn, args: [bits, ...], result: bits | "NaN" }

import { rng } from "./math.mjs";

const view = new DataView(new ArrayBuffer(8));
export const toBits = (x) => {
  view.setFloat64(0, x);
  return view.getBigUint64(0).toString(16).padStart(16, "0");
};
export const fromBits = (h) => {
  view.setBigUint64(0, BigInt(`0x${h}`));
  return view.getFloat64(0);
};

export const JS_MATH_FUNCTIONS = {
  sin: Math.sin,
  cos: Math.cos,
  tan: Math.tan,
  asin: Math.asin,
  acos: Math.acos,
  atan: Math.atan,
  atan2: Math.atan2,
  exp: Math.exp,
  log: Math.log,
  log2: Math.log2,
  log10: Math.log10,
  cbrt: Math.cbrt,
  pow: Math.pow,
  hypot: Math.hypot,
};

// Arguments (IEEE bits) where arm64 V8 (fdlibm with fused multiply-adds)
// differs from fdlibm without them ("contraction"), or from the platform
// libm of the machine the search ran on, macOS 27 arm64 ("platform").
const TRICKY = {
  sin: [
    "c005778a4a4a9f9b 430bbe446f6433c9 4107607517e30765 c12341ece5dd99bb bfe21eb2f600087a 404ca8c0ad2409df 40f70158b78742f5 4122bfde5cedf7ab 402e71a1c2711542 412a1720bd449007",
    "c004594b3c9afc43 c004bb815876064a c03e7f9d8a3b3522 c00589c59502bce9 c3079300eb8ea996 4004946e853bdc84 40e1e96b865b48da c0110756257fd888 c04a43432a211ba4 c30bf23f8f8d32d3",
  ],
  cos: [
    "40209e099668a756 412af88d67b7594a c02cb1ffa4055a83 40403ec0b1a56b8f 3fe42b414b8571b2 c02477b2a05354e3 41069ac9f514377d c01168c87e3d0269 c2fa8f7a2432e71e 40144d14c6eee06c",
    "c1142e518a29395d 412942b4d82ccee7 c0171d564e2d9eb9 4048c21587fe22ac c1292cfb4a048822 4119c87a7673e90c bff35640238a745a c2e9751f059d0f3d c003c63896a34024 c30b86add541f593",
  ],
  tan: [
    "bfde7936e94ba032 43069c7eaf107bb1 4129f3a47cf8c354 404426c8102dcd78 c00cc0e721d6fae0 c034e8e048be2321 400dd9a69fff1006 400d15791830878f 40351304bd616362 c3033891fdbf4723",
    "c1195592e34b7205 404331bdf1b11ef8 410496d56ce472fb c2f066c2e35e0db5 40f91cdbe611926f 400d5fa24d89b5ac 3ff4b2ec5648db88 c2cb720c2ee5db8a 3fb733fa2f36b7c0 c03428499674bdde",
  ],
  asin: [
    "3fe1b8876d2fdbc0 bfe51527d6bdd431 3fc99ac704ab2fe8 3fda397dd9571ef0 bfe40d5904105fac 3fd8218c7862ed94 3fe06eb9a07316bc bfdd9e7c86b82d0c bfd1423fd930ab08 3fe1b31f48fe62ca",
    "3fe1b41f049f0664 bfeec5e59760b818 3fd71a4936b60d58 3fe0e6b4ccfa4f56 3feecd84ccc7dcc4 bfe0c5ce6c4df435 bfe048f561d423c8 bfe8ac3b601c61ae 3fe9fe198d268cb8 bfecda54ade3958b",
  ],
  acos: [
    "3fe5419df19a65b6 3fe2e0ebc128ebfe bfe3396cef23dbdc 3fe0c70b566e4b42 3fe5c1c5ad4f0758 3fd56521576c9f3c bfe297b90a05b324 bfe41d127ec1ea2a 3fe3d84211197d90 3fddbfd60ec3776c",
    "bf358f27e3e9c5ba bf26255a69449a25 bfc7289c8c0bfbf4 3f44c98fb2925004 3f37d77fb95ffe25 bfe0510dab6ffbb1 3f3a27ca9af41a4e bf3112a3dfc4900c 3f4ddccdb3ab2bbd 3f32f1ef1f150140",
  ],
  atan: [
    "3fd4e958822c0580 c00f374b1e2ca8da c01b04dc3f1f4726 bfd971c32e247546 bfdb429e903ddb92 c004787132ca88e0 bfcf10bc93795a3c 4004e5991add72fe c00f037e56eebacc 3fd9518a83551dd8",
    "4019f550b9c2810f bfe3b04276cdd1c7 c0027977b51f9bdf c01a6b2758597b22 4008ff317510c9d6 3fe4708698078ce6 bff2d5061baa5582 401387ecb186a179 401457bc76c13383 c007e8796a3013f6",
  ],
  exp: [
    "3fceaa47cc333de8 4066244584564274 bfd7346b74fb89de 400932586f32191a c01ede636d88bd98 c02b068db4c16295 bfd87a1e1d17040c c078a01385376645 c014bdef40fbaca8 c01798f621e2a6b6",
    "c007593c218bb3de 3fdf2f51b2fd9184 bfda52e7cb478c3c 402124bd713e73b6 bfeb186057616201 c01f4a80b70b9a08 3ff04fa3be178024 bfd6093bbde997f2 c084c906f6fae9cb c062fcecbb2c8799",
  ],
  log: [
    "3ff51ce7e183b706 3ff2795d750d0a1d 4013c85cf67b5e55 4065af2cee2fdba4 40737b2de83102cc 4004cd0ac52c4aa9 40146f81d01cf366 3ff44baad613ea50 3fead242a785d24c 3ff3822163381292",
    "4084c0ae213efcc7 3fd5751892368ad2 3fe71e5672dcc24a 401c0b7da87e8cc8 3fe41a9a3d046acb 400215135e71a73b 40164358d4dc412f 4013bc11b462924c 400325e336f117ff 4077dc2a50c5b9ef",
  ],
  log2: [
    "3fe7759ea01a09da 4080f2cb54e57b06 4016050523e96006 3fe5157b81b195ae 406913e14fc23ee0 3ffab29aa545adbf 3ff63fb32434438e 3fe753d5a82e9b29 401478ea26a0daf8 3ff8374b3416dbea",
  ],
  log10: [
    "40121a9ee6122eec 4074e1ba69c48744 3ff3339cc629bcc1 3ff447149fc4cc5c 40027cbdaeb61065 40046d5de45b604f 3ffb7c16d08c4e89 3ff7136bcf0fe880 4015a2726516eff0 4015b2c63b333ce9",
    "407f3d3eac8f132b 3ff88ef9156e74ae 401dc6d30f931aa0 3fe3153cf7c4475c 400a01b77b428201 401a63750c3cc208 401e86d290e7e98a 3fc17a6b3f79c296 3fea3cef9744151a 4007a3054e551221",
  ],
  cbrt: [
    "408287fe0b64c6cc 408c03be4f534a2b 408e2388c9e50b54 62943f9d622c2aa6 40865ff04a8b5691 59013b3128baf7a5 3ff59f269a1d0d1b",
    "40069253d0c39afe 408ba8665e74a889 4080284e76e170a5 787e7a76f5e9d3ea 40190b3b308cf363 407def6406bfd762 6e5aeb479cdd0b44 7281892ddd344f42 4079ce8b018cf2a3 408319a77df8ff36",
  ],
  atan2: [
    "3fa9b88fc40e2240,3fe60ce55df44f04 c089db27e8d0a2be,c0795189062bd70d 3fe5c1ac24168a14,3fe1b6bdc3e0b338 407c68cde18e03dd,408da6b79503e1b8 408ef384ba839669,bfad995add9fe010 40877aab6c94b1e2,bfe8d54cdb3f043b 3fefa94e03a9f1ec,bfeec00811c49690 408a6771cad2e589,bfeb8b717dc5da39 c0673da7acbb1da0,407fe66ba0f1c71f c087d9bb210bda8b,bfd33d49630b1e6c",
  ],
};

const trickyArgs = (fn) =>
  (TRICKY[fn] ?? []).flatMap((line) =>
    line.split(" ").map((a) => a.split(",").map(fromBits)),
  );

const SPECIAL = [0, -0, Infinity, -Infinity, NaN, Number.MIN_VALUE, -Number.MIN_VALUE, 2.2250738585072014e-308, Number.MAX_VALUE, -Number.MAX_VALUE, 1, -1];

export const jsMathCases = () => {
  const next = rng(9);
  const between = (a, b) => a + next() * (b - a);
  const signed = (scale) => (next() * 2 - 1) * scale;
  const ulps = (x, k) => {
    // x moved by k units in the last place (x finite, non-zero)
    view.setFloat64(0, x);
    view.setBigInt64(0, view.getBigInt64(0) + BigInt(k));
    return view.getFloat64(0);
  };
  const cases = [];
  const counts = new Map();
  const add = (fn, ...args) => {
    const n = counts.get(fn) ?? 0;
    counts.set(fn, n + 1);
    cases.push({ id: `${fn}/${n}`, fn, args });
  };
  const repeat = (n, fn, make) => {
    for (let i = 0; i < n; i++) add(fn, ...make());
  };

  // sin, cos, tan: every argument-reduction path of __ieee754_rem_pio2 (none
  // below pi/4, the n = +-1 special case, the medium path with one, two and
  // three iterations near multiples of pi/2, __kernel_rem_pio2 for large
  // arguments) and both kernels.
  for (const fn of ["sin", "cos", "tan"]) {
    for (const x of SPECIAL) add(fn, x);
    for (const x of [4, -4, 0.982953340331056, 2.4, Math.PI, Math.PI / 2, Math.PI / 4, 1e-9, 2 ** -28, 2 ** -27, 1e22, 2 ** 52, 2 ** 1000, 1e300]) {
      add(fn, x);
      add(fn, -x);
    }
    for (let k = 1; k <= 40; k++) {
      const x = (k * Math.PI) / 2;
      for (const d of [-1, 0, 1]) add(fn, ulps(x, d));
    }
    repeat(100, fn, () => [signed(2 * Math.PI)]);
    repeat(40, fn, () => [signed(1000)]);
    repeat(20, fn, () => [signed(1e9)]);
    repeat(20, fn, () => [signed(2 ** 1020) * next()]);
    for (const a of trickyArgs(fn)) add(fn, ...a);
  }

  // asin, acos: |x| < 2^-57 / 2^-27, below 0.5, 0.5 to 0.975, above, +-1,
  // outside [-1, 1].
  for (const fn of ["asin", "acos"]) {
    for (const x of SPECIAL) add(fn, x);
    for (const x of [1e-20, 2 ** -57, 2 ** -27, 0.5, 0.975, 0.9999999999999999, 1.0000000000000002, 2]) {
      add(fn, x);
      add(fn, -x);
    }
    repeat(120, fn, () => [signed(1)]);
    repeat(30, fn, () => [signed(0.5)]);
    repeat(30, fn, () => [(next() < 0.5 ? -1 : 1) * between(0.975, 1)]);
    for (const a of trickyArgs(fn)) add(fn, ...a);
  }

  // atan: each of the reduction intervals (7/16, 11/16, 19/16, 39/16, 2^66).
  for (const x of SPECIAL) add("atan", x);
  for (const x of [1e-10, 2 ** -27, 0.4375, 0.6875, 1.1875, 2.4375, 2 ** 66, 1e300]) {
    add("atan", x);
    add("atan", -x);
  }
  repeat(60, "atan", () => [signed(3)]);
  repeat(40, "atan", () => [signed(100)]);
  repeat(20, "atan", () => [signed(1e20)]);
  for (const a of trickyArgs("atan")) add("atan", ...a);

  // atan2: signs, zeros and infinities of both arguments, x = 1, |y/x| out
  // of range both ways, and the quadrants.
  const edges = [0, -0, 1, -1, Infinity, -Infinity, NaN, 2.5, -2.5];
  for (const y of edges) for (const x of edges) add("atan2", y, x);
  for (const [y, x] of [[1e300, 1e-300], [1e-300, 1e300], [1e-300, -1e300], [-1e-300, -1e300], [7.25, -0.001], [-14.610474032366646, -0.21460940732155498]]) {
    add("atan2", y, x);
  }
  repeat(80, "atan2", () => [signed(10), signed(10)]);
  repeat(40, "atan2", () => [signed(1000), signed(1)]);
  repeat(40, "atan2", () => [signed(1), signed(1000)]);
  for (const a of trickyArgs("atan2")) add("atan2", ...a);

  // exp: overflow and underflow thresholds, the 0.5 ln2 / 1.5 ln2 reduction
  // bounds, tiny arguments, exp(1).
  for (const x of SPECIAL) add("exp", x);
  for (const x of [709.782712893384, 709.7827128933841, -745.1332191019411, -745.1332191019412, -708.4, -730, 0.34657359027997264, 1.0397207708399179, 2 ** -28, 1e-20, 0.5, 2]) {
    add("exp", x);
    add("exp", -x);
  }
  repeat(80, "exp", () => [signed(2)]);
  repeat(40, "exp", () => [signed(50)]);
  repeat(40, "exp", () => [signed(740)]);
  for (const a of trickyArgs("exp")) add("exp", ...a);

  // log, log2, log10: zero, negatives, subnormals, 1 and its neighbours
  // (|f| < 2^-20), sqrt(2) either side, powers of two and ten.
  for (const fn of ["log", "log2", "log10"]) {
    for (const x of SPECIAL) add(fn, x);
    for (const x of [1 + 2 ** -21, 1 - 2 ** -22, 1.0000000000000002, 0.9999999999999999, Math.SQRT2, Math.SQRT1_2, 2, 1024, 2 ** -1074 * 3, 10, 100, 1e22, 1e-300, 0.1]) {
      add(fn, x);
    }
    repeat(60, fn, () => [between(0, 4)]);
    repeat(40, fn, () => [between(0, 1e4)]);
    repeat(40, fn, () => [Math.exp(signed(700))]);
    repeat(10, fn, () => [between(0, 1) * 2 ** -1030]);
    for (const a of trickyArgs(fn)) add(fn, ...a);
  }

  // cbrt: zero, subnormals, perfect cubes, both signs.
  for (const x of SPECIAL) add("cbrt", x);
  for (const x of [8, 27, 1e-310, 3, 1e300, 0.001]) {
    add("cbrt", x);
    add("cbrt", -x);
  }
  repeat(60, "cbrt", () => [signed(10)]);
  repeat(40, "cbrt", () => [signed(1e6)]);
  repeat(20, "cbrt", () => [Math.exp(signed(700))]);
  for (const a of trickyArgs("cbrt")) add("cbrt", ...a);

  // pow: only the platform-independent answers (see the header): NaN and
  // infinity rules, x ** 2, x ** 0.5, and exact results.
  for (const x of [...SPECIAL, 2, -2, 0.5, -0.5]) {
    for (const y of [NaN, Infinity, -Infinity, 0, -0, 1, -1, 2, 0.5, 3, -3]) add("pow", x, y);
  }
  for (let k = 0; k <= 22; k++) add("pow", 10, k);
  for (let k = -20; k <= 20; k++) add("pow", 2, k);
  for (const x of [3, -3, 7, 12, 0.25, -0.125]) for (const y of [3, 4, 5]) add("pow", x, y);
  repeat(40, "pow", () => [signed(1000), 2]);
  repeat(20, "pow", () => [between(0, 1000), 0.5]);

  // hypot (V8's own algorithm, src/builtins/math.tq): scaling, NaN with an
  // infinity, zeros.
  for (const a of [[3, 4], [0, 0], [-0, -0], [Infinity, NaN], [NaN, -Infinity], [NaN, 1], [1e300, 1e300], [1e-300, 1e-300], [5e-324, 5e-324]]) {
    add("hypot", ...a);
  }
  repeat(60, "hypot", () => [signed(100), signed(100)]);
  repeat(20, "hypot", () => [signed(1e200), signed(1e-200)]);
  return cases;
};

export const jsMathResult = (c) => {
  const result = JS_MATH_FUNCTIONS[c.fn](...c.args);
  return {
    id: c.id,
    fn: c.fn,
    args: c.args.map(toBits),
    result: Number.isNaN(result) ? "NaN" : toBits(result),
  };
};
