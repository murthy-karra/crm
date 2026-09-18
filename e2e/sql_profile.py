"""Summarize sanitized SQLx events from the real API during an E2E journey."""
from collections import Counter, defaultdict
import json
import math
from pathlib import Path


def summarize(artifacts):
    artifacts = Path(artifacts)
    events = []
    for line in (artifacts / 'services.log').read_text().splitlines():
        if 'CRM_SQL_PROFILE {' in line:
            events.append(json.loads(line.split('CRM_SQL_PROFILE ', 1)[1]))
    steps = json.loads((artifacts / 'steps.json').read_text())
    executed = [step for step in steps if 'started' in step and 'finished' in step]
    if not executed:
        raise ValueError('SQL profile has no executed journey steps')
    start = min(step['started'] for step in executed)
    end = max(step['finished'] for step in executed)
    requests = {event['request_id']: {**event, 'query_count': 0, 'query_elapsed_ms': 0,
                                     'categories': Counter()}
                for event in events if event['kind'] == 'request'}
    background = []
    queries = [event for event in events if event['kind'] == 'query']
    if not requests or not queries:
        raise ValueError('SQL profile missing API request/query events')
    unattributed = 0
    for event in queries:
        request = requests.get(event['request_id'])
        if request is not None:
            request['query_count'] += 1
            request['query_elapsed_ms'] += event['elapsed_ms'] or 0
            request['categories'][event['category']] += 1
        elif event['request_id'] is None:
            background.append(event)
        else:
            unattributed += 1
    journey = [request for request in requests.values()
               if start <= request['started_ms'] <= end and not request['route'].startswith('/internal/')]
    if not journey or not any(request['query_count'] for request in journey):
        raise ValueError('SQL profile has no queries correlated to journey HTTP requests')
    groups = defaultdict(list)
    for request in journey:
        groups[request['method'] + ' ' + request['route']].append(request)

    def aggregate(rows):
        counts = sorted(row['query_count'] for row in rows)
        return {'requests': len(rows), 'queries': sum(counts),
                'min': min(counts, default=0), 'max': max(counts, default=0),
                'p50': counts[math.ceil(len(counts) * .5) - 1] if counts else 0,
                'p95': counts[math.ceil(len(counts) * .95) - 1] if counts else 0,
                'query_elapsed_ms': round(sum(row['query_elapsed_ms'] for row in rows), 2)}

    endpoints = [{'endpoint': endpoint, **aggregate(rows)} for endpoint, rows in groups.items()]
    endpoints.sort(key=lambda row: row['max'], reverse=True)
    categories = Counter()
    for request in journey:
        categories.update(request['categories'])
    background = [event for event in background if start <= event['at_ms'] <= end]
    report = {
        'metric': 'SQLx QueryLogger executions; not exact PostgreSQL wire round trips',
        'limitations': [
            'SQLx 0.8.6 omits BEGIN/savepoint creation and drop-triggered rollback from QueryLogger.',
            'A query event may represent a multi-statement execution or a failed/cancelled attempt.',
            'Elapsed time is client-observed execution time, not isolated server CPU or network latency.',
            'Seed/startup and health requests are excluded; request starts determine journey/step attribution.',
            'Unscoped work includes background workers and pool maintenance; it is not all business work.',
            'Small synthetic E2E fixtures are coverage evidence, not production capacity benchmarks.',
        ],
        'window': {'started_ms': start, 'finished_ms': end},
        'journey_http': aggregate(journey), 'categories': dict(categories),
        'background_queries': len(background),
        'background_query_elapsed_ms': round(sum(event['elapsed_ms'] or 0 for event in background), 2),
        'queries_with_missing_request_record': unattributed,
        'endpoints': endpoints,
        'steps': [{'name': step['name'], **aggregate([request for request in journey
                    if step['started'] <= request['started_ms'] < step['finished']]),
                   'background_queries': sum(step['started'] <= event['at_ms'] < step['finished']
                                             for event in background)} for step in executed],
        'requests': sorted(journey, key=lambda row: row['query_count'], reverse=True),
    }
    (artifacts / 'sql-profile.json').write_text(json.dumps(report, indent=2) + '\n')
    lines = ['# E2E SQL execution profile', '', report['metric'], '',
             f"Journey HTTP: {len(journey)} requests, {report['journey_http']['queries']} executions; "
             f"maximum {report['journey_http']['max']} in one request. Background: {len(background)} executions.", '',
             '| Endpoint | Requests | Total executions | Median/request | p95/request | Max/request |',
             '|---|---:|---:|---:|---:|---:|']
    for row in endpoints:
        lines.append(f"| {row['endpoint']} | {row['requests']} | {row['queries']} | {row['p50']} | {row['p95']} | {row['max']} |")
    lines += ['', *['- ' + limitation for limitation in report['limitations']], '']
    (artifacts / 'sql-profile.md').write_text('\n'.join(lines))
    return report['journey_http']
