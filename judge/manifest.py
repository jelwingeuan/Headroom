#!/usr/bin/env python3
"""SHA-256 manifest of everything the judge says must survive.

Hashes every file under truth.json's `manifest_roots`, skipping the judge's own rebuildable set
(`rebuildable[].path`) except the `keep_files` inside it (the hand-edited node_modules file).

  python3 -I judge/manifest.py snapshot [--truth judge/out/truth.json] --out FILE
  python3 -I judge/manifest.py diff BEFORE AFTER     (exit 1 if a file was lost or changed)
"""
import argparse
import hashlib
import json
import os
import stat
import sys


def digest(path):
    """Content hash of a file, target of a symlink, or why it could not be read."""
    try:
        st = os.lstat(path)
        if stat.S_ISLNK(st.st_mode):
            return 'link:' + os.readlink(path)
        if not stat.S_ISREG(st.st_mode):
            return 'special'
        h = hashlib.sha256()
        with open(path, 'rb') as f:
            for chunk in iter(lambda: f.read(1 << 20), b''):
                h.update(chunk)
        return h.hexdigest()
    except OSError as e:
        return f'error:{e.errno}'


def snapshot(roots, exclude, keep):
    """{absolute path: digest} for files under roots, minus excluded paths, plus keep files."""
    skip = {os.path.normpath(p) for p in exclude}
    out = {}

    def walk(d):
        try:
            entries = list(os.scandir(d))
        except OSError as e:
            out[d + '/'] = f'error:{e.errno}'
            return
        for e in entries:
            if e.path in skip:
                continue
            if e.is_dir(follow_symlinks=False):
                walk(e.path)
            else:
                out[e.path] = digest(e.path)

    for r in map(os.path.normpath, roots):
        if r in skip or not os.path.lexists(r):
            continue
        if os.path.isdir(r) and not os.path.islink(r):
            walk(r)
        else:
            out[r] = digest(r)
    for k in map(os.path.normpath, keep):
        if os.path.lexists(k):
            out[k] = digest(k)
    return dict(sorted(out.items()))


def scope(truth):
    return truth['manifest_roots'], [e['path'] for e in truth['rebuildable']], truth['keep_files']


def diff(before, after):
    return {
        'lost': sorted(k for k in before if k not in after),
        'changed': sorted(k for k in before if k in after and before[k] != after[k]),
        'added': sorted(k for k in after if k not in before),
    }


def save(m, path):
    with open(path, 'w') as f:
        json.dump(m, f, indent=0, sort_keys=True)


def load(path):
    with open(path) as f:
        return json.load(f)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = p.add_subparsers(dest='cmd', required=True)
    s = sub.add_parser('snapshot')
    s.add_argument('--truth', default=os.path.join(os.path.dirname(os.path.abspath(__file__)), 'out', 'truth.json'))
    s.add_argument('--out', required=True)
    d = sub.add_parser('diff')
    d.add_argument('before')
    d.add_argument('after')
    a = p.parse_args(argv)
    if a.cmd == 'snapshot':
        m = snapshot(*scope(load(a.truth)))
        save(m, a.out)
        print(f'{len(m)} files hashed -> {a.out}')
        return 0
    r = diff(load(a.before), load(a.after))
    for kind in ('lost', 'changed'):
        print(f'{kind}: {len(r[kind])}')
        for path in r[kind][:200]:
            print(f'  {path}')
        if len(r[kind]) > 200:
            print(f'  ... and {len(r[kind]) - 200} more')
    print(f'added: {len(r["added"])}')
    return 1 if r['lost'] or r['changed'] else 0


if __name__ == '__main__':
    sys.exit(main())
