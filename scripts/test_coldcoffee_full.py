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
        self.assertEqual(len(templates), 16)
        for template in templates:
            self.assertEqual(set(template), {"id", "name", "source_path"})
            self.assertTrue((self.vendor / template["source_path"]).is_file())
            text = (self.target / f"{template['id']}.md").read_text(encoding="utf-8")
            self.assertIn("QQ四群 1131239659", text)
            self.assertNotIn("{{", text)
        self.assertEqual((self.target / "codex.md").read_bytes(), (ROOT / "profiles/coldcoffee-gpt/system.md").read_bytes())

    def test_gpt_62_presets_share_the_upstream_codex_chain(self):
        inputs = MODULE.load_inputs(self.root)
        self.assertEqual(MODULE.codex_label(inputs), "GPT-6.2 Astra / Sol")
        templates = json.loads((self.target / "templates.json").read_text(encoding="utf-8"))
        codex = next(item for item in templates if item["id"] == "codex")
        self.assertEqual(codex["name"], "冷咖啡 · GPT-6.2 Astra / Sol")
        models = json.loads((self.target / "models.json").read_text(encoding="utf-8"))
        gpt_models = [item for item in models if item["model_family"] == "gpt-6.2"]
        self.assertEqual([item["models"] for item in gpt_models], [["gpt-6.2-astra"], ["gpt-6.2-sol"]])
        for item in gpt_models:
            self.assertEqual(item["id"], f"coldcoffee-{item['models'][0]}-v1")
            self.assertEqual(item["model_family"], "gpt-6.2")
            self.assertNotIn("instruction", item)
        source = json.loads((self.target / "SOURCE.json").read_text(encoding="utf-8"))
        data = (self.target / "models.json").read_bytes()
        self.assertEqual(source["outputs"]["models.json"], {"bytes": len(data), "sha256": MODULE.sha256(data)})

    def test_codex_label_follows_metadata_and_rejects_missing_or_ambiguous_seats(self):
        inputs = MODULE.load_inputs(self.root)
        name = "desktop/src/shared/workbench-core.js"
        original = inputs[name]
        inputs[name] = original.replace(b"GPT-6.2 Astra / Sol", b"Updated Codex label")
        self.assertEqual(MODULE.codex_label(inputs), "Updated Codex label")
        for replacement in [b"const SEATS = [];", b"const SEATS = [{id:'codex',tag:'A'}, {id:'codex',tag:'B'}];"]:
            inputs[name] = replacement
            with self.assertRaisesRegex(ValueError, "(Missing upstream Codex|Invalid or duplicate upstream seat)"):
                MODULE.codex_label(inputs)

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
            self.assertIn("后续按用户的任务目标继续执行" if template_id == "opencode" else "后续任务按以下顺序推进", generated)
            self.assertNotIn("CHA-ROUTE", generated)


    def test_all_labels_follow_current_upstream_and_old_template_ids_stay_valid(self):
        labels = MODULE.seat_labels(MODULE.load_inputs(self.root))
        expected = {
            "codex": "GPT-6.2 Astra / Sol", "claude": "Claude Fable 5.2 / 全模型支持",
            "glm53": "GLM 5.4全模型支持", "gemini": "Gemini 4 全模型支持",
            "kimi": "Kimi K3.1 Pro", "mimo": "MiniMax M3.1 Flash", "doubao": "豆包 2.1",
        }
        self.assertEqual(set(labels), set(MODULE.SEATS))
        catalog = {item["id"]: item["name"] for item in json.loads((self.target / "templates.json").read_text(encoding="utf-8"))}
        for seat, label in expected.items():
            self.assertEqual(labels[seat], label)
            self.assertEqual(catalog[seat], f"冷咖啡 · {label}")
            self.assertIn(label, (self.target / f"{seat}.md").read_text(encoding="utf-8"))

    def test_new_exact_presets_and_display_only_templates_are_distinct(self):
        models = json.loads((self.target / "models.json").read_text(encoding="utf-8"))
        self.assertEqual(len(models), 9)
        self.assertEqual({item["models"][0] for item in models}, {
            "gpt-6.2-astra", "gpt-6.2-sol", "glm-5.4", "claude-fable-5.2",
            "gemini-4", "gemini-4-carbon", "gemini-4-neon", "gemini-4-hydrogen", "gemini-4-argon",
        })
        for item in models:
            self.assertEqual(item["id"], f"coldcoffee-{item['models'][0]}-v1")
            self.assertEqual(len(item["models"]), 1)
            self.assertNotIn("instruction", item)
        relay = MODULE.load_inputs(self.root)["desktop/src/shared/relay-models.js"].decode("utf-8")
        for signature in ("gpt62astra", "gpt62sol", "glm54", "claudefable52", "^gemini4(?:carbon|neon|hydrogen|argon)?$"):
            self.assertIn(signature, relay)

    def test_codex_regeneration_tracks_metadata_instead_of_freezing_old_payload(self):
        inputs = MODULE.load_inputs(self.root)
        name = "desktop/src/shared/workbench-core.js"
        inputs[name] = inputs[name].replace(b"GPT-6.2 Astra / Sol", b"Future upstream Codex")
        output = MODULE.render(inputs, "codex").decode("utf-8")
        self.assertIn("# seat: Future upstream Codex / Codex CLI", output)
        self.assertNotIn("# seat: GPT-6 Astra", output)
        self.assertIn("CHA-ROUTE", output)
        self.assertEqual(output.count(" · LEAF-"), 80)

    def test_opencode_is_api_text_without_installing_or_claiming_a_new_model(self):
        output = (self.target / "opencode.md").read_text(encoding="utf-8")
        self.assertIn("冷咖啡 · OpenCode 工作流", output)
        self.assertIn("{{ACTIVATION_REPLY}}", (self.vendor / MODULE.PACKS / "local-agent-skills/opencode.md").read_text(encoding="utf-8"))
        self.assertNotIn("{{", output)
        self.assertNotIn("这是随冷咖啡工作台安装", output)
        self.assertNotIn("name: coldcoffee-opencode", output)
        self.assertIn("不安装本地技能", output)

if __name__ == "__main__":
    unittest.main()
