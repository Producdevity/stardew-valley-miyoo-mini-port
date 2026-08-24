import assert from "node:assert/strict";
import test from "node:test";

import { validateReleaseMetadata } from "./release-metadata.mjs";

const valid = {
  version: "1.2.3",
  packageVersion: "1.2.3",
  cargoVersion: "1.2.3",
  tauriVersion: "../package.json",
  releaseKit: {
    version: "1.2.3",
    archive: "stardew-valley-miyoo-mini-v1.2.3.tar.gz",
    root: "stardew-valley-miyoo-mini-v1.2.3",
    sha256: "a".repeat(64),
  },
};

test("returns release names from consistent metadata", () => {
  assert.deepEqual(validateReleaseMetadata(valid), {
    version: "1.2.3",
    tag: "v1.2.3",
    archive: "stardew-valley-miyoo-mini-v1.2.3.tar.gz",
    root: "stardew-valley-miyoo-mini-v1.2.3",
    sha256: "a".repeat(64),
  });
});

test("rejects a version mismatch", () => {
  assert.throws(
    () => validateReleaseMetadata({ ...valid, cargoVersion: "9.9.9" }),
    /Cargo\.toml has version 9\.9\.9; expected 1\.2\.3/,
  );
});

test("rejects an archive name that does not match the version", () => {
  assert.throws(
    () =>
      validateReleaseMetadata({
        ...valid,
        releaseKit: { ...valid.releaseKit, archive: "old-release.tar.gz" },
      }),
    /release-kit\.json has archive old-release\.tar\.gz/,
  );
});
