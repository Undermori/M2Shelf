"""Release CLI verification: synthetic current-schema SQLite only; reports stay private by default.
Run from any cwd after cargo test and cargo build --release --features fixtures.
"""
import hashlib
import json
import re
import statistics
import subprocess
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PRIVATE = ROOT / '.tmp/smart-mixed-phase2'
EXE = ROOT / 'tools/smart-mixed-shadow/target/release/m2shelf-smart-mixed-shadow.exe'
LAB = ROOT / 'tools/smart-mixed-lab/target/release/m2shelf-smart-mixed-lab.exe'


def invoke(exe, args=(), data=None):
    return subprocess.run([str(exe), *args], input=data, capture_output=True, check=True)


def projection(plan):
    by_ref = {u['source_ref']: u['path'] for u in plan['reading_units']}
    return dict(
        units=sorted([dict(path=u['path'], kind=u['kind'], pages=u['pages']) for u in plan['reading_units']], key=lambda u: u['path']),
        root_paths=sorted(r['path'] for r in plan['root_items']),
        series=sorted([dict(directory=g['directory'], title=g['title'], decision=g['decision'], members=sorted([dict(path=by_ref[m['source_ref']], volume=m['volume'], chapter=m['chapter'], role=m['role'], decision=m['decision']) for m in g['members']], key=lambda m: m['path'])) for g in plan['groups'] if g['kind'] == 'SERIES'], key=lambda g: g['title']),
        editions=len(plan['editions']), retained_prior=len(plan['retained_prior']))


def phase1():
    passed = []
    for p in sorted((ROOT / 'docs/smart-mixed/fixtures').glob('[0-9][0-9].json')):
        c = json.loads(p.read_text(encoding='utf-8'))
        plan = json.loads(invoke(LAB, data=json.dumps(c['input'], ensure_ascii=False).encode()).stdout)
        e = c['expected']
        expected = dict(units=sorted(e['units'], key=lambda u: u['path']), root_paths=sorted(e['root_paths']), series=sorted([{**s, 'members': sorted(s['members'], key=lambda m: m['path'])} for s in e['series']], key=lambda g: g['title']), editions=e['editions'], retained_prior=e['retained_prior'])
        assert projection(plan) == expected, p.name
        for code in e['diagnostics']:
            assert any(d['code'] == code for d in plan['diagnostics'])
        for path, role in e['directory_roles'].items():
            assert any(d['path'] == path and d['role'] == role for d in plan['directories'])
        passed.append(c['id'])
    return len(passed)


def make(case):
    return Path(json.loads(invoke(EXE, ['--make-fixture', str(case)]).stdout)['synthetic_db'])


def run(db):
    return json.loads(invoke(EXE, ['--shadow', str(db), '1', '--authorized-index']).stdout)


def main():
    PRIVATE.mkdir(parents=True, exist_ok=True)
    golden = phase1()
    cases = []
    for case in (3, 10, 11, 12, 20, 22, 28, 36, 42, 45, 49):
        db = make(case)
        before = hashlib.sha256(db.read_bytes()).hexdigest()
        result = run(db)
        assert result['summary']['missing_source_ids'] == 0
        assert result['summary']['duplicate_source_ids'] == 0
        assert before == hashlib.sha256(db.read_bytes()).hexdigest()
        # Deterministic summary; timing/OS telemetry deliberately excluded.
        assert result['summary'] == run(db)['summary']
        (PRIVATE / f'case-{case:02}.summary.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
        cases.append(dict(case=case, **result['summary'], sqlite_bytes_unchanged=True))
    benchmarks = []
    for n in (1000, 10000, 100000):
        db = make(f'benchmark:{n}')
        runs = [run(db) for _ in range(3)]
        assert all(r['summary']['indexed_sources'] == n and r['summary']['works'] == n and r['summary']['proposed_reading_units'] == n for r in runs)
        benchmarks.append(dict(sources=n, database_bytes=db.stat().st_size,
            medians_ms={stage: statistics.median(r['timings'][stage] for r in runs) for stage in ('adapter_ms', 'recognize_ms', 'diff_ms', 'total_ms')},
            median_open_through_summary_ms=statistics.median(r['open_through_summary_ms'] for r in runs),
            median_summary_serialization_ms=statistics.median(r['summary_serialization_ms'] for r in runs),
            max_peak_working_set_bytes=max(r['peak_working_set_bytes'] for r in runs), runs=runs))
    invalid = subprocess.run([str(EXE), '--shadow', str(make(1)), '999', '--authorized-index'], capture_output=True)
    assert invalid.returncode == 2 and json.loads(invalid.stderr)['error'] == 'ROOT_NOT_FOUND'
    phase1_log = (PRIVATE / 'phase1-tests.log').read_text(encoding='utf-8')
    shadow_log = (PRIVATE / 'shadow-tests-final.log').read_text(encoding='utf-8')
    def counts(log):
        matches = re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored', log)
        assert matches and not re.search(r'test result: FAILED', log)
        return dict(zip(('passed', 'failed', 'ignored'), map(sum, zip(*[tuple(map(int, m)) for m in matches]))))
    result = dict(recorded_at_utc=datetime.now(timezone.utc).isoformat(), phase1_tests=counts(phase1_log), phase1_cli_goldens=golden, shadow_tests=counts(shadow_log), cli_cases=cases, benchmarks=benchmarks, invalid_cli_exit=invalid.returncode,
        scope='Synthetic databases created from all 24 current production SQL migrations. No personal database or media scanned/read. Release standalone CLI; fresh process for each sample; adapter includes SQLite IO, mapping, digest and current-detail projections; diff includes review downgrades and full structural diff. Summary serialization reported separately. Peak includes entire process and returned snapshot/plan; database construction happens in another process. OS filesystem cache is uncontrolled. Full private plan JSON serialization is not in benchmark.')
    (PRIVATE / 'verification.json').write_text(json.dumps(result, ensure_ascii=False, indent=2), encoding='utf-8')
    # Only synthetic, deidentified measurements; no database paths or private source names.
    (ROOT / 'docs/smart-mixed/phase2-verification.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    print(json.dumps({k: v for k, v in result.items() if k not in ('cli_cases', 'benchmarks')}, ensure_ascii=False, indent=2))
    print(json.dumps([{k: v for k, v in b.items() if k != 'runs'} for b in benchmarks], indent=2))


if __name__ == '__main__':
    main()
