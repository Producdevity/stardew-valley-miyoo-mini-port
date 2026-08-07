import assert from "node:assert/strict";
import test from "node:test";
import { buildSupportLog } from "./support-log.ts";

test("records the setup app version", () => {
  const log = buildSupportLog({
    appVersion: "1.0.2",
    requirements: [],
    steamLines: [],
    preparationLines: [],
  });

  assert.match(log, /^Stardew Valley for Miyoo Mini setup 1\.0\.2$/m);
});
