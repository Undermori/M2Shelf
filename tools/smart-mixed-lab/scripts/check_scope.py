"""Check this task's original private manifest; never restore, delete, stage or mutate baseline files."""
import hashlib
import json
import subprocess
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
meta=json.loads((ROOT/'.tmp/smart-mixed-phase1/current.json').read_text(encoding='utf-8'))
base=ROOT/meta['snapshot']
manifest=json.loads((base/'manifest.json').read_text(encoding='utf-8'))
changed=[]
for rel,old in manifest.items():
    p=ROOT/rel
    if not p.is_file() or hashlib.sha256(p.read_bytes()).hexdigest()!=old['sha256']:changed.append(rel)
def git(*args):
    return subprocess.check_output(['git','--git-dir=.git-codex-local','--work-tree=.',*args],cwd=ROOT)
head=git('rev-parse','HEAD').decode().strip();branch=git('branch','--show-current').decode().strip()
new=sorted(p.decode('utf-8') for p in git('ls-files','--others','--exclude-standard','-z').split(b'\0') if p and p.decode('utf-8') not in manifest)
outside=[p for p in new if not p.startswith(('tools/smart-mixed-lab/','docs/smart-mixed/'))]
assert not changed,changed
assert not outside,outside
assert head==meta['head'] and branch==meta['branch']
assert hashlib.sha256((base/'worktree-before.zip').read_bytes()).hexdigest()==meta['archiveSha256']
before=(base/'status-before.bin').read_bytes()
status=git('status','--porcelain=v1','-z')
(base/'status-after.bin').write_bytes(status)
result=dict(baseline_files=len(manifest),changed_baseline_files=changed,new_files_outside_scope=outside,new_files=new,branch=branch,head=head,recovery=meta['snapshot'].replace('\\','/'),recovery_archive_sha256=meta['archiveSha256'],before_status_records=len([p for p in before.split(b'\0') if p]),after_status_records=len([p for p in status.split(b'\0') if p]),basis='Per-file SHA-256 of original tracked, nonignored untracked and project skill files; Git HEAD and branch unchanged. Existing dirty work was preserved. Full raw before/after status remains in private snapshot.')
(ROOT/'docs/smart-mixed/scope-verification.json').write_text(json.dumps(result,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps({k:v for k,v in result.items() if k!='new_files'},ensure_ascii=False,indent=2))
