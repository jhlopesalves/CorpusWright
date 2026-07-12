import assert from "node:assert/strict";
import {
  colourForToken,
  compositeColours,
  contrastRatio,
  deltaE76,
  parseColour,
  resolveTokenValue,
} from "../scripts/colour-contracts.mjs";

assert.deepEqual(parseColour("#abc"), { r: 170, g: 187, b: 204, a: 1 });
assert.deepEqual(parseColour("#11223380"), { r: 17, g: 34, b: 51, a: 128 / 255 });
assert.deepEqual(parseColour("rgb(10, 20, 30)"), { r: 10, g: 20, b: 30, a: 1 });
assert.deepEqual(parseColour("rgba(10, 20, 30, 0.4)"), { r: 10, g: 20, b: 30, a: 0.4 });
const percentageRgb = parseColour("rgb(100% 0% 50% / 25%)");
assert(Math.abs(percentageRgb.r - 255) < 1e-9);
assert(Math.abs(percentageRgb.b - 127.5) < 1e-9);
assert.equal(percentageRgb.g, 0);
assert.equal(percentageRgb.a, 0.25);

const declarations = new Map([
  ["--base", "#ffffff"],
  ["--alias", "var(--base)"],
  ["--translucent", "rgba(0, 0, 0, 0.5)"],
  ["--mixed", "color-mix(in srgb, var(--base) 50%, #000000 50%)"],
  ["--band", "color-mix(in srgb, #ff0000 25%, transparent)"],
]);
assert.equal(resolveTokenValue("--alias", declarations), "#ffffff");
assert.deepEqual(colourForToken("--alias", declarations), { r: 255, g: 255, b: 255, a: 1 });
assert.deepEqual(colourForToken("--mixed", declarations), { r: 127.5, g: 127.5, b: 127.5, a: 1 });
assert.deepEqual(colourForToken("--band", declarations), { r: 255, g: 0, b: 0, a: 0.25 });
assert.deepEqual(
  compositeColours(colourForToken("--translucent", declarations), colourForToken("--base", declarations)),
  { r: 127.5, g: 127.5, b: 127.5, a: 1 },
);
assert.equal(contrastRatio(parseColour("#000"), parseColour("#fff")), 21);
assert.equal(deltaE76(parseColour("#123456"), parseColour("#123456")), 0);
assert(deltaE76(parseColour("#000"), parseColour("#fff")) > 90);

const cycle = new Map([
  ["--a", "var(--b)"],
  ["--b", "var(--a)"],
]);
assert.throws(() => resolveTokenValue("--a", cycle), /alias cycle: --a -> --b -> --a/);
assert.throws(() => parseColour("not-a-colour"), /Unparseable CSS colour/);
assert.throws(() => colourForToken("--missing", declarations), /Missing CSS colour token/);
