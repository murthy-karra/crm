#!/usr/bin/env python3
"""Read-only row hashes for the isolated synthetic 010d1 QA database.

No row payload, key, credential, source body or ciphertext is emitted. The
coordinator runs this only after the backend hands off its DB test slot.
"""
import argparse
import hashlib
import json
import pathlib
import subprocess
import uuid

QA = pathlib.Path('__PRIVATE_QA_ROOT__')
BASE = ['docker', 'compose', '-p', 'crm-010d1-qa', '-f', str(QA / 'compose.json'),
        '--env-file', str(QA / 'qa.env'), 'exec', '-T', 'postgres', 'psql',
        '-X', '-qAt', '-v', 'ON_ERROR_STOP=1', '-U', 'crm_010d1_admin',
        '-d', 'crm_010d1_qa']


def sql(query):
    result = subprocess.run(BASE, input=query, text=True, capture_output=True)
    if result.returncode:
        # Error text can include SQL values: keep it private and don't print it.
        (QA / 'audit-retained-error.log').write_text(result.stderr)
        raise RuntimeError('Read-only QA audit failed; private error retained')
    return result.stdout.strip()


def quoted(name):
    return '"' + name.replace('"', '""') + '"'


parser = argparse.ArgumentParser()
parser.add_argument('name')
args = parser.parse_args()
assert args.name and all(ch.isalnum() or ch in '-_' for ch in args.name)
context = json.loads((QA / 'people-context.json').read_text())
orgs = [str(uuid.UUID(item['id'])) for item in context['organizations']]
org_sql = ','.join("'" + item + "'::uuid" for item in orgs)
assert sql('SELECT current_database()') == 'crm_010d1_qa'
tables = json.loads(sql("SELECT coalesce(json_agg(table_name ORDER BY table_name),'[]') FROM information_schema.columns WHERE table_schema='public' AND column_name='organization_id'"))
queries = []
for table in tables:
    assert '\x00' not in table
    queries.append("SELECT json_build_object('table', '" + table.replace("'", "''") +
        "', 'count', count(*)::text, 'row_set_sha256', encode(sha256(convert_to(coalesce(string_agg(row_hash, '' ORDER BY row_hash), ''),'UTF8')),'hex')) " +
        'FROM (SELECT encode(sha256(convert_to(to_jsonb(t)::text,\'UTF8\')),\'hex\') AS row_hash FROM ' +
        quoted(table) + ' AS t WHERE organization_id IN (' + org_sql + ')) AS hashed')
queries.append("SELECT json_build_object('table','organization','count',count(*)::text,'row_set_sha256',encode(sha256(convert_to(coalesce(string_agg(row_hash,'' ORDER BY row_hash),''),'UTF8')),'hex')) FROM (SELECT encode(sha256(convert_to(to_jsonb(t)::text,'UTF8')),'hex') row_hash FROM organization t WHERE id IN (" + org_sql + ')) hashed')
ledger_query = "SELECT json_build_object('kind','organization_ledger','rows',coalesce(json_agg(to_jsonb(t) ORDER BY organization_id),'[]')) FROM migration_snapshot_storage t WHERE organization_id IN (" + org_sql + ')'
runs_query = "SELECT json_build_object('kind','history_runs','rows',coalesce(json_agg(json_build_object('id',id,'organization_id',organization_id,'parent_import_id',parent_import_id,'state',state,'raw_bytes',raw_bytes::text,'retained_bytes',retained_bytes::text,'reserved_bytes',reserved_bytes::text,'capture_sequence',capture_sequence::text) ORDER BY id),'[]')) FROM migration_history_capture_run WHERE organization_id IN (" + org_sql + ')'
snapshot = [json.loads(line) for line in sql('BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\n' + ';\n'.join(queries + [ledger_query,runs_query]) + ';\nCOMMIT;').splitlines()]
rows = [row for row in snapshot if 'table' in row]
ledger = next(row['rows'] for row in snapshot if row.get('kind') == 'organization_ledger')
runs = next(row['rows'] for row in snapshot if row.get('kind') == 'history_runs')
result = {'fixture':'constructed-010d1', 'name':args.name, 'database':'crm_010d1_qa',
          'organizations':orgs, 'tables':rows, 'organization_ledger':ledger, 'history_runs':runs}
for filename in ['control.stats.json','control.history-stats.json','control.delivery-stats.json']:
    source = QA / filename
    if source.exists():
        value = json.loads(source.read_text())
        value.pop('requests', None)
        result[filename] = value
encoded = json.dumps(result, sort_keys=True, indent=2) + '\n'
output = QA / ('audit-' + args.name + '.json')
output.write_text(encoded)
output.chmod(0o600)
print(json.dumps({'name':args.name,'tenant_tables':len(rows),'sha256':hashlib.sha256(encoded.encode()).hexdigest()}))
