"""Check all independent strategy sources, not only the Coldcoffee catalog."""

import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[1]
SOURCES = {
    "hanshuang": ("import_profile", "execution-excerpts.json", "system.md"),
    "kimi-break": ("import_profiles", "SYSTEM.md", "system.md"),
    "ai-jailbreak": ("generate", "glm-5.3-identity-frame.txt", "glm.md"),
}


def importer(source):
    spec = importlib.util.spec_from_file_location(source, ROOT / f"scripts/import-{source}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return getattr(module, SOURCES[source][0])


def snapshot(root):
    return {p.relative_to(root): (p.read_bytes(), p.stat().st_mtime_ns)
            for p in root.rglob("*") if p.is_file()}


class SourceProfilesTests(unittest.TestCase):
    def fixture(self, source):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        for parent in ["profiles", "third_party"]:
            shutil.copytree(ROOT / parent / source, root / parent / source)
        return root

    def run_import(self, source, root, check=True):
        with contextlib.redirect_stdout(io.StringIO()):
            importer(source)(root, check=check)

    def test_each_source_reproduces_offline_without_modifying_files(self):
        for source in SOURCES:
            with self.subTest(source=source):
                root = self.fixture(source)
                before = snapshot(root)
                self.run_import(source, root)
                self.assertEqual(before, snapshot(root))

    def test_changed_source_is_rejected_before_writing_outputs(self):
        for source, (_, filename, _) in SOURCES.items():
            with self.subTest(source=source):
                root = self.fixture(source)
                path = root / "third_party" / source / filename
                path.write_bytes(path.read_bytes() + b"\nchanged\n")
                before = snapshot(root / "profiles")
                with self.assertRaises(ValueError):
                    self.run_import(source, root, check=False)
                self.assertEqual(before, snapshot(root / "profiles"))

    def test_each_source_can_repair_a_missing_or_changed_body(self):
        for source, (_, _, filename) in SOURCES.items():
            with self.subTest(source=source):
                root = self.fixture(source)
                path = root / "profiles" / source / filename
                expected = path.read_bytes()
                path.write_bytes(b"not a strategy\n")
                with self.assertRaises(ValueError):
                    self.run_import(source, root)
                self.run_import(source, root, check=False)
                self.assertEqual(expected, path.read_bytes())
                path.unlink()
                self.run_import(source, root, check=False)
                self.assertEqual(expected, path.read_bytes())

    def assert_digest(self, path, metadata):
        data = path.read_bytes()
        self.assertEqual(len(data), metadata["bytes"], str(path))
        self.assertEqual(hashlib.sha256(data).hexdigest(), metadata["sha256"], str(path))

    def test_all_declared_manifests_exist(self):
        manifest = json.loads((ROOT / "SOURCE.json").read_text(encoding="utf-8"))
        version = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["package"]["version"]
        self.assertEqual(manifest["version"], version)
        self.assertEqual(len(manifest["strategy_manifests"]), 7)
        for filename in manifest["strategy_manifests"]:
            with self.subTest(filename=filename):
                self.assertTrue(json.loads((ROOT / filename).read_text(encoding="utf-8")))

    def test_runtime_check_rejects_missing_or_substituted_bodies(self):
        spec = importlib.util.spec_from_file_location("runtime_profiles", ROOT / "scripts/check-runtime-profiles.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        expected = module.expected_profiles(ROOT)
        rows = [{"id": key, "models": item["models"], "instruction": item["instruction"],
                 "metadata": {"source_url": item["repository"], "validation": "unverified"}}
                for key, item in expected.items()]
        self.assertEqual(module.check(ROOT, rows), 51)
        result = subprocess.run(
            [sys.executable, str(ROOT / "scripts/check-runtime-profiles.py")],
            input=json.dumps(rows, ensure_ascii=False).encode("utf-8"),
            capture_output=True, timeout=30,
            env={**os.environ, "PYTHONIOENCODING": "ascii"},
        )
        self.assertEqual(result.returncode, 0, result.stderr.decode("utf-8", errors="replace"))
        with self.assertRaisesRegex(ValueError, "count differs"):
            module.check(ROOT, rows[:-1])
        for row in rows:
            with self.subTest(profile=row["id"]):
                original = row["instruction"]
                row["instruction"] = ""
                with self.assertRaisesRegex(ValueError, "instruction differs"):
                    module.check(ROOT, rows)
                row["instruction"] = original

    def test_gpt_default_and_deepseek_adapter_hashes(self):
        vendor = ROOT / "third_party/gpt-instruct"
        manifest = json.loads((vendor / "SOURCE.json").read_text(encoding="utf-8"))
        self.assertIsNone(manifest["revision"])
        for filename, metadata in manifest["files"].items():
            self.assert_digest(vendor / filename, metadata)
        entries = json.loads((vendor / "profiles.legacy.json").read_text(encoding="utf-8"))
        self.assertEqual({p["id"] for p in entries}, set(manifest["profile_ids"]))
        default = json.loads((ROOT / "profiles/default.SOURCE.json").read_text(encoding="utf-8"))
        original = next(p for p in entries if p["id"] == default["source_profile"])
        self.assertEqual(hashlib.sha256(original["instruction"].encode()).hexdigest(),
                         default["source_instruction_sha256"])
        self.assert_digest(ROOT / default["output"]["file"], default["output"])
        adapter = json.loads((ROOT / "profiles/dsh-purge/SOURCE.json").read_text(encoding="utf-8"))
        for filename, metadata in adapter["outputs"].items():
            self.assert_digest(ROOT / "profiles/dsh-purge" / filename, metadata)


if __name__ == "__main__":
    unittest.main()
