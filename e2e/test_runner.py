"""Safety and diagnostics checks; no Docker daemon or shared database required."""
import base64
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import zipfile

from e2e import runner


class RunnerTests(unittest.TestCase):
    def test_cached_images_skip_build_and_missing_image_rebuilds_only_target(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            run = state / 'run'
            run.mkdir()
            for missing in [False, True]:
                def inspect(args, **kwargs):
                    absent = missing and 'browser' in args[-1]
                    return runner.subprocess.CompletedProcess(args, 1 if absent else 0,
                                                              stdout='' if absent else 'sha256:cached\n')
                with self.subTest(missing=missing), patch.object(runner, 'STATE', state), \
                     patch.object(runner, 'image_fingerprint', side_effect=lambda target: target), \
                     patch.object(runner.subprocess, 'run', side_effect=inspect), \
                     patch.object(runner, 'read_command', return_value='sha256:built'), \
                     patch.object(runner, 'command') as command:
                    images = runner.build(run)
                    self.assertEqual(command.call_count, int(missing))
                    self.assertEqual(images['api'], 'sha256:cached')
                    self.assertEqual(images['web'], 'sha256:cached')
                    self.assertEqual(images['browser'], 'sha256:built' if missing else 'sha256:cached')
                    if missing:
                        self.assertEqual(command.call_args.args[0][4:6], ['--target', 'browser'])

    def test_image_keys_track_only_relevant_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def put(name, content):
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(content)
            def keys():
                return {target: runner.image_fingerprint(target, root) for target in runner.BUILD_INPUTS}
            put('backend/crates/api/src/main.rs', 'old')
            put('web/src/main.ts', 'old')
            put('e2e/families/leads.spec.mjs', 'old')
            original = keys()
            put('e2e/runner.py', 'host change')
            put('e2e/README.md', 'documentation change')
            put('backend/crates/api/target/output', 'build output')
            put('web/src/.env.local', 'ignored')
            self.assertEqual(keys(), original)
            for path, target in [('backend/crates/api/src/main.rs', 'api'),
                                 ('web/src/main.ts', 'web'), ('e2e/families/leads.spec.mjs', 'browser')]:
                before = keys()
                put(path, 'changed')
                after = keys()
                self.assertEqual([t for t in before if before[t] != after[t]], [target] + ({'api': ['migration-api'], 'web': ['calls-web']}.get(target, [])))
                (root / path).unlink()
                self.assertNotEqual(keys()[target], after[target])
            before = keys()
            put('e2e/Dockerfile', 'new recipe')
            self.assertTrue(all(keys()[t] != before[t] for t in before))

    def test_cleanup_rejects_shared_project_before_docker(self):
        with patch.object(runner, 'command') as command, patch.object(runner, 'read_command') as read:
            with self.assertRaises(ValueError):
                runner.owned_cleanup('crm-development', Path('/nonexistent'), Path('/tmp/unused'))
            command.assert_not_called()
            read.assert_not_called()

    def test_cleanup_rejects_unowned_live_resource(self):
        project = 'crm-e2e-123456abcdef-leads-1-a1'
        with tempfile.TemporaryDirectory() as directory:
            private = Path(directory)
            runner.write_json(private / 'compose.json', {'services': {'api': {'labels': {
                'crm.e2e.owner': runner.OWNER, 'crm.e2e.project': project}}}})
            for resource in ['container', 'network', 'volume']:
                calls = ['id', json.dumps([{'Config': {'Labels': {}}}])] if resource == 'container' else (
                    ['', 'id', json.dumps([{'Labels': {}}])] if resource == 'network' else
                    ['', '', 'id', json.dumps([{'Labels': {}}])])
                with self.subTest(resource=resource), patch.object(runner, 'read_command', side_effect=calls), patch.object(runner, 'command') as command:
                    with self.assertRaises(ValueError):
                        runner.owned_cleanup(project, private, private / 'log')
                    command.assert_not_called()

    def test_cleanup_rejects_explicit_shared_volume_name(self):
        project = 'crm-e2e-123456abcdef-leads-1-a1'
        with tempfile.TemporaryDirectory() as directory:
            private = Path(directory)
            runner.write_json(private / 'compose.json', {'services': {}, 'volumes': {
                'database': {'name': 'shared-development-database'}}})
            with patch.object(runner, 'read_command') as read:
                with self.assertRaises(ValueError):
                    runner.owned_cleanup(project, private, private / 'log')
                read.assert_not_called()

    def test_sanitizes_nested_trace_and_embedded_html(self):
        secret = 'synthetic-session-value'
        def archive(name, content):
            out = io.BytesIO()
            with zipfile.ZipFile(out, 'w') as z:
                z.writestr(name, content)
            return out.getvalue()
        nested = archive('trace.zip', archive('network', secret))
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            artifacts, private = root / 'artifacts', root / 'private'
            artifacts.mkdir(); private.mkdir()
            (private / 'observed-secrets.json').write_text(json.dumps([secret]))
            (artifacts / 'trace.zip').write_bytes(nested)
            (artifacts / 'index.html').write_text('data:application/zip;base64,' + base64.b64encode(nested).decode())
            runner.sanitize(artifacts, private, {})
            for data in [(artifacts / 'trace.zip').read_bytes(),
                         base64.b64decode((artifacts / 'index.html').read_text().split(',')[1])]:
                with zipfile.ZipFile(io.BytesIO(data)) as outer:
                    with zipfile.ZipFile(io.BytesIO(outer.read('trace.zip'))) as inner:
                        self.assertEqual(inner.read('network'), b'[REDACTED]')

    def test_timeout_and_interrupt_reap_command(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / 'log'
            with self.assertRaises(TimeoutError):
                runner.command(['python3', '-c', 'import time; time.sleep(30)'], log, timeout=.05)
            runner.STOP.set()
            try:
                with self.assertRaises(InterruptedError):
                    runner.command(['python3', '-c', 'import time; time.sleep(30)'], log)
            finally:
                runner.STOP.clear()

    def test_inbound_credential_is_family_scoped(self):
        credentials = dict.fromkeys(['app', 'root', 'migrator', 'audit', 'session', 'raw',
                                    'password', 'realtime_api', 'realtime_token', 'inbound', 'inference', 'livekit_key', 'livekit_secret'], 'synthetic')
        for family in runner.FAMILIES:
            config = runner.config('test-project', Path('/private'), Path('/artifacts'),
                                   dict.fromkeys(runner.BUILD_INPUTS, 'image'), credentials, family)
            api = config['services']['api']['environment']
            browser = config['services']['browser']['environment']
            self.assertEqual(api['CRM_INBOUND_EMAIL_SECRET'], 'synthetic' if family in {'routing', 'correspondence'} else '')
            self.assertEqual(browser.get('E2E_INBOUND_SECRET'), 'synthetic' if family in {'routing', 'correspondence'} else None)
            self.assertEqual(api['GROQ_API_KEY'], 'synthetic' if family in {'operator', 'calls'} else '')

    def test_optional_images_only_build_for_their_family(self):
        self.assertEqual(runner.build_targets(['leads', 'operator']), ['api', 'web', 'browser'])
        self.assertEqual(runner.build_targets(['calls', 'migration']), ['api', 'web', 'browser', 'calls-web', 'migration-api'])

    def test_manifest_owns_private_network_and_no_host_ports(self):
        credentials = dict.fromkeys(['app', 'root', 'migrator', 'audit', 'session', 'raw',
                                    'password', 'realtime_api', 'realtime_token'], 'synthetic')
        config = runner.config('test-project', Path('/private'), Path('/artifacts'),
                               dict.fromkeys(['api', 'web', 'browser'], 'image'), credentials, 'leads')
        self.assertTrue(config['networks']['default']['internal'])
        for service in config['services'].values():
            self.assertNotIn('ports', service)
            self.assertNotIn('network_mode', service)
            self.assertEqual(service['labels']['crm.e2e.owner'], runner.OWNER)
        self.assertEqual(config['services']['api']['environment']['GROQ_API_KEY'], '')


if __name__ == '__main__':
    unittest.main()
