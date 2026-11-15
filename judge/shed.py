#!/usr/bin/env python3
"""Shed baseline: `shed clean <root> --apply` moves things to the Trash, which frees nothing on
the same volume until the Trash is emptied. Free space is read before, after the clean and after
emptying the Trash; both deltas go to judge/out/shed.json for run.py's reclaim gate.

  python3 -I judge/shed.py --bin PATH [--truth judge/out/truth.json] --empty-trash
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # python3 -I leaves this folder off sys.path
import world  # noqa: E402


def settle_free():
    os.sync()
    time.sleep(5)
    return world.free_bytes()


def empty_trash():
    trash = os.path.join(world.HOME, '.Trash')
    try:
        for n in os.listdir(trash):
            p = os.path.join(trash, n)
            if os.path.isdir(p) and not os.path.islink(p):
                shutil.rmtree(p)
            else:
                os.unlink(p)
        return 'emptied'
    except OSError as e:  # ~/.Trash can be off limits without Full Disk Access; Finder can still empty it
        r = subprocess.run(['osascript', '-e', 'tell application "Finder" to empty trash'],
                           capture_output=True, text=True, timeout=900)
        return 'emptied by Finder' if r.returncode == 0 else f'not emptied: {e}; Finder: {r.stderr.strip()}'


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--bin', required=True, help='the shed binary built from 19a2339')
    p.add_argument('--truth', default=os.path.join(world.OUT, 'truth.json'))
    p.add_argument('--out', default=os.path.join(world.OUT, 'shed.json'))
    p.add_argument('--empty-trash', action='store_true', help='empty ~/.Trash afterwards (all of it)')
    p.add_argument('--force-local', action='store_true', help='run outside GitHub Actions anyway')
    a = p.parse_args(argv)
    if os.environ.get('GITHUB_ACTIONS') != 'true' and not a.force_local:
        sys.exit('shed.py cleans with Shed and empties the Trash: CI only (--force-local to override)')
    with open(a.truth) as f:
        truth = json.load(f)
    rep = {'commit': world.SHED_SHA, 'root': truth['root'], 'scale': truth['scale']}
    try:
        f0 = settle_free()
        r = subprocess.run([a.bin, 'clean', truth['root'], '--apply'], capture_output=True, text=True, timeout=3600)
        f1 = settle_free()
        rep.update(rc=r.returncode, output_tail=(r.stdout + r.stderr)[-3000:], free_before_bytes=f0,
                   moved_delta_bytes=f1 - f0)
        rep['trash'] = empty_trash() if a.empty_trash else 'not emptied (no --empty-trash)'
        f2 = settle_free()
        if rep['trash'].startswith('emptied'):
            rep['emptied_delta_bytes'] = f2 - f0
    except Exception as e:
        rep['error'] = repr(e)
    os.makedirs(os.path.dirname(a.out), exist_ok=True)
    with open(a.out, 'w') as f:
        json.dump(rep, f, indent=2)
    h = {k: (f"{rep[k] / world.GB:.2f} GB" if isinstance(rep.get(k), int) else 'n/a')
         for k in ('moved_delta_bytes', 'emptied_delta_bytes')}
    md = (f"## Shed baseline ({world.SHED_SHA[:7]})\n\n"
          f"World: {truth['scale'].get('projects')} projects, scale {truth['scale']['ratio']}. "
          f"`shed clean` exit {rep.get('rc')}; Trash: {rep.get('trash')}{'; error: ' + rep['error'] if 'error' in rep else ''}\n\n"
          "| Free-space delta | Bytes |\n|---|---|\n"
          f"| after `shed clean <root> --apply` (moved to Trash) | {h['moved_delta_bytes']} |\n"
          f"| after emptying the Trash | {h['emptied_delta_bytes']} |\n")
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as f:
            f.write(md)
    print(md)
    print(rep.get('output_tail', ''))
    return 0 if 'emptied_delta_bytes' in rep else 1


if __name__ == '__main__':
    sys.exit(main())
