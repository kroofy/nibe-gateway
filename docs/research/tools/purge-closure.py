#!/usr/bin/env python3
"""Purge safety check used for image slimming (docs/research/image-size-2026-10-25.md section 7).

Usage: purge-closure.py STATUS EXTENDED_STATES PURGE.txt PROTECT.txt
STATUS / EXTENDED_STATES: /var/lib/dpkg/status and /var/lib/apt/extended_states of a built root, e.g.
  tar -xf BUNDLE.tar rootfs.img; debugfs -R "cat /var/lib/dpkg/status" rootfs.img > status
PURGE.txt: packages nibe-slim.sh purges; PROTECT.txt: packages that must survive. Exit 1 if any protected
package would be removed by the purge (reverse Depends) or by the following autoremove.
"""
import re, sys
def parse(path):
    out = {}
    for b in open(path).read().split('\n\n'):
        f, cur = {}, None
        for l in b.split('\n'):
            if l.startswith(' ') and cur: f[cur] += ' ' + l.strip(); continue
            if ':' in l:
                k, v = l.split(':', 1); f[k] = v.strip(); cur = k
        if 'Package' in f: out.setdefault(f['Package'], f)
    return out
st = {k: v for k, v in parse(sys.argv[1]).items() if v.get('Status', '').endswith(' installed')}
auto = {k for k, v in parse(sys.argv[2]).items() if v.get('Auto-Installed') == '1'}
purge = set(open(sys.argv[3]).read().split()); protect = set(open(sys.argv[4]).read().split())
missing = sorted((purge | protect) - set(st))
prov = {}
for n, f in st.items():
    prov.setdefault(n, set()).add(n)
    for p in f.get('Provides', '').split(','):
        p = re.split(r'[ (:]', p.strip())[0]
        if p: prov.setdefault(p, set()).add(n)
def groups(f, keys):
    for k in keys:
        for g in f.get(k, '').split(','):
            alts = [re.split(r'[ (:]', a.strip())[0] for a in g.split('|')]
            alts = [a for a in alts if a]
            if alts: yield k, alts
hard = ('Depends', 'Pre-Depends')
R = set(purge) & set(st)
changed = True
while changed:
    changed = False
    for n, f in st.items():
        if n in R: continue
        for _, alts in groups(f, hard):
            sat = set().union(*(prov.get(a, set()) for a in alts))
            if sat and sat <= R:
                R.add(n); changed = True; break
roots = {n for n in st if n not in R and (n not in auto or st[n].get('Essential') == 'yes' or st[n].get('Protected') == 'yes')}
keep, todo = set(), list(roots)
while todo:
    n = todo.pop()
    if n in keep: continue
    keep.add(n)
    for _, alts in groups(st[n], hard + ('Recommends', 'Suggests')):
        for a in alts:
            c = sorted(prov.get(a, set()) - R)
            if c: todo.append(c[0]); break
autoremoved = set(st) - R - keep
gone = R | autoremoved
print('not installed (ignored by nibe-slim):', ' '.join(m for m in missing if m in purge) or '-')
print('protected not installed:', ' '.join(m for m in missing if m in protect) or '-')
print('removed by purge (incl. reverse Depends):', len(R), ' '.join(sorted(R - purge)) or '(no extra)')
print('then autoremoved:', len(autoremoved), ' '.join(sorted(autoremoved)))
bad = sorted(gone & protect)
print('PROTECTED REMOVED:', ' '.join(bad) or 'none')
sys.exit(1 if bad else 0)
