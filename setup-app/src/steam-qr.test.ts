import assert from "node:assert/strict";
import test from "node:test";
import { steamQrMatrix } from "./steam-qr.ts";

test("preserves the QR quiet zone", () => {
  const light = "  ";
  const dark = "██";
  const lines = Array.from({ length: 37 }, (_, y) =>
    Array.from({ length: 37 }, (_, x) =>
      x >= 4 && x < 33 && y >= 4 && y < 33 && (x + y) % 3 === 0
        ? dark
        : light,
    ).join(""),
  );

  const matrix = steamQrMatrix(lines);
  assert.equal(matrix.length, 37);
  assert.ok(matrix.every((row) => row.length === 37));
  assert.ok(matrix.slice(0, 4).every((row) => row.every((module) => !module)));
  assert.ok(matrix.every((row) => row.slice(0, 4).every((module) => !module)));
  assert.ok(matrix.slice(4, 33).some((row) => row.some(Boolean)));
});
