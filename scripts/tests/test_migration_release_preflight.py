"""Synthetic-only release evidence tests. No database, build, or live processes."""
import contextlib
import copy
import datetime as dt
import hashlib
import io
import json
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest import mock


SCRIPT = Path(__file__).resolve().parents[1] / "migration-release-preflight"
MODULE = runpy.run_path(str(SCRIPT), run_name="migration_preflight_test")
evaluate = MODULE["evaluate"]
EvidenceError = MODULE["EvidenceError"]
NOW = dt.datetime(2026, 9, 11, 12, tzinfo=dt.timezone.utc)
CURRENT = "a" * 64
LEGACY = "b" * 64


def fixtures(bindings="0", retired=True):
    artifacts = {"version": 1, "artifacts": [
        {"sha256": CURRENT, "role": "api", "gate_version": "crm-workspace-v1", "revision": "c" * 40},
        {"sha256": LEGACY, "role": "api", "gate_version": "pre-010c", "revision": "d" * 40},
    ]}
    candidates = {"version": 1, "artifacts": [{"role": "api", "sha256": CURRENT}]}
    inventory = {"version": 1, "target": "synthetic", "observed_at": "2026-09-11T12:00:00Z",
                 "complete": True, "pre_010c_retired": retired,
                 "processes": [{"id": "synthetic-api", "role": "api", "sha256": CURRENT}]}
    state = {"schema_present": True, "binding_count": bindings,
             "gate_versions": ["crm-workspace-v1"] if bindings != "0" else [],
             "database_name": "synthetic_only"}
    return artifacts, candidates, inventory, state


def check(values):
    return evaluate(*values, "synthetic", NOW)


class PreflightTests(unittest.TestCase):
    def test_current_artifact_retired_fleet_allows_initial_and_retained_review(self):
        for count in ["0", "1", "100000"]:
            report = check(fixtures(count))
            self.assertTrue(report["launch_allowed"])
            self.assertTrue(report["confirmation_ready"])
            self.assertEqual(report["binding_count"], count)
            self.assertEqual(report["process_count"], "1")

    def test_first_confirmation_requires_explicit_retirement(self):
        report = check(fixtures(retired=False))
        self.assertTrue(report["launch_allowed"])
        self.assertFalse(report["confirmation_ready"])
        self.assertIn("retirement_required", report["confirmation_reasons"])
        report = check(fixtures("1", retired=False))
        self.assertFalse(report["launch_allowed"])

    def test_retained_binding_rejects_legacy_and_unknown_candidates(self):
        # Run lifecycle is deliberately absent: completed/cancelled still bind.
        for digest in [LEGACY, "e" * 64]:
            values = fixtures("1")
            values[1]["artifacts"][0]["sha256"] = digest
            report = check(values)
            self.assertFalse(report["launch_allowed"])
            self.assertFalse(report["confirmation_ready"])

    def test_retirement_does_not_override_legacy_unknown_or_mixed_fleet(self):
        for digest in [LEGACY, "e" * 64]:
            values = fixtures()
            values[2]["processes"].append({"id": "forgotten-worker", "role": "api", "sha256": digest})
            report = check(values)
            self.assertFalse(report["launch_allowed"])
            self.assertFalse(report["confirmation_ready"])
            expected = "retirement_contradicted" if digest == LEGACY else "process_unknown"
            self.assertIn(expected, report["launch_reasons"])

    def test_legacy_only_without_bindings_cannot_enable_confirmation(self):
        values = fixtures(retired=False)
        values[1]["artifacts"][0]["sha256"] = LEGACY
        values[2]["processes"][0]["sha256"] = LEGACY
        report = check(values)
        self.assertTrue(report["launch_allowed"])
        self.assertFalse(report["confirmation_ready"])

    def test_inventory_must_be_complete_fresh_and_for_target(self):
        changes = [
            ("complete", False), ("target", "other"),
            ("observed_at", "2026-09-11T11:54:59Z"),
            ("observed_at", "2026-09-11T12:00:31Z"),
        ]
        for key, value in changes:
            values = fixtures()
            values[2][key] = value
            self.assertFalse(check(values)["launch_allowed"])

    def test_schema_and_future_binding_versions_fail_closed(self):
        values = fixtures()
        values[3]["schema_present"] = False
        self.assertFalse(check(values)["confirmation_ready"])
        values = fixtures("1")
        values[3]["gate_versions"] = ["crm-workspace-v2"]
        report = check(values)
        self.assertIn("binding_gate_unsupported", report["launch_reasons"])
        self.assertFalse(report["launch_allowed"])

    def test_boolean_counts_duplicates_roles_and_unknown_keys_are_rejected(self):
        variants = []
        values = fixtures(); values[3]["binding_count"] = True; variants.append(values)
        values = fixtures(); values[2]["pre_010c_retired"] = "true"; variants.append(values)
        values = fixtures(); values[2]["tenant_bypass"] = True; variants.append(values)
        values = fixtures(); values[2]["processes"].append(copy.deepcopy(values[2]["processes"][0])); variants.append(values)
        values = fixtures(); values[0]["artifacts"].append(copy.deepcopy(values[0]["artifacts"][0])); variants.append(values)
        values = fixtures(); values[0]["artifacts"][0]["gate_version"] = "unknown"; variants.append(values)
        values = fixtures(); values[0]["artifacts"][0]["role"] = []; variants.append(values)
        values = fixtures(); values[0]["artifacts"][0]["gate_version"] = {}; variants.append(values)
        values = fixtures(); values[3]["binding_count"] = "01"; variants.append(values)
        for values in variants:
            with self.assertRaises(EvidenceError):
                check(values)
        values = fixtures(); values[1]["artifacts"][0]["role"] = "cli"
        self.assertIn("candidate_unknown", check(values)["launch_reasons"])

    def test_exact_candidate_bytes_are_hashed_and_mutation_is_unknown(self):
        with tempfile.TemporaryDirectory() as folder:
            candidate = Path(folder) / "synthetic-binary"
            candidate.write_bytes(b"synthetic compatible artifact")
            payload = {"version": 1, "artifacts": [{"role": "api", "path": str(candidate)}]}
            result = MODULE["hash_candidates"](payload)
            digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
            self.assertEqual(result["artifacts"][0]["sha256"], digest)
            values = fixtures()
            values[0]["artifacts"][0]["sha256"] = digest
            values[2]["processes"][0]["sha256"] = digest
            self.assertTrue(check((values[0], result, values[2], values[3]))["confirmation_ready"])
            candidate.write_bytes(b"different bytes")
            result = MODULE["hash_candidates"](payload)
            self.assertFalse(check((values[0], result, values[2], values[3]))["launch_allowed"])
            candidate.chmod(0o666)
            with self.assertRaises(EvidenceError):
                MODULE["hash_candidates"](payload)

    def test_json_duplicate_keys_invalid_constants_and_writable_files_fail(self):
        for raw in [b'{"complete":true,"complete":false}', b'{"n":NaN}', b'{"n":Infinity}', b'\xff']:
            with self.assertRaises(EvidenceError):
                MODULE["decode_json"](raw)
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "inventory.json"
            path.write_text("{}")
            path.chmod(0o666)
            with self.assertRaises(EvidenceError):
                MODULE["operator_file"](path)

    def test_database_queries_are_read_only_no_state_bypass_or_lifecycle_filter(self):
        calls = []
        def fake_run(command, **kwargs):
            calls.append((command, kwargs))
            value = {"present": True, "database_name": "synthetic_only"} if len(calls) == 1 else fixtures("1")[3]
            return mock.Mock(returncode=0, stdout=json.dumps(value).encode())
        with mock.patch.object(MODULE["subprocess"], "run", side_effect=fake_run):
            result = MODULE["database_state"]()
        self.assertEqual(result["binding_count"], "1")
        self.assertEqual(len(calls), 2)
        sql = calls[1][0][-1]
        self.assertIn("FROM public.migration_workspace", sql)
        self.assertNotIn("WHERE", sql)
        self.assertNotIn("migration_import", sql)
        for command, kwargs in calls:
            self.assertIn("default_transaction_read_only=on", kwargs["env"]["PGOPTIONS"])
            self.assertIn("--no-password", command)
            self.assertEqual(kwargs["timeout"], 15)

    def test_database_error_does_not_echo_secret_stderr(self):
        with mock.patch.object(MODULE["subprocess"], "run", return_value=mock.Mock(returncode=1, stdout=b"", stderr=b"password=private")):
            with self.assertRaisesRegex(EvidenceError, "^database_unavailable$"):
                MODULE["database_state"]()

    def test_cli_outputs_distinct_launch_and_confirmation_results(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            candidate = base / "synthetic-binary"
            candidate.write_bytes(b"synthetic compatible artifact")
            artifacts, _, inventory, state = fixtures(retired=False)
            digest = hashlib.sha256(candidate.read_bytes()).hexdigest()
            artifacts["artifacts"][0]["sha256"] = digest
            inventory["processes"][0]["sha256"] = digest
            inventory["observed_at"] = MODULE["utc_text"](dt.datetime.now(dt.timezone.utc))
            candidates = {"version": 1, "artifacts": [{"role": "api", "path": str(candidate)}]}
            arguments = ["--target", "synthetic"]
            for name, value in [("artifacts", artifacts), ("candidates", candidates), ("inventory", inventory)]:
                path = base / (name + ".json")
                path.write_text(json.dumps(value))
                arguments.extend(["--" + name, str(path)])
            with mock.patch.dict(MODULE["main"].__globals__, {"database_state": lambda: state}):
                for purpose, expected in [("launch", 0), ("confirm", 1)]:
                    output = io.StringIO()
                    with contextlib.redirect_stdout(output):
                        status = MODULE["main"](arguments + ["--purpose", purpose])
                    self.assertEqual(status, expected)
                    report = json.loads(output.getvalue())
                    self.assertTrue(report["launch_allowed"])
                    self.assertFalse(report["confirmation_ready"])
                    self.assertEqual(report["purpose"], purpose)
                    self.assertEqual(report["candidates"][0]["sha256"], digest)
                    self.assertNotIn(str(candidate), output.getvalue())
                    self.assertEqual(len(report["inventory_sha256"]), 64)

    def test_cli_has_no_synthetic_database_state_override(self):
        arguments = ["--artifacts", "unused", "--candidates", "unused",
                     "--inventory", "unused", "--target", "synthetic", "--purpose", "confirm",
                     "--state-file", "cannot-bypass-live-query.json"]
        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit) as error:
                MODULE["main"](arguments)
        self.assertEqual(error.exception.code, 2)


if __name__ == "__main__":
    unittest.main()
