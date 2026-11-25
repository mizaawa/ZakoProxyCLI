"""Export the source-managed relay edition from a committed Git ref into a NEW directory."""
import argparse
import json
from pathlib import Path
import subprocess


def export(root, destination, ref):
    commit = subprocess.check_output(['git', 'rev-parse', '--verify', f'{ref}^{{commit}}'], cwd=root, text=True).strip()
    destination.mkdir(parents=True, exist_ok=False)
    def read(path):
        return subprocess.check_output(['git', 'show', f'{commit}:{path}'], cwd=root)
    def write(path, contents):
        dest = destination / path
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(contents)
    paths = subprocess.check_output(['git', 'ls-tree', '-r', '--name-only', '-z', commit], cwd=root).decode().split('\0')
    fixed = {'.dockerignore','.env.example','.gitignore','Cargo.lock','Cargo.toml','Dockerfile','docker-compose.yaml',
             'deploy.py','LICENSE','SECURITY.md','scripts/check-release-version.py','scripts/test_release_version.py',
             'scripts/test_deploy.py','scripts/test_relay_catalog.py','tests/providers.rs','tests/provider_keys.rs'}
    for path in filter(None, paths):
        notice = path in {'third_party/gpt-instruct.LICENSE','third_party/coldcoffee.LICENSE','third_party/coldcoffee-gpt/LICENSE','third_party/coldcoffee-full/LICENSE','third_party/kimi-break/LICENSE','third_party/ai-jailbreak/LICENSE','third_party/ai-jailbreak/NOTICE.md','third_party/hanshuang/NOTICE.md'}
        if path != 'src/profiles.rs' and (path in fixed or path.startswith(('src/', 'web/', 'packaging/', 'tests/fixtures/official-provider/')) or notice):
            write(path, read(path))
    write('src/profiles.rs', read('packaging/relay-profiles.rs'))
    write('src/relay-catalog.json', read('packaging/relay-catalog.json'))
    write('.cargo/config.toml', b'[env]\nZAKO_DEPLOYMENT_FLAVOR = { value = "source-maintenance", force = true }\n')
    readme = read('docs/releases/v0.14.4.md')
    write('README.md', readme)
    write('docs/releases/v0.14.4.md', readme)
    write('SOURCE.json', (json.dumps({'edition':'source-maintenance','source_commit':commit}, indent=2)+'\n').encode())
    return commit


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('destination', type=Path)
    parser.add_argument('--ref', default='HEAD')
    args = parser.parse_args()
    print(export(Path(__file__).resolve().parents[1], args.destination, args.ref))
