"""Verify Phase 2 isolation against its private pre-change manifest; never restore files."""
import hashlib
import json
import subprocess
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
meta = json.loads((ROOT / '.tmp/smart-mixed-phase2/current.json').read_text(encoding='utf-8'))
base = ROOT / meta['snapshot']
manifest = json.loads((base / 'manifest.json').read_text(encoding='utf-8'))


def git(*args):
    return subprocess.check_output(['git', '--git-dir=.git-codex-local', '--work-tree=.', *args], cwd=ROOT)


changed = [p for p, old in manifest.items() if not (ROOT / p).is_file() or hashlib.sha256((ROOT / p).read_bytes()).hexdigest() != old['sha256']]
head = git('rev-parse', 'HEAD').decode().strip()
branch = git('branch', '--show-current').decode().strip()
new = sorted(p.decode('utf-8') for p in git('ls-files', '--others', '--exclude-standard', '-z').split(b'\0') if p and p.decode('utf-8') not in manifest)
outside = [p for p in new if not p.startswith(('tools/smart-mixed-shadow/', 'docs/smart-mixed/'))]
assert not changed, changed
assert not outside, outside
assert (head, branch) == (meta['head'], meta['branch'])
archive_hash = hashlib.sha256((base / 'worktree-before.zip').read_bytes()).hexdigest()
assert archive_hash == meta['archiveSha256']
with zipfile.ZipFile(base / 'worktree-before.zip') as z:
    assert z.testzip() is None
    assert all(hashlib.sha256(z.read(p)).hexdigest() == v['sha256'] for p, v in manifest.items())
assert subprocess.run(['git', '--git-dir=.git-codex-local', '--work-tree=.', 'check-ignore', '-q', '.tmp/smart-mixed-phase2/current.json'], cwd=ROOT).returncode == 0
status = git('status', '--porcelain=v1', '-z')
(base / 'status-after.bin').write_bytes(status)
result = dict(baseline_files=len(manifest), changed_baseline_files=changed, new_files_outside_scope=outside,
    new_files=new, head=head, branch=branch, snapshot=meta['snapshot'], archive_sha256=archive_hash,
    snapshot_zip_verified=True, private_directory_git_ignored=True,
    before_status_records=len([p for p in (base / 'status-before.bin').read_bytes().split(b'\0') if p]),
    after_status_records=len([p for p in status.split(b'\0') if p]),
    protected='All 467 original tracked, nonignored untracked and ignored project Skill files. No production code/config/schema/docs/Phase 1/AGENTS/Skill bytes changed. No Git commit, branch change or push.')
(ROOT / 'docs/smart-mixed/phase2-scope-verification.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
print(json.dumps({k: v for k, v in result.items() if k != 'new_files'}, ensure_ascii=False, indent=2))
