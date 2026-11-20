#!/usr/bin/env python3
"""Fast manifest and Docker-input checks before downloading or compiling dependencies.

Dockerfile `COPY` instructions and `.dockerignore` rules are always written
with POSIX separators, so every Docker-context path this module builds must be
normalized to POSIX (`/`) before it is compared or matched. Building paths with
`str(pathlib.Path(...))` emits `\\` on Windows and silently breaks those
comparisons there; `to_docker_path` is the single normalization point.
"""
import fnmatch
from pathlib import Path, PurePosixPath
import re
import subprocess
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def to_docker_path(value):
    """Return a POSIX, Docker-context-relative path for any host path style.

    Accepts `str`, `PurePosixPath`, `PureWindowsPath` or `Path` and always
    yields `/`-separated output, independent of the host operating system.
    """
    text = str(value).replace('\\', '/')
    return PurePosixPath(text).as_posix()


def to_fs_path(root, docker_path):
    """Map a normalized Docker-context path back onto the local filesystem.

    Windows accepts forward slashes in filesystem APIs, so `root / 'a/b'`
    is portable; using `PurePosixPath.parts` keeps the join deterministic.
    """
    return root.joinpath(*PurePosixPath(docker_path).parts)


def docker_included(path, rules):
    path = to_docker_path(path)
    included = True
    for rule in rules:
        rule = rule.strip()
        if not rule or rule.startswith('#'):
            continue
        allow = rule.startswith('!')
        pattern = to_docker_path(rule.lstrip('!').rstrip('/'))
        if fnmatch.fnmatch(path, pattern) or path.startswith(pattern + '/'):
            included = allow
    return included


def _copy_sources(line):
    """Extract normalized source operands from a `COPY` instruction.

    Handles `COPY --chown=... a b dest`, `COPY a b ./`, directory sources and
    root-relative (`/a/b`) or explicit relative (`./a/b`) spellings. Flags that
    take a value are skipped so `--chown=user:group` never looks like a source.
    """
    tokens = line.split()
    index = 1
    while index < len(tokens) and tokens[index].startswith('--'):
        index += 1
    sources = tokens[index:-1]
    return [to_docker_path(source.lstrip('./')) for source in sources]


def _is_covered(path, copies):
    return any(
        path == source or path.startswith(source.rstrip('/') + '/')
        for source in copies
    )


def validate(root=ROOT):
    manifest = tomllib.loads((root / 'Cargo.toml').read_text())
    required = ['Cargo.toml', 'Cargo.lock', 'assets/runtime-identity.onnx']
    for example in manifest.get('example', []):
        required.append(
            to_docker_path(example.get('path', f"examples/{example['name']}.rs"))
        )
    dependencies = dict(manifest.get('dependencies', {}))
    for table in manifest.get('target', {}).values():
        dependencies.update(table.get('dependencies', {}))
    for patches in manifest.get('patch', {}).values():
        dependencies.update(patches)
    for dependency in dependencies.values():
        if isinstance(dependency, dict) and 'path' in dependency:
            path = Path(dependency['path'])
            required.append(to_docker_path(path / 'Cargo.toml'))
            local = tomllib.loads((root / path / 'Cargo.toml').read_text())
            lib = local.get('lib', {}).get('path', 'src/lib.rs')
            required.append(to_docker_path(path / lib))
    rules = (root / '.dockerignore').read_text().splitlines()
    for path in required:
        if not to_fs_path(root, path).is_file():
            raise ValueError(f'Missing manifest input: {path}')
        if not docker_included(path, rules):
            raise ValueError(f'Docker context excludes required input: {path}')
    stage = ''
    copies = []
    for line in (root / 'Dockerfile').read_text().splitlines():
        if line.startswith('FROM '):
            stage = line.split()[-1]
            copies = []
        if line.startswith('COPY ') and '--from=' not in line:
            copies.extend(_copy_sources(line))
        if 'cargo fetch ' in line:
            for path in required:
                if not _is_covered(path, copies):
                    raise ValueError(f'{stage}: {path} must be copied before cargo fetch')
        if 'cargo build ' in line:
            if '--no-default-features' not in line:
                raise ValueError(f'{stage}: select container features explicitly')
            features = re.search(r'--features\s+([\w,-]+)', line)
            if not features:
                raise ValueError(f'{stage}: missing explicit features')
            selected = set(features[1].split(','))
            expected = {
                'builder': {'server', 'reverb-tts', 'web-ui', 'webgpu'},
                'builder-playback': {'server', 'playback', 'reverb-tts', 'web-ui'},
                'builder-cuda': {'server', 'cuda', 'reverb-tts', 'web-ui'},
            }
            for feature in features[1].split(','):
                if feature not in manifest['features']:
                    raise ValueError(f'{stage}: unknown Cargo feature {feature}')
            if stage in expected and selected != expected[stage]:
                raise ValueError(f'{stage}: expected provider/backend features {sorted(expected[stage])}')
    return required


def main():
    validate()
    subprocess.run(['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'],
                   cwd=ROOT, check=True, stdout=subprocess.DEVNULL)
    print('Cargo manifests, Docker inputs and features verified')


if __name__ == '__main__':
    main()
