"""Owned synthetic API prerequisites; no activity commands or direct DB writes."""
from pathlib import Path
import importlib.util, json, os, shlex, time, uuid
import requests
QA=Path('/private/tmp/crm-010f2-qa-694szdwe')
API='http://127.0.0.1:3017/api'
vals={}
for line in (QA/'qa.env').read_text().splitlines():
    k,_,v=line.partition('='); vals[k]=shlex.split(v)[0]
PASSWORD=vals['CRM_DEV_SEED_PASSWORD']
STATE=QA/'people-context.json'
state=json.loads(STATE.read_text()) if STATE.exists() else {'organizations':[]}
def save():
    tmp=STATE.with_suffix('.tmp'); tmp.write_text(json.dumps(state,indent=2)+'\n'); tmp.chmod(0o600); tmp.replace(STATE)
def req(session,method,path,data=None):
    r=session.request(method,API+path,json=data,timeout=30)
    if r.status_code not in (200,201,202,204):
        try: code=r.json().get('error',{}); code=code.get('code') if isinstance(code,dict) else 'error'
        except Exception: code='non-json'
        raise RuntimeError(f'{method} {path}: {r.status_code} {code}')
    return r.json() if r.content else None
def login(email):
    s=requests.Session(); req(s,'POST','/session',{'email':email,'password':PASSWORD}); return s
def uid(): return str(uuid.uuid4())
def wait(s,path,predicate,label,timeout=300):
    end=time.monotonic()+timeout
    while time.monotonic()<end:
        v=req(s,'GET',path)
        if predicate(v): return v
        obj=v.get('snapshot',v.get('preview',v))
        if obj.get('state') in ('paused','failed','cancelled','expired'):
            raise RuntimeError(f'{label} stopped: '+str(obj.get('pause_reason',obj.get('state'))))
        time.sleep(.4)
    raise RuntimeError(label+' timed out')
def accept(email,display,invitation):
    s=requests.Session(); req(s,'POST','/invitations/accept',{'token':invitation['accept_path'].rsplit('/',1)[1],'display_name':display,'password':PASSWORD}); return s
platform=login('owner@platform.test')
cases=[('complete','Acme Activity QA','alice@acme.test','Alice Anderson','carol@acme.test','Carol Chen'),('cancel','Best Activity QA','bob@best.test','Bob Baker','dave@best.test','Dave Diaz'),('budget','Budget Activity QA','elsa@budget.test','Elsa Ellis','fred@budget.test','Fred Foster')]
for key,name,email,display,member_email,member_name in cases:
    org=next((o for o in state['organizations'] if o['case']==key),None)
    if org is None:
        made=req(platform,'POST','/platform/organizations',{'name':name})['organization']
        invitation=req(platform,'POST',f"/platform/organizations/{made['id']}/invitations",{'email':email,'role':'admin'})
        admin=accept(email,display,invitation)
        req(admin,'GET','/me')
        invitation=req(admin,'POST','/organization/invitations',{'email':member_email,'role':'member'})
        member=accept(member_email,member_name,invitation)
        former_email=key+'-former@source.invalid'
        invitation=req(admin,'POST','/organization/invitations',{'email':former_email,'role':'member'})
        former=accept(former_email,'Synthetic Former Agent',invitation)
        former_id=req(former,'GET','/me')['user']['id']
        req(admin,'PUT',f'/organization/members/{former_id}/status',{'status':'inactive'})
        helper_email=key+'-helper@source.invalid'
        invitation=req(admin,'POST','/organization/invitations',{'email':helper_email,'role':'admin'})
        helper=accept(helper_email,'Synthetic Review Helper',invitation)
        org={'case':key,'id':made['id'],'admin_email':email,'admin_id':req(admin,'GET','/me')['user']['id'],'member_email':member_email,'member_id':req(member,'GET','/me')['user']['id'],'former_email':former_email,'former_id':former_id,'helper_email':helper_email,'helper_id':req(helper,'GET','/me')['user']['id']}
        state['organizations'].append(org); save()
    admin=login(email)
    if 'connection_id' not in org:
        c=req(admin,'POST','/migrations/fub/connections',{'request_id':uid(),'api_key':'synthetic-snapshot'})['connection']
        org.update(connection_id=c['id'],connection_revision=c['revision']); save()
    if 'snapshot_id' not in org:
        s=req(admin,'POST','/migrations/fub/snapshots',{'request_id':uid(),'connection_id':org['connection_id'],'expected_revision':org['connection_revision']})['snapshot']
        org['snapshot_id']=s['id']; save()
    current=req(admin,'GET',f"/migrations/fub/snapshots/{org['snapshot_id']}")['snapshot']
    if current['state']=='proposed':
        req(admin,'POST',f"/migrations/fub/snapshots/{org['snapshot_id']}/confirm",{'request_id':uid()})
    s=wait(admin,f"/migrations/fub/snapshots/{org['snapshot_id']}",lambda v:v['snapshot']['state'] in ('completed','completed_with_gaps'),key+' snapshot')
    if 'preview_id' not in org:
        p=req(admin,'POST',f"/migrations/fub/snapshots/{org['snapshot_id']}/previews",{'request_id':uid()})
        org['preview_id']=p['preview_id']; save()
    wait(admin,f"/migrations/fub/snapshots/{org['snapshot_id']}/previews/{org['preview_id']}",lambda v:v['preview']['state']=='completed',key+' preview')
    if 'parent_id' not in org:
        p=req(admin,'POST','/migrations/fub/imports',{'request_id':uid(),'snapshot_id':org['snapshot_id'],'preview_id':org['preview_id']})
        org['parent_id']=p['import_id']; save()
    path='/migrations/fub/imports/'+org['parent_id']
    parent=wait(admin,path,lambda v:v['state']=='completed' or (v['state']=='proposed' and v['plan']['state']=='ready'),key+' parent plan')
    if parent['state']!='completed':
        pid=parent['plan']['id']
        stages=req(admin,'GET',path+'/plans/'+pid+'/mappings?kind=stage&limit=50')['mappings']
        actors=req(admin,'GET',path+'/plans/'+pid+'/mappings?kind=assignee&limit=50')['mappings']
        # Explicit fixture choices only: preserve Lead via existing native stage;
        # Trash/unavailable Person remains held by the normal source policy.
        native_stages=req(admin,'GET','/stages')['stages']
        lead=next(s['id'] for s in native_stages if s['name']=='Lead')
        req(admin,'POST',path+'/plans',{'request_id':uid(),'expected_plan_revision':parent['plan']['revision'],'stage_mappings':[{'source_key':r['source_key'],'choice':{'kind':'existing','stage_id':lead} if r['source_key']=='11' else {'kind':'hold'}} for r in stages],'assignee_mappings':[{'source_key':r['source_key'],'choice':{'kind':'member','user_id':org['admin_id']} if r['source_key']=='7' else {'kind':'unassigned'}} for r in actors]})
        parent=wait(admin,path,lambda v:v['plan']['state']=='ready',key+' mapped parent')
        req(admin,'POST',path+'/confirm',{'request_id':uid(),'plan_id':parent['plan']['id'],'plan_revision':parent['plan']['revision'],'confirmation_digest':parent['plan']['confirmation_digest'],'acknowledgments':{'held_count':parent['plan']['counts']['held_people'],'review_only':True,'remaining_data':True}})
        parent=wait(admin,path,lambda v:v['state']=='completed',key+' parent import')
    rows=req(admin,'GET',path+'/results?limit=50')['results']
    org['people']={r['source_id']:r['person_id'] for r in rows if r['person_id']}
    org['parent_baseline']=parent; org['parent_results_baseline']=rows
    org['source_stats_after_parent']=json.loads((QA/'control.stats.json').read_text()); save()
    print(key+': completed retained snapshot and People parent through ordinary API',flush=True)
print('Synthetic activity prerequisites ready; no activity child prepared.',flush=True)
