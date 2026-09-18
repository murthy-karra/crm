"""Run a bounded large-pool Person-detail measurement in an owned E2E stack.

Usage: python3 -m e2e.load_person
"""
import argparse
import json
import re
import secrets
import shutil
import signal
import threading
import time
import uuid
from e2e import runner


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--duration-seconds', type=int, default=30,
                        help='duration of each 1-agent and 100-agent condition (default 30)')
    parser.add_argument('--agents', type=int, default=100)
    parser.add_argument('--pool-size', type=int, default=25000)
    parser.add_argument('--database-connections', type=int, default=10,
                        help='API SQLx maximum connection count (default 10)')
    parser.add_argument('--fixture', choices=['data-light', 'history-heavy'], default='data-light')
    parser.add_argument('--heavy-pool-size', type=int, default=1000,
                        help='random-access hot set when --fixture history-heavy (default 1000)')
    options = parser.parse_args()
    if not 2 <= options.duration_seconds <= 120:
        parser.error('--duration-seconds must be between 2 and 120')
    if not 1 <= options.agents <= 100:
        parser.error('--agents must be between 1 and 100')
    if not 1 <= options.pool_size <= 25000:
        parser.error('--pool-size must be between 1 and 25000')
    if not 1 <= options.database_connections <= 200:
        parser.error('--database-connections must be between 1 and 200')
    if options.fixture == 'history-heavy' and not 1 <= options.heavy_pool_size <= options.pool_size:
        parser.error('--heavy-pool-size must be between 1 and --pool-size')
    run_id = uuid.uuid4().hex[:12]
    project = f'crm-e2e-{run_id}-personload-1-a1'
    artifacts = runner.STATE / 'runs' / run_id / 'personload-1-a1'
    private = runner.STATE / 'private' / project
    artifacts.mkdir(parents=True, mode=0o700)
    private.mkdir(parents=True, mode=0o700)
    for sig in (signal.SIGINT, signal.SIGTERM):
        signal.signal(sig, lambda *_: runner.STOP.set())
    credentials = {key: secrets.token_hex(32) for key in ['app','root','migrator','audit','session','raw','password',
        'realtime_api','realtime_token','inbound','inference','livekit_key','livekit_secret']}
    images = runner.build(artifacts.parent, targets=['api','web','browser'])
    manifest = runner.config(project, private, artifacts, images, credentials, 'leads')
    manifest['services']['postgres']['command'] = ['postgres', '-c', 'shared_preload_libraries=pg_stat_statements']
    manifest['services']['api']['environment']['CRM_DATABASE_MAX_CONNECTIONS'] = str(options.database_connections)
    load = manifest['services']['browser']
    load['command'] = ['node', 'load-person.mjs']
    load['environment'].update({'E2E_LOAD_DURATION_SECONDS': str(options.duration_seconds),
                                'E2E_LOAD_AGENTS': str(options.agents),
                                'E2E_LOAD_POOL_SIZE': str(options.pool_size)})
    load['volumes'].append(f"{runner.ROOT / 'e2e/load-person.mjs'}:/e2e/load-person.mjs:ro")
    # Deliberately leave SQLx per-query profiling off during the latency test.
    runner.write_json(private / 'compose.json', manifest)
    runner.write_json(private / 'centrifugo.json', {'log': {'level':'info'}, 'health': {'enabled':True},
        'client': {'allowed_origins':['http://web:8080']}, 'channel': {'namespaces':[{'name':'org'}]}})
    for path in private.iterdir(): path.chmod(0o600)
    shutil.copyfile(runner.ROOT / 'e2e/load-person.mjs', artifacts / 'workload.mjs')
    cmd = runner.compose(project, private)
    def run(args, timeout=180, log='runner.log'):
        runner.command(cmd + args, artifacts / log, timeout=timeout)
    report = {'project':project, 'started_at':time.time(), 'status':'failed',
              'revision':runner.read_command(['git','rev-parse','HEAD']), 'images':images,
              'database_max_connections': options.database_connections,
              'scripts': {name: runner.hashlib.sha256((runner.ROOT/name).read_bytes()).hexdigest()
                          for name in ['e2e/load_person.py', 'e2e/load-person.mjs']},
              'docker_resources': runner.read_command(['docker','info','--format',
                  '{{.NCPU}} CPUs; {{.MemTotal}} bytes RAM; {{.Architecture}}; Docker {{.ServerVersion}}'])}
    monitor_stop = threading.Event()
    monitor = None
    try:
        run(['up','-d','--wait','postgres','centrifugo','mocks'])
        run(['exec','-T','postgres','psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d','crm','-c',
             'CREATE EXTENSION pg_stat_statements; GRANT pg_read_all_stats TO e2e_audit;'])
        run(['run','--rm','migrate'])
        run(['run','--rm','bootstrap'])
        run(['up','-d','--wait','api','web'])
        run(['run','--rm','seed'])
        # Only these disposable read-pool rows bypass commands. Creating 25k
        # rows through ReceiveInquiry would benchmark its serial intake lock,
        # instead of the requested Person-detail read path.
        pool_row = runner.read_command(cmd + ['exec','-T','postgres','psql','-X','-A','-t','-v','ON_ERROR_STOP=1',
            '-U','postgres','-d','crm','-c',
            "SELECT o.id || '|' || (SELECT id FROM stage WHERE organization_id=o.id AND position=1) || '|' || (SELECT id FROM stage WHERE organization_id=o.id AND position=2) || '|' || (SELECT user_id FROM organization_membership WHERE organization_id=o.id AND role='admin' ORDER BY user_id LIMIT 1) FROM organization o WHERE o.name='Journey Realty'"])
        org_id, stage_id, second_stage_id, admin_id = pool_row.split('|')
        if not all(re.fullmatch(r'[0-9a-f-]{36}', value) for value in [org_id, stage_id, second_stage_id, admin_id]):
            raise RuntimeError('large-pool fixture could not resolve organization/stage')
        pool_sql = "INSERT INTO person (organization_id,first_name,last_name,stage_id) SELECT '" + org_id + "'::uuid,'Load','Pool ' || n::text,'" + stage_id + "'::uuid FROM generate_series(1," + str(options.pool_size) + ") AS n"
        run(['exec','-T','postgres','psql','-X','-v','ON_ERROR_STOP=1',
             '-U','postgres','-d','crm','-c',pool_sql], timeout=120, log='pool-seed.log')
        expected_detail = {'contact_methods': 0, 'tags': 0, 'tasks': 0, 'history': 0}
        expected_state = {'people': options.pool_size, 'inquiries': 0, 'inquiry_facts': 0,
                          'stage_facts': 0, 'notes': 0, 'tasks': 0, 'contact_facts': 0}
        target_limit = options.pool_size
        if options.fixture == 'history-heavy':
            target_limit = options.heavy_pool_size
            heavy_sql = f"""
WITH heavy AS (SELECT id,row_number() OVER (ORDER BY id) AS n FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO contact_method(organization_id,person_id,kind,value,normalized_value)
SELECT '{org_id}'::uuid,h.id,CASE WHEN g=1 THEN 'email' ELSE 'phone' END,
       CASE WHEN g=1 THEN 'heavy-'||h.n||'@example.test' ELSE '+1202'||lpad((h.n*10+g)::text,7,'0') END,
       CASE WHEN g=1 THEN 'heavy-'||h.n||'@example.test' ELSE '+1202'||lpad((h.n*10+g)::text,7,'0') END
FROM heavy h CROSS JOIN generate_series(1,3) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO note(organization_id,person_id,author_user_id,body,origin,correlation_id,created_at,updated_at)
SELECT '{org_id}'::uuid,h.id,'{admin_id}'::uuid,'Production-shaped relationship note '||g||' with enough content to represent an ordinary agent interaction and follow-up context.','web_session',gen_random_uuid(),now()-(g||' days')::interval,now()-(g||' days')::interval
FROM heavy h CROSS JOIN generate_series(1,12) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO task(organization_id,person_id,title,kind,due_at,assignee_user_id,created_by_user_id,completed_at,completed_by_user_id,origin,correlation_id,created_at,updated_at)
SELECT '{org_id}'::uuid,h.id,'Production-shaped task '||g,CASE g%5 WHEN 0 THEN 'call' WHEN 1 THEN 'email' WHEN 2 THEN 'text' WHEN 3 THEN 'follow_up' ELSE 'other' END,now()+(g||' days')::interval,'{admin_id}'::uuid,'{admin_id}'::uuid,CASE WHEN g>12 THEN now()-(g||' hours')::interval END,CASE WHEN g>12 THEN '{admin_id}'::uuid END,'web_session',gen_random_uuid(),now()-(g||' days')::interval,now()-(g||' days')::interval
FROM heavy h CROSS JOIN generate_series(1,20) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO contact_attempted(organization_id,actor_kind,actor_user_id,on_behalf_of_user_id,origin,occurred_at,recorded_at,correlation_id,person_id,channel,outcome)
SELECT '{org_id}'::uuid,'user','{admin_id}'::uuid,'{admin_id}'::uuid,'web_session',now()-(g||' days')::interval,now()-(g||' days')::interval,gen_random_uuid(),h.id,CASE g%4 WHEN 0 THEN 'call' WHEN 1 THEN 'text' WHEN 2 THEN 'email' ELSE 'other' END,CASE g%4 WHEN 0 THEN 'reached' WHEN 1 THEN 'sent' WHEN 2 THEN 'left_message' ELSE 'no_answer' END
FROM heavy h CROSS JOIN generate_series(1,20) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO stage_changed(organization_id,actor_kind,actor_user_id,on_behalf_of_user_id,origin,occurred_at,recorded_at,correlation_id,person_id,from_stage_id,to_stage_id,reason)
SELECT '{org_id}'::uuid,'user','{admin_id}'::uuid,'{admin_id}'::uuid,'web_session',now()-((g+20)||' days')::interval,now()-((g+20)||' days')::interval,gen_random_uuid(),h.id,CASE WHEN g%2=0 THEN '{stage_id}'::uuid ELSE '{second_stage_id}'::uuid END,CASE WHEN g%2=0 THEN '{second_stage_id}'::uuid ELSE '{stage_id}'::uuid END,'load_fixture'
FROM heavy h CROSS JOIN generate_series(1,10) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO assignment_changed(organization_id,actor_kind,actor_user_id,on_behalf_of_user_id,origin,occurred_at,recorded_at,correlation_id,person_id,from_user_id,to_user_id,reason)
SELECT '{org_id}'::uuid,'user','{admin_id}'::uuid,'{admin_id}'::uuid,'web_session',now()-((g+30)||' days')::interval,now()-((g+30)||' days')::interval,gen_random_uuid(),h.id,NULL,'{admin_id}'::uuid,'load_fixture'
FROM heavy h CROSS JOIN generate_series(1,10) g;
INSERT INTO tag(organization_id,name,created_by_user_id) SELECT '{org_id}'::uuid,'Heavy Tag '||g,'{admin_id}'::uuid FROM generate_series(1,5) g;
WITH heavy AS (SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit})
INSERT INTO person_tag(organization_id,person_id,tag_id,added_by_user_id) SELECT '{org_id}'::uuid,h.id,t.id,'{admin_id}'::uuid FROM heavy h CROSS JOIN tag t WHERE t.organization_id='{org_id}'::uuid AND t.name LIKE 'Heavy Tag %';
"""
            run(['exec','-T','postgres','psql','-X','-v','ON_ERROR_STOP=1','-U','postgres','-d','crm','-c',heavy_sql], timeout=180, log='heavy-seed.log')
            expected_detail = {'contact_methods': 3, 'tags': 5, 'tasks': 12, 'history': 60}
            expected_state.update({'stage_facts': target_limit * 10, 'notes': target_limit * 12,
                                   'tasks': target_limit * 20, 'contact_facts': target_limit * 20})
        rebuild_sql = f"SELECT crm_rebuild_person_detail_projection('{org_id}'::uuid,id) FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit}"
        run(['exec','-T','postgres','psql','-X','-v','ON_ERROR_STOP=1',
             '-U','postgres','-d','crm','-c',rebuild_sql], timeout=180, log='projection-seed.log')
        person_ids = runner.read_command(cmd + ['exec','-T','postgres','psql','-X','-A','-t','-v','ON_ERROR_STOP=1',
            '-U','postgres','-d','crm','-c',f"SELECT id FROM person WHERE organization_id='{org_id}'::uuid ORDER BY id LIMIT {target_limit}"]).splitlines()
        if len(person_ids) != target_limit or any(len(value) != 36 for value in person_ids):
            raise RuntimeError('large-pool fixture count or IDs did not match request')
        runner.write_json(private / 'load-pool.json', {'organization_id': org_id, 'person_ids': person_ids,
            'total_people': options.pool_size, 'target_pool_size': target_limit,
            'fixture_kind': options.fixture, 'expected_detail': expected_detail,
            'expected_state': expected_state})
        ids = runner.read_command(cmd + ['ps','-q','api','postgres']).split()
        def sample():
            with (artifacts / 'container-stats.jsonl').open('w') as stream:
                while not monitor_stop.is_set():
                    try:
                        stats = runner.read_command(['docker','stats','--no-stream','--format','{{json .}}',*ids])
                        for line in stats.splitlines():
                            stream.write(json.dumps({'at_ms':int(time.time()*1000), **json.loads(line)})+'\n')
                        stream.flush()
                    except Exception as error:
                        stream.write(json.dumps({'error':type(error).__name__})+'\n'); stream.flush()
                    monitor_stop.wait(.5)
        monitor = threading.Thread(target=sample, daemon=True); monitor.start()
        run(['run','--rm','browser'], timeout=150, log='load.log')
        report['status'] = 'completed'
    finally:
        monitor_stop.set()
        if monitor: monitor.join(timeout=35)
        report['finished_at'] = time.time()
        try:
            runner.command(cmd + ['logs','--no-color','--timestamps'], artifacts/'services.log', timeout=30, cancellable=False)
        finally:
            runner.owned_cleanup(project, private, artifacts/'cleanup.log')
            report['cleanup'] = 'verified_empty'
            runner.sanitize(artifacts, private, credentials)
            shutil.rmtree(private)
            runner.write_json(artifacts/'run.json',report)
        print(f"Load evidence: {artifacts}", flush=True)


if __name__ == '__main__':
    main()
