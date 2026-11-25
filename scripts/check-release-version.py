import argparse
import json
from pathlib import Path
import re
import subprocess
import tomllib


def check(root):
    with (root / 'Cargo.toml').open('rb') as source:
        package = tomllib.load(source)['package']
    with (root / 'Cargo.lock').open('rb') as source:
        locked = tomllib.load(source)['package']
    frontend = json.loads((root / 'web/package.json').read_text(encoding='utf-8'))
    npm_lock = json.loads((root / 'web/package-lock.json').read_text(encoding='utf-8'))
    labels = re.findall(
        r'^LABEL org\.opencontainers\.image\.version="([^"]+)"\s*$',
        (root / 'Dockerfile').read_text(encoding='utf-8'), re.MULTILINE,
    )
    versions = {
        'Cargo.toml': package['version'],
        'Cargo.lock': next(item['version'] for item in locked if item['name'] == package['name']),
        'web/package.json': frontend['version'],
        'web/package-lock.json': npm_lock['version'],
        'web/package-lock.json root': npm_lock['packages']['']['version'],
        'Dockerfile OCI version label': labels[0] if len(labels) == 1 else None,
    }
    if any(value != package['version'] for value in versions.values()):
        raise ValueError(f'Release version mismatch: {versions}')
    return package['version']


def output(root, *args):
    return subprocess.check_output(args, cwd=root, text=True, encoding='utf-8', timeout=30).strip()


def source_images(root):
    # Do not pick up an old compose.yaml or an unrelated COMPOSE_FILE.
    spec = json.loads(output(root, 'docker', 'compose', '-f', 'docker-compose.yaml', 'config', '--format', 'json'))
    images = {}
    for service, target in [('proxy', 'runtime'), ('updater', 'updater')]:
        settings = spec['services'][service]
        build = settings.get('build') or {}
        context = build.get('context')
        if (settings.get('image') or not context or Path(context).resolve() != root.resolve()
                or build.get('dockerfile', 'Dockerfile') != 'Dockerfile' or build.get('target') != target):
            raise ValueError(f'{service}: expected a local build of Dockerfile target {target}, not a registry image')
        images[service] = f"{spec['name']}-{service}"
    return images


def check_running(root, version):
    label = 'org.opencontainers.image.version'
    for service, tag in source_images(root).items():
        cid = output(root, 'docker', 'compose', '-f', 'docker-compose.yaml', 'ps', '-q', service)
        if not cid or len(cid.splitlines()) != 1:
            raise ValueError(f'{service}: expected one running container in this Compose project')
        image_id, _, image_version = output(root, 'docker', 'image', 'inspect', '--format',
            '{{.Id}} {{index .Config.Labels "' + label + '"}}', tag).partition(' ')
        running_id = output(root, 'docker', 'inspect', '--format', '{{.Image}}', cid)
        if running_id != image_id or image_version != version:
            raise ValueError(f'{service}: source={version}, local={tag} ({image_id}, version={image_version}), running={running_id}')
        port = 1990 if service == 'proxy' else 9000
        health = json.loads(output(root, 'docker', 'exec', cid, 'curl', '--fail', '--silent', '--show-error',
                                   '--max-time', '5', f'http://127.0.0.1:{port}/health'))
        if health.get('status') != 'ok' or health.get('version') != version:
            raise ValueError(f"{service}: expected a healthy {version} binary, got version={health.get('version')}")
        print(f'PASS {service}: source, image and running binary are {version} ({image_id})')


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description='Check source versions and optionally the running local Compose deployment.')
    parser.add_argument('--running', action='store_true', help='verify both local image IDs, OCI labels and running programs')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    try:
        version = check(root)
        print(f'PASS release version consistency: {version}', flush=True)
        if args.running:
            check_running(root, version)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError) as exc:
        parser.exit(1, f'FAIL: {exc}\n')
