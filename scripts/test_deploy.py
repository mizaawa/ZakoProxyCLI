from pathlib import Path
import os
import re
import runpy
import subprocess
import tempfile
import unittest
from unittest.mock import Mock, patch


ROOT = Path(__file__).resolve().parents[1]
DEPLOY = runpy.run_path(str(ROOT / 'deploy.py'))


class SourceDeploymentTests(unittest.TestCase):
    def test_first_install_and_update_never_overwrite_env(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.env.example').write_bytes((ROOT / '.env.example').read_bytes())
            DEPLOY['ensure_env'](root)
            env = root / '.env'
            self.assertTrue(env.is_file())
            if os.name == 'posix':
                self.assertEqual(re.search(r'^ZAKO_GID=(.*)$', env.read_text().strip(), re.M)[1], str(env.stat().st_gid))
                self.assertEqual(env.stat().st_mode & 0o777, 0o660)
            env.write_bytes(b'operator-defined=keep\n')
            DEPLOY['ensure_env'](root)
            self.assertEqual(env.read_bytes(), b'operator-defined=keep\n')

    def test_failed_build_leaves_running_services_alone(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / '.env').touch()
            verify = Mock()
            with patch('runpy.run_path', return_value={'check':Mock(return_value='1.0.0'), 'source_images':Mock(), 'check_running':verify}), patch('subprocess.run', side_effect=subprocess.CalledProcessError(1, 'build')) as run:
                with self.assertRaises(subprocess.CalledProcessError):
                    DEPLOY['deploy'](root)
            run.assert_called_once_with(['docker', 'compose', '-f', 'docker-compose.yaml', 'build', '--pull'], cwd=root, check=True)
            verify.assert_not_called()


if __name__ == '__main__':
    unittest.main()
