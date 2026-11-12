"""Regression checks for reproducible GPT imports and source integrity."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("coldcoffee_gpt", ROOT / "scripts/import-coldcoffee-gpt.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ColdcoffeeGptTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        for name in ["third_party/coldcoffee-gpt", "profiles/coldcoffee-gpt"]:
            shutil.copytree(ROOT / name, self.root / name)
        shutil.copyfile(ROOT / "profiles/index.json", self.root / "profiles/index.json")

    def check_import(self):
        with contextlib.redirect_stdout(io.StringIO()):
            MODULE.import_profiles(self.root, check=True)

    def test_offline_check_is_read_only(self):
        paths = [p for p in self.root.rglob("*") if p.is_file()]
        before = {p: p.read_bytes() for p in paths}
        self.check_import()
        self.assertEqual(before, {p: p.read_bytes() for p in paths})

    def test_modified_source_is_rejected(self):
        path = self.root / "third_party/coldcoffee-gpt/desktop/src/lib/packs/codex.md"
        path.write_bytes(path.read_bytes() + b"\nchanged\n")
        with self.assertRaisesRegex(ValueError, "Bundled upstream hash mismatch"):
            self.check_import()

    def test_modified_catalog_and_identity_are_rejected(self):
        path = self.root / "profiles/index.json"
        path.write_bytes(path.read_bytes() + b"\n")
        with self.assertRaisesRegex(ValueError, "Generated file differs"):
            self.check_import()
        source = self.root / "profiles/coldcoffee-gpt/SOURCE.json"
        data = json.loads(source.read_text(encoding="utf-8"))
        data["revision"] = "0" * 40
        source.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "identity does not match"):
            self.check_import()

    def test_unknown_template_fails_instead_of_shipping_placeholder(self):
        source = json.loads((self.root / "profiles/coldcoffee-gpt/SOURCE.json").read_text(encoding="utf-8"))
        inputs = {name: (self.root / "third_party/coldcoffee-gpt" / name).read_bytes()
                  for name in source["inputs"]}
        inputs[MODULE.PACKS + "codex.md"] += b"\n{{UNKNOWN_TEMPLATE}}\n"
        with self.assertRaisesRegex(ValueError, "Unresolved upstream GPT template"):
            MODULE.render(inputs)


if __name__ == "__main__":
    unittest.main()
