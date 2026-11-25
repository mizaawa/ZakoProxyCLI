"""Update and deploy the root Compose project from source. Requires Python 3.11+."""
import argparse
import os
from pathlib import Path
import re
import runpy
import subprocess
import sys

ROOT = Path(__file__).resolve().parent


def ensure_env(root):
    path = root / '.env'
    if path.is_file():
        return  # never rewrite an existing deployment's secrets or permissions
    template = (root / '.env.example').read_text(encoding='utf-8')
    with open(path, 'x', encoding='utf-8', newline='\n', opener=lambda p, flags: os.open(p, flags, 0o600)) as env:
        if os.name == 'posix':
            fd = env.fileno()
            if os.fstat(fd).st_gid == 0 and os.geteuid() == 0:
                os.fchown(fd, -1, 10001)  # do not add the proxy process to the host root group
            template = re.sub(r'^ZAKO_GID=.*$', f'ZAKO_GID={os.fstat(fd).st_gid}', template, flags=re.M)
            os.fchmod(fd, 0o660)
        env.write(template)
    print('Initialized .env (first installation only)', flush=True)


def deploy(root):
    checks = runpy.run_path(str(root / 'scripts/check-release-version.py'))
    version = checks['check'](root)
    ensure_env(root)
    checks['source_images'](root)  # reject a stale or registry-based manifest before touching running services
    compose = ['docker', 'compose', '-f', 'docker-compose.yaml']
    print(f'Building proxy and updater from source version {version}', flush=True)
    subprocess.run([*compose, 'build', '--pull'], cwd=root, check=True)
    subprocess.run([*compose, 'up', '-d', '--no-build', '--pull', 'never', '--wait', '--wait-timeout', '150'], cwd=root, check=True)
    checks['check_running'](root, version)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--local', action='store_true', help='build the current checkout/tag without updating Git')
    args = parser.parse_args()
    try:
        if args.local:
            deploy(ROOT)
        else:
            if subprocess.check_output(['git', 'status', '--porcelain', '--untracked-files=no'], cwd=ROOT).strip():
                raise ValueError('Tracked files changed. Commit them or explicitly choose --local; nothing was deployed.')
            subprocess.run(['git', 'pull', '--ff-only', '--no-autostash'], cwd=ROOT, check=True)
            head, upstream = (subprocess.check_output(['git', 'rev-parse', ref], cwd=ROOT).strip() for ref in ('HEAD', '@{upstream}'))
            if head != upstream:
                raise ValueError('Local HEAD differs from its upstream; use --local if these are intentional local commits.')
            # The pull may have updated this script; run the version that matches the checkout.
            subprocess.run([sys.executable, str(ROOT / 'deploy.py'), '--local'], cwd=ROOT, check=True)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'Deployment failed: {exc}\n')
