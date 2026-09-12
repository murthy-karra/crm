#!/usr/bin/env python3
"""Assert final constructed-fixture hashes and exact shared-byte settlement."""
import json, pathlib, hashlib, subprocess, uuid
QA=pathlib.Path('__PRIVATE_QA_ROOT__')
def read(name): return json.loads((QA/name).read_text())
b,p,f=[read('audit-'+name+'.json') for name in ('baseline','before-authority','final')]
def tables(v): return {r['table']:r for r in v['tables']}
bt,pt,ft=map(tables,(b,p,f)); assert bt.keys()==pt.keys()==ft.keys(); assert len(bt)==108
history={s for s in bt if s.startswith('migration_history_')}; assert len(history)==9
capture_allowed=history|{'migration_snapshot_storage'}
assert {s for s in bt if bt[s]!=pt[s]}==capture_allowed
intentional={'membership_changed','organization_membership','migration_connection','migration_request_receipt'}
assert {s for s in bt if bt[s]!=ft[s]}==capture_allowed|intentional
assert int(ft['membership_changed']['count'])-int(bt['membership_changed']['count'])==4
assert int(ft['migration_request_receipt']['count'])-int(bt['migration_request_receipt']['count'])==2
assert ft['organization_membership']['count']==bt['organization_membership']['count']=='12'
assert ft['migration_connection']['count']==bt['migration_connection']['count']=='3'
baseledger={r['organization_id']:r for r in b['organization_ledger']}; ledger=[]
for r in f['organization_ledger']:
    org=r['organization_id']; runs=[v for v in f['history_runs'] if v['organization_id']==org]
    retained=sum(int(v['retained_bytes']) for v in runs); reserved=sum(int(v['reserved_bytes']) for v in runs)
    assert r['retained_bytes']-baseledger[org]['retained_bytes']==retained
    assert r['reserved_bytes']-baseledger[org]['reserved_bytes']==reserved
    for v in runs:
        assert int(v['reserved_bytes'])==(8192 if v['state']=='paused' else 0)
    ledger.append({'organization_id':org,'history_retained_bytes':retained,'history_reserved_bytes':reserved,'retained_delta_exact':True,'reserved_delta_exact':True})
assert len(f['history_runs'])==9
assert int(ft['migration_history_observation']['count'])==1509
assert int(ft['migration_history_identity']['count'])==1508
assert int(ft['migration_history_capture']['count'])==38
# All six original source-run counters remain byte-for-byte identical through
# later credential/membership/disconnect checks; new runs are separately named.
finalruns={v['id']:v for v in f['history_runs']}
assert all(finalruns[v['id']]==v for v in p['history_runs'])
ctx=read('people-context.json')['organizations']; orgs=','.join("'"+str(uuid.UUID(v['id']))+"'::uuid" for v in ctx)
queries=["SELECT json_build_object('kind','members','rows',json_agg(json_build_object('organization_id',organization_id,'user_id',user_id,'role',role,'status',status))) FROM organization_membership WHERE organization_id IN ("+orgs+")",
"SELECT json_build_object('kind','connections','rows',json_agg(json_build_object('organization_id',organization_id,'status',status,'revision',revision,'has_credential',credential_ciphertext IS NOT NULL,'source_account_id',source_account_id))) FROM migration_connection WHERE organization_id IN ("+orgs+")",
"SELECT json_build_object('kind','role_changes','rows',json_agg(json_build_object('organization_id',organization_id,'user_id',user_id,'actor_user_id',actor_user_id,'reason',reason,'from_role',from_role,'to_role',to_role))) FROM membership_changed WHERE organization_id IN ("+orgs+") AND recorded_at >= '2026-09-12T17:53:39.122Z'::timestamptz",
"SELECT json_build_object('kind','credential_receipts','count',count(*)) FROM migration_request_receipt WHERE organization_id IN ("+orgs+") AND created_at >= '2026-09-12T17:48:38.692Z'::timestamptz"]
cmd=['docker','compose','-p','crm-010d1-qa','-f',str(QA/'compose.json'),'--env-file',str(QA/'qa.env'),'exec','-T','postgres','psql','-X','-qAt','-v','ON_ERROR_STOP=1','-U','crm_010d1_admin','-d','crm_010d1_qa']
r=subprocess.run(cmd,input='BEGIN ISOLATION LEVEL REPEATABLE READ READ ONLY;\n'+';\n'.join(queries)+';\nCOMMIT;',text=True,capture_output=True)
assert r.returncode==0, 'Read-only authority snapshot failed; no SQL output emitted'
authority={v['kind']:v for v in map(json.loads,r.stdout.splitlines())}
for org in ctx:
    expected={org['admin_id']:('admin','active'),org['helper_id']:('admin','active'),org['member_id']:('member','active'),org['former_id']:('member','inactive')}
    actual={v['user_id']:(v['role'],v['status']) for v in authority['members']['rows'] if v['organization_id']==org['id']}; assert actual==expected
    connection=next(v for v in authority['connections']['rows'] if v['organization_id']==org['id'])
    assert connection['source_account_id']==101
    assert (connection['status'],connection['revision'],connection['has_credential'])==({'complete':('disconnected',2,False),'cancel':('connected',3,True),'budget':('connected',1,True)}[org['case']])
changes=authority['role_changes']['rows']; assert len(changes)==4
complete=next(v for v in ctx if v['case']=='complete')
assert all(v['organization_id']==complete['id'] and v['user_id']==complete['admin_id'] and v['actor_user_id']==complete['helper_id'] for v in changes)
assert sorted(v['reason'] for v in changes)==['demote','demote','promote','promote']
assert authority['credential_receipts']['count']==2
out={'fixture':'constructed-010d1','tenant_tables_checked':108,'capture_only_unchanged_tables':98,'final_unchanged_tables':94,'history_tables':sorted(history),'intentional_authority_tables':sorted(intentional),'intentional_controls':{'role_events':4,'credential_replacement_receipts':2,'all_12_members_restored':True,'connection_states_exact':True},'original_six_history_run_counters_unchanged':True,'native_people_relationship_history_today_parent_sibling_workspace_unchanged':True,'ledger':ledger,'history_runs':9,'history_observations':1509,'history_identities':1508,'retained_captures':38,'paused_control_reservations':{'count':3,'bytes_each':8192},'completed_cancelled_reserved_bytes':0,'audit_sha256':{name:hashlib.sha256((QA/('audit-'+name+'.json')).read_bytes()).hexdigest() for name in ('baseline','before-authority','final')}}
(QA/'reconciliation-final.json').write_text(json.dumps(out,indent=2)+'\n')
print(json.dumps(out,indent=2))
