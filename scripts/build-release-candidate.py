#!/usr/bin/env python3
"""Assemble the tested runtime repair without importing unrelated native changes."""
import argparse
import gzip
import hashlib
import json
from pathlib import Path
import re
import shutil
import tarfile
import tempfile

PROJECT = Path(__file__).resolve().parent.parent
PRODUCT = "Stardew Valley for Miyoo Mini"


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def require_hash(path, expected):
    if not path.is_file() or sha256(path) != expected:
        raise ValueError(f"Missing or unaccepted build input: {path}")


def normalized(info):
    if info.name.endswith(".DS_Store") or Path(info.name).name.startswith("._"):
        return None
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    info.mtime = 0
    info.pax_headers = {}
    return info


def build(baseline, framework, verifier, output, project=PROJECT):
    contract = json.loads((project / "release-tools/runtime-contract.json").read_text())
    require_hash(baseline, contract["baseline_archive_sha256"])
    require_hash(framework, contract["framework_sha256"])
    require_hash(verifier, contract["verifier_sha256"])
    version = (project / "VERSION").read_text().strip()
    if not re.fullmatch(r"\d+\.\d+\.\d+", version):
        raise ValueError("VERSION must contain a numeric release version")
    root_name = f"stardew-valley-miyoo-mini-v{version}"
    output.mkdir(parents=True, exist_ok=True)
    destination = output / root_name
    archive = output / (root_name + ".tar.gz")
    if destination.exists() or archive.exists():
        raise ValueError("Candidate output already exists; select a new output directory")
    aot_script = (project / "release-tools/scripts/compile-aot.sh").read_text()
    for setting, key in [("expected_framework", "framework_sha256"),
                         ("expected_text", "serializer_text_sha256"),
                         ("expected_rodata", "serializer_rodata_sha256")]:
        if f"{setting}={contract[key]}" not in aot_script.splitlines():
            raise ValueError(f"AOT script differs from runtime contract: {setting}")

    with tempfile.TemporaryDirectory(prefix="stardew-candidate-", dir=output) as temporary:
        staging = Path(temporary)
        with tarfile.open(baseline, "r:gz") as source:
            if {Path(member.name).parts[0] for member in source.getmembers()} != {
                contract["baseline_archive_root"]
            }:
                raise ValueError("Unexpected baseline archive layout")
            source.extractall(staging, filter="data")
        kit = staging / contract["baseline_archive_root"]
        game = kit / "runtime-template/Roms/PORTS/Games" / PRODUCT
        shutil.copyfile(framework, game / "dlls/MonoGame.Framework.dll")
        shutil.copyfile(verifier, kit / "tools/managed/PackageVerifier.exe")
        shutil.copytree(project / "release-tools/scripts", kit / "scripts", dirs_exist_ok=True)
        shutil.copyfile(project / "release-tools/prepare.sh", kit / "prepare.sh")
        (kit / "VERSION").write_text(version + "\n")
        (game / "VERSION").write_text(version + "\n")
        for folder in [game / "gamedata", game / "savedata"]:
            if any(folder.iterdir()):
                raise ValueError(f"Candidate contains game or save data: {folder}")
        if any(p.is_file() and p.name != "PLACE_GAME_FILES_HERE.txt"
               for p in (kit / "gamefiles").rglob("*")):
            raise ValueError("Candidate gamefiles directory is not empty")
        shutil.move(str(kit), destination)

    partial = archive.with_suffix(archive.suffix + ".tmp")
    try:
        with partial.open("wb") as raw:
            with gzip.GzipFile(filename="", fileobj=raw, mode="wb", mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w", format=tarfile.PAX_FORMAT) as result:
                    result.add(destination, arcname=root_name, filter=normalized)
        partial.replace(archive)
    finally:
        partial.unlink(missing_ok=True)
    checksum = sha256(archive)
    Path(str(archive) + ".sha256").write_text(f"{checksum}  {archive.name}\n")
    receipt = {"version": version, "archive": str(archive), "sha256": checksum,
               "baseline_sha256": contract["baseline_archive_sha256"],
               "framework_sha256": contract["framework_sha256"],
               "verifier_sha256": sha256(verifier), "published": False}
    (output / "build-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps(receipt, indent=2))
    return archive


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--framework", type=Path, required=True)
    parser.add_argument("--verifier", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    options = parser.parse_args()
    build(options.baseline.resolve(), options.framework.resolve(), options.verifier.resolve(),
          options.output.resolve())
