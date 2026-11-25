import json
from pathlib import Path
import re
import unittest

ROOT = Path(__file__).resolve().parents[1]


class RelayCatalogTests(unittest.TestCase):
    def test_container_catalog_is_identifiers_only(self):
        catalog = json.loads((ROOT / 'packaging/relay-catalog.json').read_text(encoding='utf-8'))
        self.assertEqual(set(catalog), {'models', 'templates'})
        self.assertEqual(len(catalog['models']), 51)
        self.assertEqual(len(set(catalog['templates'])), 16)
        for key, models in catalog['models'].items():
            self.assertRegex(key, r'^[a-z0-9._-]+$')
            self.assertTrue(models and all(re.fullmatch(r'[A-Za-z0-9._-]+', m) for m in models))
        self.assertTrue(all(re.fullmatch(r'[a-z0-9-]+', t) for t in catalog['templates']))
