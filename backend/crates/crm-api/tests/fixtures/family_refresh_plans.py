#!/usr/bin/env python3
"""One isolated 010g1 plan-shape slot after real synthetic Web acceptance.

Usage: python3 family_refresh_plans.py /absolute/fixture.json /absolute/output /absolute/query-inventory.json
The browser fixture must be paused. All inert volume is transaction-local and
rolled back. Ciphertext copies are never decrypted, executed, or fidelity proof.
Only the Docker development migrator is used; credentials never appear in argv.
"""
import hashlib
import json
import pathlib
import re
import subprocess
import sys

repo = pathlib.Path(__file__).resolve().parents[5]
fixture = json.loads(pathlib.Path(sys.argv[1]).read_text())
out = pathlib.Path(sys.argv[2])
assert out.is_absolute() and not out.exists()
out.mkdir(parents=True)
database = fixture['database']
assert re.fullmatch(r'[A-Za-z0-9_-]+', database)

def psql(sql):
    result = subprocess.run(['docker','compose','-f','infra/development/compose.yaml','--env-file','.env','exec','-T','postgres','sh','-c',
        'psql -X -qAt -v ON_ERROR_STOP=1 -U "$POSTGRES_USER" -d "$1"','psql',database],cwd=repo,input=sql,text=True,capture_output=True)
    if result.returncode:
        (out/'failure.log').write_text(result.stderr)
        raise RuntimeError(f'isolated plan fixture failed; see {out}/failure.log')
    return result.stdout

def quote(value):
    if value is None: return 'NULL'
    if isinstance(value,dict): return value['sql']
    if isinstance(value,int): return str(value)
    return "'"+str(value).replace("'","''")+"'"

org=fixture['organization']
roots=json.loads(psql("SELECT json_build_object('bundle',b.id,'payer',b.payer_plan_id,'plans',(SELECT json_object_agg(family,id) FROM migration_family_refresh_plan WHERE bundle_id=b.id AND state<>'superseded'),'person',(SELECT person_id FROM migration_family_refresh_cohort WHERE bundle_id=b.id LIMIT 1),'facts',(SELECT json_object_agg(family,original_fact_id) FROM migration_family_refresh_history_head WHERE bundle_id=b.id AND version=2),'identities',(SELECT json_object_agg(family,identity_id) FROM migration_family_refresh_history_head WHERE bundle_id=b.id AND version=2),'source',(SELECT id FROM migration_family_refresh_source WHERE bundle_id=b.id LIMIT 1),'cohort',(SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=b.id LIMIT 1)) FROM migration_family_refresh_bundle b WHERE b.organization_id="+quote(org)+" AND (SELECT count(*) FROM migration_family_refresh_plan p WHERE p.bundle_id=b.id AND p.state<>'superseded')=3 ORDER BY b.created_at LIMIT 1;"))
bundle=roots['bundle']
plans=roots['plans']
source_dir=repo/'backend/crates/crm-app/src/domain/migration/family_refresh'

def statement(file,prefix):
    text=(source_dir/file).read_text()
    candidates=[]
    for match in re.finditer(r'"(?:[^"\\]|\\.)*"',text):
        try: sql=json.loads(match.group().replace('\n','\\n'))
        except json.JSONDecodeError: continue
        if sql.startswith(prefix): candidates.append(sql)
    assert candidates and len(set(candidates))==1,(file,prefix)
    return candidates[0]

checks=[]
def add(label,file,prefix,args):
    sql=statement(file,prefix)
    assert not re.search(r'\{[A-Za-z_]',sql),sql
    bound=re.sub(r'\$(\d+)',lambda m:quote(args[int(m[1])-1]),sql)
    checks.append({'name':label,'file':file,'sql':sql,'sha256':hashlib.sha256(sql.encode()).hexdigest(),'bindings':args,'bound':bound})

for family in ['metadata','activity','history']:
    plan=plans[family]
    add(f'{family}-source-walk',family+'_walk.rs','SELECT s.*,owner.revision AS source_revision',[bundle,org,None])
    add(f'{family}-preview-page','item_queries.rs','SELECT id,position,kind,cohort_id',[bundle,plan,org,12500,50000,None,None,51])
    add(f'{family}-preview-held','item_queries.rs','SELECT id,position,kind,cohort_id',[bundle,plan,org,12500,50000,None,'held',51])
    add(f'{family}-preview-person','item_queries.rs','SELECT id,position,kind,cohort_id',[bundle,plan,org,0,50000,roots['cohort'],None,51])
    add(f'{family}-results','result_queries.rs','SELECT r.id,r.manifest_id,m.position',[bundle,plan,org,6000,50000,None,None,51])
    add(f'{family}-results-held','result_queries.rs','SELECT r.id,r.manifest_id,m.position',[bundle,plan,org,6000,50000,None,'held',51])
    add(f'{family}-execution-unit','execution.rs','SELECT * FROM migration_family_refresh_manifest WHERE plan_id=',[plan,org,12500])
    if family!='history':
        add(f'{family}-mapping-page','mapping_queries.rs','SELECT m.*,parent.source_key_hmac',[bundle,plan,org,None,'ffffffff-ffff-ffff-ffff-ffffffffffff',None,None,51])
add('cohort-walk','metadata_walk.rs','SELECT id FROM migration_family_refresh_cohort WHERE bundle_id=$1 AND organization_id=$2 AND ($3::uuid',[bundle,org,None])
add('family-summary','queries.rs','SELECT id,family,revision,state,phase,pause_reason,digest',[bundle,org])
add('remainder-source-reference','remainder_copy.rs','SELECT c.id FROM migration_family_refresh_source c JOIN',[bundle,org,roots['source']])
add('remainder-cohort-reference','remainder_copy.rs','SELECT c.id FROM migration_family_refresh_cohort c JOIN',[bundle,org,roots['cohort']])
for item in json.loads(pathlib.Path(sys.argv[3]).read_text()):
    family={'fub_event_record_imported':'events','fub_call_record_imported':'calls','fub_text_record_imported':'text_messages'}[item['kind']]
    if item['shape']=='page': args=[org,roots['person'],None,None,None,None,None,51,None]
    elif item['shape']=='after': args=[org,roots['person'],'2026-01-01T00:00:00Z','2027-01-01T00:00:00Z',0,'ffffffff-ffff-ffff-ffff-ffffffffffff',item['kind'],51,99999]
    elif item['shape']=='entry': args=[org,roots['person'],roots['facts'][family]]
    elif item['shape']=='version-first': args=[org,roots['person'],roots['identities'][family]]
    elif item['shape']=='version-page': args=[org,roots['person'],roots['identities'][family],9223372036854775807,51]
    elif item['shape']=='version-detail': args=[org,roots['person'],roots['identities'][family],2]
    else: raise AssertionError(item['shape'])
    query=item['sql']
    bound=re.sub(r'\$(\d+)',lambda m:quote(args[int(m[1])-1]),query)
    checks.append({'name':item['name'],'file':'history_review.rs','sql':query,'sha256':hashlib.sha256(query.encode()).hexdigest(),'bindings':args,'bound':bound})


# Hot preparation reconciliation and scoped destination shapes. Per-Person native
# arrays are contract-bounded; relation cardinality is independent of those caps.
for family,kind in [('metadata','person'),('activity','task')]:
    add(f'{family}-source-resolution','core_resolution.rs','SELECT s.representation,count(*)',
        [bundle,org,roots['payer'],kind,'101' if kind=='person' else '21'])
    representation={'sql':f"(SELECT representation FROM migration_family_refresh_source WHERE bundle_id={quote(bundle)} AND kind={quote(kind)} LIMIT 1)"}
    add(f'{family}-source-choice','core_resolution.rs','SELECT * FROM migration_family_refresh_source WHERE bundle_id=',
        [bundle,org,roots['payer'],kind,'101' if kind=='person' else '21',representation])
add('history-source-resolution','history_resolution.rs','SELECT count(*) AS observations',[bundle,org,'event','1'])
add('history-source-choice','history_resolution.rs','SELECT s.*,p.revision AS source_revision',[bundle,org,'event','1'])
add('metadata-native-state','metadata_baseline.rs',"SELECT jsonb_build_object('person'",[org,roots['person']])
for label,prefix in [('tag','SELECT id,name AS label'),('field','SELECT id,label,field_type'),('option','SELECT o.id,o.label'),('author','SELECT u.id,u.display_name AS label,NULL::text AS field_type,m.status FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND ($2'),('assignee',"SELECT u.id,u.display_name AS label,NULL::text AS field_type,m.status FROM organization_membership m JOIN app_user u ON u.id=m.user_id WHERE m.organization_id=$1 AND m.status='active'")]:
    target={'sql':f"(SELECT id FROM custom_field WHERE organization_id={quote(org)} LIMIT 1)"} if label=='option' else None
    add('mapping-target-'+label,'mapping_queries.rs',prefix,[org,None,target,51])
for family in ['metadata','activity']:
    add(f'{family}-mapping-upper','mapping_queries.rs','SELECT id FROM migration_family_refresh_mapping WHERE plan_id=',[plans[family],org,None,None])
add('catalog-pending-targets','metadata_destination.rs','SELECT m.*,p.source_key_hmac AS parent_key',[plans['metadata'],org,'tag',None,201])
# Explain the exact internal statements of each remainder step, rather than an
# opaque Function Scan. Only PL/pgSQL INTO/local bindings are substituted.
migration=(repo/'backend/crates/crm-api/migrations/20261008000032_fub_family_refresh_remainder.sql').read_text()
body=migration.split('CREATE FUNCTION crm_family_refresh_remainder_next',1)[1].split('RETURN answer;',1)[0]
for number,match in enumerate(re.finditer(r'SELECT (?:u\.)?id INTO answer FROM.*?;',body,re.S)):
    raw=match.group()[:-1]
    for family in ['metadata','history']:
        bound=raw.replace(' INTO answer','')
        replacements={'source.bundle_id':quote(bundle),'source.id':quote(plans[family]),'source.apply_position':'12500','p.organization_id':quote(org),'p.family':quote(family),'p.remainder_after':'NULL::uuid','p.remainder_stage':'4'}
        for key,value in replacements.items():bound=bound.replace(key,value)
        checks.append({'name':f'remainder-step-{number}-{family}','file':'20261008000032_fub_family_refresh_remainder.sql','sql':raw,'sha256':hashlib.sha256(raw.encode()).hexdigest(),'bindings':replacements,'bound':bound})


# SQL-language walkers inline into EXPLAIN, retaining their complete joins.
for name,call in [
 ('owned-history',f"crm_family_refresh_next_owned_history({quote(org)},{quote(bundle)},NULL)"),
 ('owned-activity',f"crm_family_refresh_next_owned_activity({quote(org)},{quote(bundle)},NULL,NULL)"),
 ('catalog-walk',f"crm_family_refresh_next_catalog_mapping({quote(org)},{quote(bundle)},{quote(plans['metadata'])},NULL)")]:
    query='SELECT * FROM '+call
    checks.append({'name':name,'file':'database SQL-language function','sql':query,'sha256':hashlib.sha256(query.encode()).hexdigest(),'bindings':[],'bound':query})

sql=['BEGIN;', (repo/'backend/crates/crm-api/migrations/20261008000034_fub_family_refresh_review_indexes.sql').read_text().replace('CREATE INDEX ', 'CREATE INDEX IF NOT EXISTS '),"SET LOCAL statement_timeout='300s';","SET LOCAL lock_timeout='10s';",'CREATE TEMP TABLE family_plan_evidence(name text,plan jsonb);']
# Inert cardinality copies are not mutation/fidelity evidence. Suppress row
# triggers, including FK triggers, only while filling this rolled-back fixture;
# indexes and CHECK/UNIQUE constraints remain, and production reads run normally.
sql.append("SET LOCAL session_replication_role=replica;")
tables=['cohort','source','mapping','manifest','result']
for table in tables: sql.append(f'ALTER TABLE migration_family_refresh_{table} DISABLE TRIGGER USER;')
sql.append("CREATE TEMP TABLE family_perf_people AS SELECT g,gen_random_uuid() AS id,gen_random_uuid() AS cohort FROM generate_series(1,24999)g;")
sql.append(f"INSERT INTO person(id,organization_id,first_name,last_name,stage_id,assigned_user_id) SELECT v.id,{quote(org)},'Synthetic scale',v.g::text,p.stage_id,p.assigned_user_id FROM family_perf_people v CROSS JOIN (SELECT * FROM person WHERE organization_id={quote(org)} LIMIT 1)p;")
sql.append(f"INSERT INTO app_user(id,email,display_name) SELECT gen_random_uuid(),'family-perf-'||g||'@synthetic.test','Performance member '||g FROM generate_series(1,48)g;")
sql.append(f"INSERT INTO organization_membership(organization_id,user_id,role,status) SELECT {quote(org)},id,'member','active' FROM app_user WHERE email LIKE 'family-perf-%@synthetic.test';")
sql.append(f"INSERT INTO migration_family_refresh_cohort SELECT (jsonb_populate_record(NULL::migration_family_refresh_cohort,to_jsonb(c)||jsonb_build_object('id',v.cohort,'person_id',v.id,'source_person_id',(1000000+v.g)::text,'remainder_source_id',NULL))).* FROM family_perf_people v CROSS JOIN (SELECT * FROM migration_family_refresh_cohort WHERE bundle_id={quote(bundle)} LIMIT 1)c;")
for family in ['metadata','activity','history']:
    plan=plans[family]
    sql.append(f"CREATE TEMP TABLE family_perf_{family} AS SELECT v.*,gen_random_uuid() AS source,gen_random_uuid() AS manifest,gen_random_uuid() AS result,gen_random_uuid() AS identity,gen_random_uuid() AS fact,gen_random_uuid() AS version FROM family_perf_people v;")
    kind={'metadata':'person','activity':'task','history':'event'}[family]
    sql.append(f"INSERT INTO migration_family_refresh_source SELECT (jsonb_populate_record(NULL::migration_family_refresh_source,to_jsonb(s)||jsonb_build_object('id',v.source,'ordinal',10000+v.g,'source_id',(1000000+v.g)::text,'source_person_id',(1000000+v.g)::text,'identity_hmac',chr(92)||'x'||encode(sha256(convert_to(v.g::text,'UTF8')),'hex'),'remainder_source_id',NULL))).* FROM family_perf_{family} v CROSS JOIN (SELECT * FROM migration_family_refresh_source WHERE bundle_id={quote(bundle)} AND kind={quote(kind)} LIMIT 1)s;")
    sql.append(f"INSERT INTO migration_family_refresh_manifest SELECT (jsonb_populate_record(NULL::migration_family_refresh_manifest,to_jsonb(m)||jsonb_build_object('id',v.manifest,'position',100+v.g,'cohort_id',v.cohort,'person_id',v.id,'source_row_id',v.source,'source_id',(1000000+v.g)::text,'source_key_hmac',chr(92)||'x'||encode(sha256(convert_to(v.g::text,'UTF8')),'hex'),'disposition',CASE WHEN v.g%97=0 THEN 'held' ELSE m.disposition END,'remainder_source_id',NULL,'inherited_result_id',NULL))).* FROM family_perf_{family} v CROSS JOIN (SELECT * FROM migration_family_refresh_manifest WHERE plan_id={quote(plan)} AND kind<>'catalog' LIMIT 1)m;")
    result_filter=' WHERE v.g<=12500' if family=='activity' else ''
    sql.append(f"UPDATE migration_family_refresh_plan SET position=25100,apply_position={12600 if family=='activity' else 25100} WHERE id={quote(plan)};")
    sql.append(f"INSERT INTO migration_family_refresh_result SELECT (jsonb_populate_record(NULL::migration_family_refresh_result,to_jsonb(r)||jsonb_build_object('id',v.result,'bundle_id',{quote(bundle)},'plan_id',{quote(plan)},'manifest_id',v.manifest,'person_id',v.id,'disposition',CASE WHEN v.g%97=0 THEN 'held' ELSE 'applied' END))).* FROM family_perf_{family} v CROSS JOIN (SELECT * FROM migration_family_refresh_result WHERE organization_id={quote(org)} LIMIT 1)r{result_filter};")
# Populate all three typed current-history readers, with 10% dense history on
# one Person and known/unknown corrected dates. Raw display copies stay opaque.
history_tables=['migration_history_import_identity','migration_family_refresh_history_display','migration_family_refresh_history_head']+[f'fub_{stem}_record_{suffix}' for stem in ['event','call','text'] for suffix in ['imported','corrected']]
for table in history_tables: sql.append(f'ALTER TABLE {table} DISABLE TRIGGER USER;')
for modulo,(stem,family) in enumerate([('event','events'),('call','calls'),('text','text_messages')]):
    person=f"CASE WHEN v.g%10=0 THEN {quote(roots['person'])}::uuid ELSE v.id END"
    dated="CASE WHEN v.g%7=0 THEN NULL ELSE '2026-01-01T00:00:00Z'::timestamptz END"
    basis="CASE WHEN v.g%7=0 THEN 'unknown' ELSE 'fub_record_created' END"
    sql.append(f"INSERT INTO migration_history_import_identity SELECT (jsonb_populate_record(NULL::migration_history_import_identity,to_jsonb(i)||jsonb_build_object('id',v.identity,'identity_hmac',chr(92)||'x'||encode(sha256(convert_to(v.g::text,'UTF8')),'hex'),'person_id',{person},'fact_id',v.fact,'family',{quote(family)}))).* FROM family_perf_history v CROSS JOIN (SELECT * FROM migration_history_import_identity WHERE organization_id={quote(org)} AND family='events' LIMIT 1)i WHERE v.g%3={modulo};")
    sql.append(f"INSERT INTO fub_{stem}_record_imported SELECT (jsonb_populate_record(NULL::fub_{stem}_record_imported,to_jsonb(f)||jsonb_build_object('id',v.fact,'identity_id',v.identity,'person_id',{person},'stable_position',100+v.g))).* FROM family_perf_history v CROSS JOIN (SELECT * FROM fub_event_record_imported WHERE organization_id={quote(org)} LIMIT 1)f WHERE v.g%3={modulo};")
    sql.append(f"INSERT INTO migration_family_refresh_history_display SELECT (jsonb_populate_record(NULL::migration_family_refresh_history_display,to_jsonb(d)||jsonb_build_object('id',v.version,'identity_id',v.identity,'result_id',v.result))).* FROM family_perf_history v CROSS JOIN (SELECT * FROM migration_family_refresh_history_display WHERE organization_id={quote(org)} LIMIT 1)d WHERE v.g%3={modulo};")
    sql.append(f"INSERT INTO fub_{stem}_record_corrected SELECT (jsonb_populate_record(NULL::fub_{stem}_record_corrected,to_jsonb(f)||jsonb_build_object('id',v.version,'identity_id',v.identity,'original_fact_id',v.fact,'person_id',{person},'result_id',v.result,'manifest_id',v.manifest,'source_created_at',{dated},'source_time_basis',{basis}))).* FROM family_perf_history v CROSS JOIN (SELECT * FROM fub_event_record_corrected WHERE organization_id={quote(org)} AND version=2 LIMIT 1)f WHERE v.g%3={modulo};")
    sql.append(f"INSERT INTO migration_family_refresh_history_head SELECT (jsonb_populate_record(NULL::migration_family_refresh_history_head,to_jsonb(h)||jsonb_build_object('identity_id',v.identity,'person_id',{person},'family',{quote(family)},'original_fact_id',v.fact,'version_id',v.version,'result_id',v.result,'source_created_at',{dated}))).* FROM family_perf_history v CROSS JOIN (SELECT * FROM migration_family_refresh_history_head WHERE organization_id={quote(org)} AND version=2 LIMIT 1)h WHERE v.g%3={modulo};")
for table in history_tables: sql += [f'ALTER TABLE {table} ENABLE TRIGGER USER;',f'ANALYZE {table};']
for table in tables:
    sql += [f'ALTER TABLE migration_family_refresh_{table} ENABLE TRIGGER USER;',f'ANALYZE migration_family_refresh_{table};']
sql += ['ANALYZE person;','ANALYZE organization_membership;']
sql.append("SET LOCAL session_replication_role=origin;")
sql.append("SET LOCAL statement_timeout='60s';")
for check in checks:
    sql.append("DO $e$ DECLARE p jsonb; BEGIN EXECUTE $q$EXPLAIN (ANALYZE,BUFFERS,FORMAT JSON) "+check['bound']+"$q$ INTO p; INSERT INTO family_plan_evidence VALUES("+quote(check['name'])+",p); END $e$;")
sql.append("SELECT json_build_object('people',(SELECT count(*) FROM person WHERE organization_id="+quote(org)+"),'members',(SELECT count(*) FROM organization_membership WHERE organization_id="+quote(org)+"),'plans',(SELECT json_object_agg(name,plan) FROM family_plan_evidence));")
sql.append('ROLLBACK;')
(out/'runner.sql').write_text('\n'.join(sql))
result=json.loads(psql('\n'.join(sql)))
for check in checks: check.pop('bound')
result['statements']=checks
(out/'plans.json').write_text(json.dumps(result,indent=2))
assert result['people']==25000 and result['members']==50,result
def nodes(plan):
    yield plan
    for child in plan.get('Plans',[]):
        yield from nodes(child)
required_indexes={'family-summary':'family_refresh_remainder_eligible_tail'}
for family in ['metadata','activity','history']:
    required_indexes[family+'-preview-person']='family_refresh_manifest_cohort_page'
    required_indexes[family+'-results-held']='family_refresh_result_outcome'
for name,index in required_indexes.items():
    assert any(node.get('Index Name')==index for node in nodes(result['plans'][name][0]['Plan'])),name
for name,plans in result['plans'].items():
    if name.startswith('fub_'):
        assert plans[0]['Plan']['Actual Rows']>0,name
print(f"Retained {len(checks)} exact statement plans at {out}/plans.json; inspect shapes before accepting.")
