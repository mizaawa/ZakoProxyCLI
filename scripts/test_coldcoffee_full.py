"""Regression coverage for offline imports, source integrity and API adaptation."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("coldcoffee_full", ROOT / "scripts/import-coldcoffee-full.py")
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ColdcoffeeFullTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        for name in ["third_party/coldcoffee-full", "profiles/coldcoffee-full"]:
            shutil.copytree(ROOT / name, self.root / name)
        self.target = self.root / "profiles/coldcoffee-full"
        self.vendor = self.root / "third_party/coldcoffee-full"

    def run_import(self, check=True):
        with contextlib.redirect_stdout(io.StringIO()):
            MODULE.import_profiles(self.root, check=check)

    def test_offline_check_preserves_files_and_modification_times(self):
        paths = [p for p in self.root.rglob("*") if p.is_file()]
        before = {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in paths}
        self.run_import()
        self.assertEqual(before, {p: (p.read_bytes(), p.stat().st_mtime_ns) for p in paths})

    def test_regeneration_repairs_output_without_replacing_legacy(self):
        legacy = self.target / "legacy.json"
        before = (legacy.read_bytes(), legacy.stat().st_mtime_ns)
        generated = self.target / "kimi.md"
        expected = generated.read_bytes()
        generated.write_bytes(b"broken")
        with self.assertRaisesRegex(ValueError, "Generated file differs"):
            self.run_import()
        self.run_import(check=False)
        self.assertEqual(generated.read_bytes(), expected)
        self.assertEqual((legacy.read_bytes(), legacy.stat().st_mtime_ns), before)
        self.run_import()

    def test_modified_source_is_rejected_before_any_output_write(self):
        source = self.vendor / "desktop/src/lib/packs/kimi.md"
        source.write_bytes(source.read_bytes() + b"\nchanged\n")
        before = {p: p.read_bytes() for p in self.target.iterdir() if p.is_file()}
        with self.assertRaisesRegex(ValueError, "Bundled upstream hash mismatch"):
            self.run_import(check=False)
        self.assertEqual(before, {p: p.read_bytes() for p in self.target.iterdir() if p.is_file()})

    def test_missing_and_unlisted_source_files_are_rejected(self):
        extra = self.vendor / "unexpected.md"
        extra.write_text("unexpected", encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "file inventory differs"):
            self.run_import()
        extra.unlink()
        (self.vendor / "desktop/src/lib/packs/local-agent-skills/meta-muse.md").unlink()
        with self.assertRaisesRegex(ValueError, "file inventory differs"):
            self.run_import()

    def test_historical_snapshot_cannot_be_silently_regenerated(self):
        legacy = self.target / "legacy.json"
        changed = legacy.read_bytes() + b"\n"
        legacy.write_bytes(changed)
        with self.assertRaisesRegex(ValueError, "Historical profile snapshot hash mismatch"):
            self.run_import(check=False)
        self.assertEqual(legacy.read_bytes(), changed)

    def test_historical_snapshot_hash_is_stable_across_line_endings(self):
        legacy = self.target / "legacy.json"
        content = legacy.read_bytes().replace(b"\r\n", b"\n")
        legacy.write_bytes(content.replace(b"\n", b"\r\n"))
        self.run_import()

    def test_upstream_revision_and_unknown_placeholders_fail_closed(self):
        inputs = MODULE.load_inputs(self.root)
        for template_id, path in [
            ("codex", "codex.md"), ("meta-muse", "local-agent-skills/meta-muse.md"),
        ]:
            changed = dict(inputs)
            changed[MODULE.PACKS + path] += b"\n{{unknown_placeholder}}\n"
            with self.assertRaisesRegex(ValueError, "Unresolved upstream template"):
                MODULE.render(changed, template_id)
        source = self.target / "SOURCE.json"
        data = json.loads(source.read_text(encoding="utf-8"))
        data["revision"] = "0" * 40
        source.write_text(json.dumps(data), encoding="utf-8")
        with self.assertRaisesRegex(ValueError, "identity does not match"):
            self.run_import()

    def test_catalog_contains_templates_without_invented_model_ids(self):
        templates = json.loads((self.target / "templates.json").read_text(encoding="utf-8"))
        self.assertEqual({item["id"] for item in templates}, set(MODULE.SEATS) | set(MODULE.VARIANTS))
        self.assertEqual(len(templates), 15)
        for template in templates:
            self.assertEqual(set(template), {"id", "name", "source_path"})
            self.assertTrue((self.vendor / template["source_path"]).is_file())
            text = (self.target / f"{template['id']}.md").read_text(encoding="utf-8")
            self.assertIn("QQ四群 1131239659", text)
            self.assertNotIn("{{", text)
        self.assertEqual((self.target / "codex.md").read_bytes(), (ROOT / "profiles/coldcoffee-gpt/system.md").read_bytes())

    def test_installer_seats_use_request_guidance_and_keep_source_unchanged(self):
        for template_id in MODULE.INSTALLER_SEATS:
            original = (self.vendor / MODULE.PACKS / f"{template_id}.md").read_text(encoding="utf-8")
            generated = (self.target / f"{template_id}.md").read_text(encoding="utf-8")
            self.assertTrue(original.startswith("---\n"))
            self.assertIn("安装", original)
            self.assertIn("## API 交付三拍", generated)
            for removed in ["%USERPROFILE%", "%LOCALAPPDATA%", "安装位置只有", "安装两处", "安装只放", "规则文件：", "技能文件："]:
                self.assertNotIn(removed, generated)
        # Short upstream variants do not accidentally grow a full route bundle.
        for template_id in MODULE.VARIANTS:
            generated = (self.target / f"{template_id}.md").read_text(encoding="utf-8")
            self.assertIn("后续任务按以下顺序推进", generated)
            self.assertNotIn("CHA-ROUTE", generated)


if __name__ == "__main__":
    unittest.main()
