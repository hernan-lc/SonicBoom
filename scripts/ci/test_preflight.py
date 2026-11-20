"""Regression checks for the source preflight; no third-party test framework."""
from pathlib import Path, PureWindowsPath
import shutil
import tempfile
import unittest

from preflight import (
    ROOT,
    _copy_sources,
    docker_included,
    to_docker_path,
    to_fs_path,
    validate,
)


class PathNormalizationTests(unittest.TestCase):
    def test_windows_and_posix_paths_normalize_identically(self):
        expected = 'vendor/esaxx-rs/Cargo.toml'
        self.assertEqual(to_docker_path(expected), expected)
        self.assertEqual(to_docker_path('vendor\\esaxx-rs\\Cargo.toml'), expected)
        self.assertEqual(
            to_docker_path(PureWindowsPath('vendor', 'esaxx-rs', 'Cargo.toml')),
            expected,
        )
        self.assertEqual(to_docker_path(Path('vendor/esaxx-rs/Cargo.toml')), expected)

    def test_fs_path_is_portable(self):
        root = Path(tempfile.gettempdir())
        self.assertEqual(to_fs_path(root, 'a/b/c.txt'), root / 'a' / 'b' / 'c.txt')

    def test_copy_sources_normalize_and_strip_prefixes(self):
        self.assertEqual(_copy_sources('COPY vendor ./vendor'), ['vendor'])
        self.assertEqual(
            _copy_sources('COPY Cargo.toml Cargo.lock ./'),
            ['Cargo.toml', 'Cargo.lock'],
        )
        self.assertEqual(_copy_sources('COPY /src ./src'), ['src'])
        self.assertEqual(
            _copy_sources('COPY --chown=1000:1000 vendor ./vendor'),
            ['vendor'],
        )

    def test_docker_included_accepts_windows_separators(self):
        rules = ['**', '!vendor/', '!vendor/**']
        self.assertTrue(docker_included('vendor\\esaxx-rs\\Cargo.toml', rules))


class PreflightTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        for path in validate():
            dest = self.root / path
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / path, dest)
        for path in ['Dockerfile', '.dockerignore']:
            shutil.copyfile(ROOT / path, self.root / path)

    def test_repository_inputs(self):
        validate(self.root)

    def test_missing_path_dependency_source(self):
        (self.root / 'vendor/esaxx-rs/src/lib.rs').unlink()
        with self.assertRaisesRegex(ValueError, 'vendor/esaxx-rs/src/lib.rs'):
            validate(self.root)

    def test_unknown_docker_feature(self):
        path = self.root / 'Dockerfile'
        path.write_text(path.read_text().replace('reverb-tts', 'reverb'))
        with self.assertRaisesRegex(ValueError, 'unknown Cargo feature reverb'):
            validate(self.root)

    def test_optional_path_must_be_copied_before_fetch(self):
        path = self.root / 'Dockerfile'
        path.write_text(path.read_text().replace('COPY vendor ./vendor', ''))
        with self.assertRaisesRegex(ValueError, 'before cargo fetch'):
            validate(self.root)

    def test_cuda_must_select_the_onnx_cuda_backend(self):
        path = self.root / 'Dockerfile'
        path.write_text(path.read_text().replace('server,cuda,', 'server,'))
        with self.assertRaisesRegex(ValueError, 'expected provider/backend features'):
            validate(self.root)

    def test_context_must_include_examples(self):
        with (self.root / '.dockerignore').open('a') as stream:
            stream.write('\nexamples/**\n')
        with self.assertRaisesRegex(ValueError, 'context excludes'):
            validate(self.root)

    def test_credentials_and_nested_git_are_excluded(self):
        rules = (ROOT / '.dockerignore').read_text().splitlines()
        for path in ['.env', 'tokens.json', 'models/weights.onnx', 'vendor/ort/.git',
                     'frontend/.env.local', 'vendor/tokens.json',
                     'vendor/weights.gguf', 'vendor/weights.onnx']:
            self.assertFalse(docker_included(path, rules), path)


if __name__ == '__main__':
    unittest.main()
