#!/usr/bin/env python3
"""Exercise candidate reproducibility, input binding and game-data exclusion."""
import importlib.util
import json
from pathlib import Path
import tarfile
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "candidate_builder", Path(__file__).with_name("build-release-candidate.py"))
BUILDER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(BUILDER)


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.work = tempfile.TemporaryDirectory()
        self.addCleanup(self.work.cleanup)
        self.root = Path(self.work.name)
        self.project = self.root / "project"
        (self.project / "release-tools/scripts").mkdir(parents=True)
        (self.project / "VERSION").write_text("1.2.1\n")
        (self.project / "release-tools/prepare.sh").write_text("#!/bin/sh\n")
        self.framework = self.root / "framework.dll"
        self.framework.write_bytes(b"tested framework")
        self.verifier = self.root / "verifier.exe"
        self.verifier.write_bytes(b"compiled verifier")
        self.kit = self.root / "baseline-kit"
        self.game = self.kit / "runtime-template/Roms/PORTS/Games" / BUILDER.PRODUCT
        for folder in [self.game / "dlls", self.game / "gamedata", self.game / "savedata",
                       self.kit / "gamefiles", self.kit / "tools/managed", self.kit / "scripts"]:
            folder.mkdir(parents=True)
        (self.game / "dlls/MonoGame.Framework.dll").write_bytes(b"old framework")
        (self.kit / "VERSION").write_text("1.2.0\n")
        self.archive = self.root / "baseline.tar.gz"
        self.contract = {"baseline_archive_root": "baseline-kit",
                         "framework_sha256": BUILDER.sha256(self.framework),
                         "serializer_text_sha256": "a" * 64,
                         "serializer_rodata_sha256": "b" * 64}
        self.script = self.project / "release-tools/scripts/compile-aot.sh"
        self.script.write_text(
            "expected_framework=" + self.contract["framework_sha256"] + "\n" +
            "expected_text=" + self.contract["serializer_text_sha256"] + "\n" +
            "expected_rodata=" + self.contract["serializer_rodata_sha256"] + "\n")
        self.repack()

    def repack(self):
        with tarfile.open(self.archive, "w:gz") as output:
            output.add(self.kit, arcname="baseline-kit")
        self.contract["baseline_archive_sha256"] = BUILDER.sha256(self.archive)
        (self.project / "release-tools/runtime-contract.json").write_text(json.dumps(self.contract))

    def build(self, name):
        return BUILDER.build(self.archive, self.framework, self.verifier,
                             self.root / name, self.project)

    def test_reproducible_archive_contains_repair(self):
        a, b = self.build("a"), self.build("b")
        self.assertEqual(BUILDER.sha256(a), BUILDER.sha256(b))
        with tarfile.open(a) as archive:
            path = "stardew-valley-miyoo-mini-v1.2.1/runtime-template/Roms/PORTS/Games/" + BUILDER.PRODUCT
            self.assertEqual(archive.extractfile(path + "/dlls/MonoGame.Framework.dll").read(),
                             self.framework.read_bytes())
        self.assertEqual((self.root / "a/stardew-valley-miyoo-mini-v1.2.1/VERSION").read_text(), "1.2.1\n")

    def test_rejects_unverified_framework(self):
        self.framework.write_bytes(b"unverified framework")
        with self.assertRaisesRegex(ValueError, "unaccepted build input"):
            self.build("rejected")

    def test_rejects_stale_aot_contract(self):
        self.script.write_text("expected_framework=old\n")
        with self.assertRaisesRegex(ValueError, "AOT script differs"):
            self.build("rejected")

    def test_rejects_gamefiles_and_saves(self):
        for relative in ["gamedata/Stardew Valley.exe", "savedata/player-save"]:
            with self.subTest(relative=relative):
                path = self.game / relative
                path.write_bytes(b"user-owned data")
                self.repack()
                with self.assertRaisesRegex(ValueError, "game or save data"):
                    self.build(relative.split("/")[0])
                path.unlink()


if __name__ == "__main__":
    unittest.main()
