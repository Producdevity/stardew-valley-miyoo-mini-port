import { appendFile, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const appRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const projectRoot = path.resolve(appRoot, "..");

function cargoPackageVersion(source) {
  let inPackage = false;
  for (const line of source.split(/\r?\n/)) {
    if (line === "[package]") {
      inPackage = true;
      continue;
    }
    if (inPackage && line.startsWith("[")) {
      break;
    }
    if (inPackage) {
      const version = line.match(/^version\s*=\s*"([^"]+)"\s*$/);
      if (version) {
        return version[1];
      }
    }
  }
  throw new Error("Cargo.toml does not contain a package version");
}

export function validateReleaseMetadata({
  version,
  packageVersion,
  cargoVersion,
  tauriVersion,
  releaseKit,
}) {
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error(`Invalid project version: ${version}`);
  }

  const versions = [
    ["package.json", packageVersion],
    ["Cargo.toml", cargoVersion],
    ["release-kit.json", releaseKit.version],
  ];
  for (const [file, value] of versions) {
    if (value !== version) {
      throw new Error(`${file} has version ${value}; expected ${version}`);
    }
  }

  if (tauriVersion !== "../package.json") {
    throw new Error("tauri.conf.json must read its version from ../package.json");
  }

  const tag = `v${version}`;
  const archive = `stardew-valley-miyoo-mini-${tag}.tar.gz`;
  const root = `stardew-valley-miyoo-mini-${tag}`;
  if (releaseKit.archive !== archive) {
    throw new Error(
      `release-kit.json has archive ${releaseKit.archive}; expected ${archive}`,
    );
  }
  if (releaseKit.root !== root) {
    throw new Error(`release-kit.json has root ${releaseKit.root}; expected ${root}`);
  }
  if (!/^[a-f0-9]{64}$/.test(releaseKit.sha256)) {
    throw new Error("release-kit.json contains an invalid SHA-256");
  }

  return { version, tag, archive, root, sha256: releaseKit.sha256 };
}

export async function readReleaseMetadata() {
  const [version, packageSource, cargoSource, tauriSource, releaseKitSource] =
    await Promise.all([
      readFile(path.join(projectRoot, "VERSION"), "utf8"),
      readFile(path.join(appRoot, "package.json"), "utf8"),
      readFile(path.join(appRoot, "src-tauri", "Cargo.toml"), "utf8"),
      readFile(path.join(appRoot, "src-tauri", "tauri.conf.json"), "utf8"),
      readFile(path.join(appRoot, "src-tauri", "release-kit.json"), "utf8"),
    ]);

  return validateReleaseMetadata({
    version: version.trim(),
    packageVersion: JSON.parse(packageSource).version,
    cargoVersion: cargoPackageVersion(cargoSource),
    tauriVersion: JSON.parse(tauriSource).version,
    releaseKit: JSON.parse(releaseKitSource),
  });
}

async function main() {
  const metadata = await readReleaseMetadata();
  if (process.argv.includes("--github-output")) {
    if (!process.env.GITHUB_OUTPUT) {
      throw new Error("GITHUB_OUTPUT is not set");
    }
    const output = Object.entries(metadata)
      .map(([key, value]) => `${key}=${value}`)
      .join("\n");
    await appendFile(process.env.GITHUB_OUTPUT, `${output}\n`);
  }
  console.log(`Release metadata checked: ${metadata.tag}`);
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
