"""Publishing must fail closed on ambiguous authorization and asset failures."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import package
import io
import tarfile
import zipfile
from test_packaging import BUILD_INFO
import publish


class PublishingTests(unittest.TestCase):
    COMMIT = 'a' * 40
    EXPECTED_ASSETS = [{'name': n, 'size': 1} for n in (*package.ASSETS, 'SHA256SUMS')]

    def run_preflight(self, releases, ref=None, tag_object=None, commit=COMMIT):
        """Drive preflight with canned GitHub state; returns (state, gh_calls)."""
        def api_get(path):
            if path.endswith('/git/ref/tags/v0.0.5'):
                return ref
            if '/git/tags/' in path:
                return tag_object
            raise AssertionError(path)
        with patch.object(publish, 'api_get', side_effect=api_get):
            with patch.object(publish, 'releases_for_tag', return_value=releases):
                with patch.object(publish, 'gh') as gh:
                    state = publish.preflight('owner/repo', 'v0.0.5', commit)
                    return state, gh.call_args_list

    def test_api_errors_other_than_404_fail_closed(self):
        for error in ['HTTP 403: Forbidden', 'network unavailable']:
            with patch.object(publish.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', error)):
                with self.assertRaises(RuntimeError):
                    publish.api_get('repos/owner/repo/git/ref/tags/v0.0.5')
        with patch.object(publish.subprocess, 'run', return_value=subprocess.CompletedProcess([], 1, '', 'HTTP 404: Not Found')):
            self.assertIsNone(publish.api_get('repos/owner/repo/git/ref/tags/v0.0.5'))

    def test_no_release_and_no_tag_publishes_normally(self):
        state, calls = self.run_preflight([])
        self.assertEqual(state, 'absent')
        self.assertEqual(calls, [])

    def test_existing_draft_with_matching_assets_is_reused(self):
        draft = {'id': 5, 'draft': True, 'body': 'generated notes', 'assets': self.EXPECTED_ASSETS}
        state, calls = self.run_preflight([draft])
        self.assertEqual(state, 'reuse-draft')
        self.assertEqual(calls, [])  # nothing deleted

    def test_draft_with_missing_or_unexpected_assets_fails_without_deleting(self):
        bad = [
            self.EXPECTED_ASSETS[:2],
            self.EXPECTED_ASSETS + [{'name': 'extra.bin', 'size': 1}],
            [{'name': a['name'], 'size': 0} for a in self.EXPECTED_ASSETS],
            [{'name': 'other', 'size': 1}],
        ]
        for assets in bad:
            draft = {'id': 5, 'draft': True, 'body': '', 'assets': assets}
            with patch.object(publish, 'gh') as gh:
                with self.assertRaises(RuntimeError):
                    with patch.object(publish, 'releases_for_tag', return_value=[draft]):
                        with patch.object(publish, 'api_get', return_value=None):
                            publish.preflight('owner/repo', 'v0.0.5', self.COMMIT)
                gh.assert_not_called()

    def test_two_drafts_are_ambiguous(self):
        drafts = [{'id': i, 'draft': True, 'assets': self.EXPECTED_ASSETS} for i in (1, 2)]
        with self.assertRaisesRegex(RuntimeError, 'Ambiguous'):
            self.run_preflight(drafts)

    def test_published_release_is_never_replaced(self):
        occupied = [{'id': 7, 'draft': False, 'assets': [{'name': package.ASSETS[0], 'size': 1}], 'body': ''},
                    {'id': 7, 'draft': False, 'assets': [], 'body': 'operator release notes'}]
        for release in occupied:
            with self.assertRaisesRegex(RuntimeError, 'already exists'):
                self.run_preflight([release])

    def test_empty_placeholders_are_removed_only_after_every_check_passes(self):
        placeholder = {'id': 42, 'draft': False, 'assets': [], 'body': ' \n'}
        state, calls = self.run_preflight([placeholder])
        self.assertEqual(state, 'absent')
        self.assertEqual([c.args for c in calls], [('api', '-X', 'DELETE', 'repos/owner/repo/releases/42')])
        # A bad tag must abort before anything is deleted.
        moved = {'object': {'type': 'commit', 'sha': 'b' * 40}}
        with patch.object(publish, 'api_get', return_value=moved):
            with patch.object(publish, 'releases_for_tag', return_value=[placeholder]):
                with patch.object(publish, 'gh') as gh:
                    with self.assertRaises(RuntimeError):
                        publish.preflight('owner/repo', 'v0.0.5', self.COMMIT)
                    gh.assert_not_called()

    def test_tag_on_another_commit_is_refused_and_tag_on_tested_commit_is_allowed(self):
        other = {'object': {'type': 'commit', 'sha': 'b' * 40}}
        with self.assertRaisesRegex(RuntimeError, 'does not point at the tested commit'):
            self.run_preflight([], ref=other)
        same = {'object': {'type': 'commit', 'sha': self.COMMIT}}
        self.assertEqual(self.run_preflight([], ref=same)[0], 'absent')
        with self.assertRaises(RuntimeError):  # unknown tested commit: cannot prove a match
            self.run_preflight([], ref=same, commit=None)

    def test_annotated_tag_is_resolved_to_its_commit(self):
        annotated = {'object': {'type': 'tag', 'sha': 'c' * 40}}
        good = {'object': {'type': 'commit', 'sha': self.COMMIT}}
        bad = {'object': {'type': 'commit', 'sha': 'b' * 40}}
        self.assertEqual(self.run_preflight([], ref=annotated, tag_object=good)[0], 'absent')
        with self.assertRaises(RuntimeError):
            self.run_preflight([], ref=annotated, tag_object=bad)

    def test_existing_release_message_tells_the_operator_what_to_do(self):
        with self.assertRaises(RuntimeError) as caught:
            self.run_preflight([{'id': 1, 'draft': False, 'assets': [], 'body': 'notes'}])
        message = str(caught.exception)
        self.assertIn('v0.0.5', message)
        self.assertIn('Delete both the existing release and tag, or choose a new version.', message)

    def test_release_lookup_resolves_drafts_and_rejects_missing_or_ambiguous(self):
        empty = {'tag_name': 'v0.0.5', 'draft': False, 'assets': []}
        draft = {'tag_name': 'v0.0.5', 'draft': True, 'assets': [{'name': package.ASSETS[0], 'size': 1}]}
        other = {'tag_name': 'v0.0.5', 'draft': True, 'assets': [{'name': package.ASSETS[1], 'size': 1}]}
        for pages, expected in [([[draft]], draft), ([[empty, draft]], draft), ([[empty]], empty)]:
            with patch.object(publish, 'gh', return_value=json.dumps(pages)):
                self.assertEqual(publish.find_release('owner/repo', 'v0.0.5'), expected)
        for pages in [[], [[]], [[draft, other]]]:
            with patch.object(publish, 'gh', return_value=json.dumps(pages)):
                with self.assertRaises(RuntimeError):
                    publish.find_release('owner/repo', 'v0.0.5')

    def test_verification_rejects_missing_duplicate_and_empty_assets(self):
        good = [{'name': name, 'size': 1} for name in [*package.ASSETS, 'SHA256SUMS']]
        for assets in [good[:-1], [*good, good[0]], [{**a, 'size': 0} for a in good], []]:
            release = json.dumps([[{'tag_name': 'v0.0.5', 'draft': True, 'assets': assets}]])
            with patch.object(publish, 'gh', return_value=release):
                with self.assertRaises(RuntimeError):
                    publish.verify('owner/repo', 'v0.0.5', Path('unused'))

    def test_downloaded_bytes_must_match_locally_tested_bytes(self):
        with tempfile.TemporaryDirectory() as temp:
            local = Path(temp)
            for name in package.ASSETS:
                payload = json.dumps(BUILD_INFO).encode()
                if name.endswith('.zip'):
                    with zipfile.ZipFile(local / name, 'w') as z:
                        z.writestr('BUILD-INFO.json', payload)
                else:
                    with tarfile.open(local / name, 'w:gz') as tar:
                        entry = tarfile.TarInfo('BUILD-INFO.json')
                        entry.size = len(payload)
                        tar.addfile(entry, io.BytesIO(payload))
            package.checksums(local)
            tamper = False
            def gh(*args):
                if args[0] == 'api':
                    return json.dumps([[{'tag_name': 'v0.0.5', 'draft': True,
                                         'assets': [{'name': name, 'size': (local / name).stat().st_size}
                                                    for name in [*package.ASSETS, 'SHA256SUMS']]}]])
                downloaded = Path(args[-1])
                for source in local.iterdir():
                    shutil.copyfile(source, downloaded / source.name)
                if tamper:
                    # Even a self-consistent replacement must fail comparison
                    # against the original tested artifacts.
                    (downloaded / package.ASSETS[0]).write_bytes(b'replacement bytes')
                    package.checksums(downloaded)
                return ''
            with patch.object(publish, 'gh', side_effect=gh):
                publish.verify('owner/repo', 'v0.0.5', local)
                tamper = True
                with self.assertRaises(RuntimeError):
                    publish.verify('owner/repo', 'v0.0.5', local)


if __name__ == '__main__':
    unittest.main()
