import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RelayCatalogTests(unittest.TestCase):
    def test_all_model_and_template_bindings_are_preserved(self):
        catalog = json.loads((ROOT / 'src/relay-catalog.json').read_text(encoding='utf-8'))
        self.assertEqual(set(catalog), {'models', 'templates'})
        self.assertEqual(len(catalog['models']), 51)
        self.assertEqual(len(set(catalog['templates'])), 16)
        for key, models in catalog['models'].items():
            self.assertRegex(key, r'^[a-z0-9._-]+$')
            self.assertTrue(models and all(re.fullmatch(r'[A-Za-z0-9._-]+', m) for m in models))
        self.assertTrue(all(re.fullmatch(r'[a-z0-9-]+', t) for t in catalog['templates']))

    def test_docker_build_uses_the_same_strategy_implementation(self):
        docker = (ROOT / 'Dockerfile').read_text(encoding='utf-8')
        self.assertIn('COPY profiles/ ./profiles/', docker)
        self.assertNotIn('source-maintenance', docker)
        self.assertNotIn('packaging/relay-profiles', docker)

    def test_deprecated_port_is_not_in_the_environment_template(self):
        env = (ROOT / '.env.example').read_text(encoding='utf-8')
        self.assertNotRegex(env, r'(?m)^\s*(?:export\s+)?PORT\s*=')
        main = (ROOT / 'src/main.rs').read_text(encoding='utf-8')
        self.assertIn('var("BIND_ADDR")', main)
        self.assertNotIn('var("PORT")', main)
