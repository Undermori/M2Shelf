"""Exercise the actual CLI, compare authored golden projections, and measure fresh-process peaks.
Only reads fixture inputs, executes this lab's CLI, and writes this task's new audit artifacts.
"""
import json
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
DOCS=ROOT/'docs/smart-mixed'
FIX=DOCS/'fixtures'
EXE=ROOT/'tools/smart-mixed-lab/target/release/m2shelf-smart-mixed-lab.exe'

def invoke(args=(),data=None):
    return subprocess.run([str(EXE),*args],input=None if data is None else json.dumps(data,ensure_ascii=False).encode(),capture_output=True,check=True)

def projection(plan):
    by_ref={u['source_ref']:u['path'] for u in plan['reading_units']}
    return dict(
        units=sorted([dict(path=u['path'],kind=u['kind'],pages=u['pages']) for u in plan['reading_units']],key=lambda u:u['path']),
        root_paths=sorted(r['path'] for r in plan['root_items']),
        series=sorted([dict(directory=g['directory'],title=g['title'],decision=g['decision'],members=sorted([dict(path=by_ref[m['source_ref']],volume=m['volume'],chapter=m['chapter'],role=m['role'],decision=m['decision']) for m in g['members']],key=lambda m:m['path'])) for g in plan['groups'] if g['kind']=='SERIES'],key=lambda g:g['title']),
        editions=len(plan['editions']),retained_prior=len(plan['retained_prior']))

checks=[]
for path in sorted(FIX.glob('[0-9][0-9].json')):
    c=json.loads(path.read_text(encoding='utf-8'));p=json.loads(invoke(data=c['input']).stdout)
    actual=projection(p); exp=c['expected']
    expected=dict(units=sorted(exp['units'],key=lambda u:u['path']),root_paths=sorted(exp['root_paths']),series=sorted([{**s,'members':sorted(s['members'],key=lambda m:m['path'])} for s in exp['series']],key=lambda g:g['title']),editions=exp['editions'],retained_prior=exp['retained_prior'])
    assert actual==expected,(path.name,actual,expected)
    for label in exp['diagnostics']:assert any(d['code']==label for d in p['diagnostics'])
    for name,role in exp['directory_roles'].items():assert any(d['path']==name and d['role']==role for d in p['directories'])
    checks.append(dict(id=c['id'],passed=True,units=len(p['reading_units']),root_items=len(p['root_items']),series=len(actual['series'])))

for path in sorted(FIX.glob('example-*.input.json')):
    result=invoke([str(path)])
    path.with_name(path.name.replace('.input.','.output.')).write_bytes(result.stdout)

benchmark=[]
for n in (1000,10000,100000):
    runs=[json.loads(invoke(['--benchmark',str(n)]).stdout) for _ in range(3)]
    assert all(r['reading_units']==n and r['groups']==n for r in runs)
    benchmark.append(dict(files=n,median_elapsed_ms=statistics.median(r['elapsed_ms'] for r in runs),max_peak_working_set_bytes=max(r['peak_working_set_bytes'] for r in runs),runs=runs))

invalid=subprocess.run([str(EXE)],input=b'{"not":"snapshot"}',capture_output=True)
assert invalid.returncode==2 and json.loads(invalid.stderr)['error']
out=dict(recorded_at_utc=datetime.now(timezone.utc).isoformat(),cli_golden_passed=len(checks),cli_golden_failed=0,checks=checks,benchmarks=benchmark,invalid_cli_exit=invalid.returncode,notes=['Synthetic caller-verified evidence; no decoder or production-library acceptance claim.','Fresh release process each run; elapsed covers recognize() only; peak includes process, input, output and regex initialization; no JSON serialization.'])
(DOCS/'verification.json').write_text(json.dumps(out,ensure_ascii=False,indent=2)+'\n',encoding='utf-8')
print(json.dumps(dict(cli_passed=len(checks),benchmarks=[{k:v for k,v in b.items() if k!='runs'} for b in benchmark]),indent=2))
