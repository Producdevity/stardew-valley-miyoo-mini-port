import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

import { readReleaseMetadata } from "./release-metadata.mjs";

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const projectRoot = path.resolve(appRoot, "..");
const metadata = await readReleaseMetadata();
const source = process.env.SVMM_RELEASE_ARCHIVE
  ? path.resolve(process.env.SVMM_RELEASE_ARCHIVE)
  : path.join(projectRoot, "releases", `v${metadata.version}`, metadata.archive);
const data = await readFile(source).catch(() => {
  throw new Error(`Release archive not found: ${source}`);
});
const actual = createHash("sha256").update(data).digest("hex");
if (actual !== metadata.sha256) {
  throw new Error(`Release archive failed its integrity check: ${source}`);
}
const destination = path.join(appRoot, "src-tauri", "resources", "release-kit.tar.gz");
await mkdir(path.dirname(destination), { recursive: true });
await copyFile(source, destination);
console.log(`Staged release ${metadata.version}: ${destination}`);
