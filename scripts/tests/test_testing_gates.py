"""Exercise the real shell gate with disposable command stand-ins, no services."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]


class TestingGateTests(unittest.TestCase):
    def run_gate(self, *, db_status=0, e2e_status=0, concurrency=None):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'scripts'
            scripts.mkdir()
            (root / 'backend').mkdir()
            shutil.copyfile(ROOT / 'scripts/check-db', scripts / 'check-db')
            (root / '.env').write_text('MIGRATION_DATABASE_URL=postgres://fixture/fixture\n')
            commands = {
                'curl': 'exit 0\n',
                'cargo-nextest': 'exit 0\n',
                'cargo': 'if [ "$1" = nextest ]; then exit "$DB_STATUS"; fi\nexit 0\n',
                'e2e': 'printf "%s\\n" "$@" > "$E2E_ARGS"\nexit "$E2E_STATUS"\n',
            }
            for name, body in commands.items():
                target = scripts / name
                target.write_text('#!/bin/sh\n' + body)
                target.chmod(0o755)
            args_file = root / 'e2e-args'
            env = {
                'PATH': str(scripts) + os.pathsep + os.defpath,
                'DB_STATUS': str(db_status), 'E2E_STATUS': str(e2e_status),
                'E2E_ARGS': str(args_file),
            }
            if concurrency is not None:
                env['CRM_E2E_CONCURRENCY'] = str(concurrency)
            result = subprocess.run(['bash', str(scripts / 'check-db')],
                                    cwd=root, env=env, capture_output=True,
                                    text=True, timeout=10)
            args = args_file.read_text().splitlines() if args_file.exists() else None
            return result, args

    def test_full_gate_runs_all_e2e_families_by_default(self):
        result, args = self.run_gate()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(args, ['--all', '--concurrency', '4', '--timeout', '900'])
        self.assertIn('all checks passed', result.stdout)

    def test_e2e_failure_fails_gate_and_honors_parallelism(self):
        result, args = self.run_gate(e2e_status=7, concurrency=2)
        self.assertEqual(result.returncode, 7)
        self.assertEqual(args, ['--all', '--concurrency', '2', '--timeout', '900'])
        self.assertNotIn('all checks passed', result.stdout)

    def test_failed_integration_suite_does_not_launch_e2e(self):
        result, args = self.run_gate(db_status=8)
        self.assertEqual(result.returncode, 8)
        self.assertIsNone(args)
        self.assertNotIn('all checks passed', result.stdout)


if __name__ == '__main__':
    unittest.main()
