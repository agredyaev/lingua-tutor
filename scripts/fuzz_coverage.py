#!/usr/bin/env python3
"""Reproducible fuzz-only line coverage, one binary/profile, per-file gate.

No merging of profiles produced by different binaries; no unit-test coverage.
Run: python3 scripts/fuzz_coverage.py [--minimum 86] [--report-only]
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]

def seeds(directory):
    directory.mkdir(parents=True, exist_ok=True)
    rng = random.Random(20261028)
    cases = {bytes([mode, seed]) for mode in range(15) for seed in ([0,1,2,3,4,5,6,7,8,9,10,11,16,32,64,128] if mode==13 else range(256))}
    for mode in range(15):
        for start in (range(0) if mode==13 else range(0, 256, 16)):
            cases.add(bytes([mode, *range(start, start+16)]))
        for _ in range(0 if mode==13 else 16):
            cases.add(bytes([mode])+rng.randbytes(32))
    for text in ('niño','¿Dónde?','áéíóúüñ','tunnel_'+'a'*32,'1ms','0s','10us','1h','100m','-1s','Mon, 28 Oct 2030 12:00:00 GMT'):
        cases.add(bytes([11])+text.encode())
        cases.add(bytes([0])+text.encode())
    for case in sorted(cases):
        (directory/hashlib.sha256(case).hexdigest()).write_bytes(case)

def main():
    ap=argparse.ArgumentParser(description=__doc__)
    ap.add_argument('--minimum',type=float,default=86)
    ap.add_argument('--report-only',action='store_true')
    ap.add_argument('--seeds-only',action='store_true')
    args=ap.parse_args()
    os.chdir(ROOT)
    corpus=ROOT/'fuzz/seed-corpus/system_state';seeds(corpus)
    if args.seeds_only:return 0
    if not args.report_only:
        subprocess.run(['cargo','+nightly','fuzz','coverage','system_state',str(corpus),'--sanitizer','none'],check=True)
    host=subprocess.check_output(['rustc','+nightly','-vV'],text=True).split('host: ')[1].splitlines()[0]
    sysroot=Path(subprocess.check_output(['rustc','+nightly','--print','sysroot'],text=True).strip())
    llvm=sysroot/'lib/rustlib'/host/'bin/llvm-cov'
    binary=ROOT/'target'/host/'coverage'/host/'release/system_state'
    profile=ROOT/'fuzz/coverage/system_state/coverage.profdata'
    report=subprocess.run([str(llvm),'export',str(binary),'-instr-profile='+str(profile),'-format=lcov'],capture_output=True,text=True,check=True)
    if 'mismatched' in report.stderr or 'out of date' in report.stderr:raise RuntimeError(report.stderr)
    lines={};file=None;cutoffs={};support={}
    for line in report.stdout.splitlines():
        if line.startswith('SF:'):
            try:file=str(Path(line[3:]).relative_to(ROOT))
            except ValueError:file=None
            if file and not file.startswith(('apps/local/src/','crates/','language-packs/')):file=None
            if file and ('/tests/' in file or '/fuzzing' in file):file=None
            if file and file not in cutoffs:
                source=(ROOT/file).read_text().splitlines()
                cutoffs[file]=next((i+1 for i,s in enumerate(source) if s.startswith('// FUZZ SUPPORT BELOW:')),len(source)+1)
                omitted=set();inside=False
                for n,s in enumerate(source,1):
                    if '// FUZZ SUPPORT BEGIN' in s:inside=True
                    if inside:omitted.add(n)
                    if '// FUZZ SUPPORT END' in s:inside=False
                support[file]=omitted
        elif file and line.startswith('DA:'):
            number,count,*_=line[3:].split(',')
            if int(number)<cutoffs[file] and int(number) not in support[file]:lines.setdefault(file,{})[int(number)]=int(count)
    rows=[]
    for file,counts in sorted(lines.items()):
        total=len(counts);hit=sum(v>0 for v in counts.values());pct=100*hit/total
        rows.append(dict(file=file,covered=hit,total=total,percent=pct,uncovered=[n for n,h in counts.items() if not h]))
        print(f'{pct:6.2f}% {hit:4}/{total:<4} {file}')
    out=ROOT/'fuzz/coverage/system_state';out.mkdir(parents=True,exist_ok=True)
    (out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
    (out/'coverage.lcov').write_text(report.stdout)
    (out/'metadata.json').write_text(json.dumps(dict(metric='fuzz-only executable-line coverage',minimum=args.minimum,files=len(rows),binary=str(binary),profile=str(profile),excluded='fuzz harnesses and marked transport fixtures; CLI binary and web frontend are outside this Rust library report'),indent=2)+'\n')
    below=[r for r in rows if r['percent']<args.minimum]
    if not rows:raise RuntimeError('no first-party source coverage')
    print(f'{len(rows)-len(below)}/{len(rows)} files meet {args.minimum:g}% fuzz line coverage')
    return bool(below)

if __name__=='__main__':sys.exit(main())
