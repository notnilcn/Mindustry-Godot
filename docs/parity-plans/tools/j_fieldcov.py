#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
# Round-J field-coverage audit: for every public class-body field of the given
# Java packages (relative to Mindustry/core/src/mindustry), report the field names
# that never appear (case/underscore-insensitive) in the port sources.
# Usage: python3 -I docs/parity-plans/tools/j_fieldcov.py world type entities > out.tsv
# Paths J/R below are absolute for the 2026-10-16 host; adjust when re-running.
import os, re, sys, json
J = '/home/clintonnguyen/g/code_examples/Mindustry/core/src/mindustry'
R = '/home/clintonnguyen/g/code_examples/Mindustry-Godot/client'
# corpus of port identifiers
ids = set()
for root in (R + '/rust/mind-core/src', R + '/rust/mind-gdext/src', R + '/ui', R + '/scenes'):
    for dp, dn, fn in os.walk(root):
        for f in fn:
            if f.endswith(('.rs', '.gd')):
                try: t = open(os.path.join(dp, f), encoding='utf-8', errors='ignore').read()
                except Exception: continue
                ids.update(re.findall(r'[A-Za-z_][A-Za-z0-9_]*', t))
low = {i.lower().replace('_', '') for i in ids}
field_re = re.compile(r'^\s{4}public\s+(?:static\s+)?(?:final\s+)?(?:@\w+\s+)*[\w<>\[\], .?]+?\s+(\w+)\s*(?:=|;|,)', re.M)
out = {}
for sub in sys.argv[1:]:
    for dp, dn, fn in os.walk(os.path.join(J, sub)):
        for f in fn:
            if not f.endswith('.java'): continue
            p = os.path.join(dp, f)
            t = open(p, encoding='utf-8').read()
            # only top-level class body fields (4-space indent)
            names = []
            for m in field_re.finditer(t):
                line = t[m.start():t.find('\n', m.start())]
                if '(' in line.split('=')[0]: continue
                # also catch comma-separated declarations
                decl = line.split('=')[0]
                names.append(m.group(1))
                for extra in re.findall(r',\s*(\w+)\s*(?:=[^,;]+)?', line.split(';')[0]):
                    names.append(extra)
            names = [n for n in dict.fromkeys(names) if not n.isupper() and len(n) > 2]
            miss = [n for n in names if n.lower() not in low]
            if names:
                out[os.path.relpath(p, J)] = (len(names), miss)
for k, (n, miss) in sorted(out.items(), key=lambda kv: -len(kv[1][1])):
    if miss: print(f"{k}\t{n}\t{len(miss)}\t{' '.join(miss)}")
