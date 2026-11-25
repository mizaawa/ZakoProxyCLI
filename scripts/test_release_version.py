import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location('release_version', ROOT / 'scripts/check-release-version.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class ReleaseVersionTests(unittest.TestCase):
    def test_repository_consistent(self):
        self.assertRegex(MODULE.check(ROOT), r'^\d+\.\d+\.\d+$')

    def test_stale_docker_label_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            fixture = Path(directory)
            for relative in ['Cargo.toml', 'Cargo.lock', 'web/package.json', 'web/package-lock.json', 'Dockerfile']:
                destination = fixture / relative
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(ROOT / relative, destination)
            dockerfile = fixture / 'Dockerfile'
            text = dockerfile.read_text(encoding='utf-8')
            current = MODULE.check(ROOT)
            dockerfile.write_text(text.replace(f'image.version="{current}"', 'image.version="0.0.0"'), encoding='utf-8')
            with self.assertRaisesRegex(ValueError, 'Release version mismatch'):
                MODULE.check(fixture)

    def test_root_compose_requires_checkout_for_both_targets(self):
        version = MODULE.check(ROOT)
        self.assertFalse((ROOT / 'compose.yaml').exists(), 'would shadow docker-compose.yaml')
        spec = (ROOT / 'docker-compose.yaml').read_text(encoding='utf-8')
        self.assertNotRegex(spec, r'(?m)^\s+image:')
        self.assertIn('target: runtime', spec)
        self.assertIn('target: updater', spec)
        self.assertIn(f'org.opencontainers.image.version="{version}"', (ROOT / 'Dockerfile').read_text(encoding='utf-8'))

    def test_running_check_requires_matching_image_id_label_and_both_binaries(self):
        version = '0.14.4'
        sources = {s: f'test-{s}' for s in ['proxy', 'updater']}
        def responses():
            return [arg for service in sources for arg in [f'ctr-{service}', f'id-{service} {version}',
                    f'id-{service}', json.dumps({'status':'ok', 'version':version})]]
        for wrong_at, wrong_value in [(None, ''), (2, 'other-id'), (5, 'id-updater 0.6.0'),
                                      (7, '{"status":"ok","version":"0.6.0"}'), (0, '')]:
            with self.subTest(wrong_at=wrong_at):
                output = responses()
                if wrong_at is not None:
                    output[wrong_at] = wrong_value
                with patch.object(MODULE, 'source_images', return_value=sources), patch.object(MODULE, 'output', side_effect=output), patch('builtins.print'):
                    if wrong_at is None:
                        MODULE.check_running(ROOT, version)
                    else:
                        with self.assertRaises(ValueError):
                            MODULE.check_running(ROOT, version)


if __name__ == '__main__':
    unittest.main()
