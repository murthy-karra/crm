"""Post-release configuration comparison. Never emits values."""
import shlex,os
from release_ops import ROOT,RELEASE,values,save

def parse(path):
    out={}
    for line in path.read_text().splitlines():
        if not line.strip() or line.lstrip().startswith('#') or '=' not in line:continue
        k,v=line.split('=',1);parts=shlex.split(v,comments=True);out[k.strip().removeprefix('export ')]=parts[0] if parts else ''
    return out
before=parse(RELEASE/'env-before.private');after=values()
changed=[k for k in before.keys()|after.keys() if before.get(k)!=after.get(k)]
assert changed==['CRM_MIGRATION_RELEASE_REPORT']
assert after['CRM_MIGRATION_RELEASE_REPORT']==str(RELEASE/'release-report.json')
assert all(not after.get(k) and not os.environ.get(k) for k in ['CRM_FUB_SYSTEM_NAME','CRM_FUB_SYSTEM_KEY'])
assert (ROOT/'.env').stat().st_mode&0o077==0
save('configuration-check.json',{'changed_keys':changed,'all_other_configuration_values_preserved':True,'fub_system_name_and_key_unset':True,'configuration_mode':'0600','secret_values_emitted':False})
print('Only the protected compatibility-report path changed; registered FUB settings remain unset')
