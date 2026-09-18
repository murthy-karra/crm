import json
from pathlib import Path
import tempfile
import unittest
from e2e.sql_profile import summarize


class SqlProfileTests(unittest.TestCase):
    def test_overlapping_requests_background_and_setup_are_separate(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'steps.json').write_text(json.dumps([{'name': 'step', 'started': 100, 'finished': 200}]))
            events = [dict(kind='request', request_id=id, method='GET', route='/api/people',
                           started_ms=start, finished_ms=180, elapsed_ms=10)
                      for id, start in [(1, 50), (2, 110), (3, 120)]]
            events += [dict(kind='query', request_id=id, at_ms=130, elapsed_ms=2, category='select')
                       for id in [1, 2, 3, 2, None]]
            (root / 'services.log').write_text('\n'.join('api | CRM_SQL_PROFILE ' + json.dumps(e) for e in events))
            result = summarize(root)
            self.assertEqual(result['queries'], 3)
            self.assertEqual(result['requests'], 2)
            self.assertEqual(result['max'], 2)
            report = json.loads((root / 'sql-profile.json').read_text())
            self.assertEqual(report['background_queries'], 1)
            self.assertEqual(report['queries_with_missing_request_record'], 0)
            self.assertEqual(report['steps'][0]['queries'], 3)

    def test_missing_telemetry_is_not_a_zero_query_success(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'steps.json').write_text('[{"started":1,"finished":2}]')
            (root / 'services.log').write_text('')
            with self.assertRaisesRegex(ValueError, 'missing API'):
                summarize(root)
