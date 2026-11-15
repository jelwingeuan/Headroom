#!/usr/bin/env python3
"""Runs the judge's gates against a Headroom binary; writes judge/out/report.json and the job summary.

  python3 -I judge/run.py --bin PATH [--truth judge/out/truth.json] [--out judge/out]

Each gate reports real numbers and PASS, FAIL or NOT-MEASURED. Without a binary every Headroom
gate is FAIL ("binary missing") and the world's eligible bytes are still reported. Exits 1 if
any gate FAILs.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import time
import traceback

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))  # python3 -I leaves this folder off sys.path
import manifest  # noqa: E402
import world  # noqa: E402

PROFILE = '(version 1)(allow default)(deny network*)(allow network* (local unix))'
SANDBOX = '/usr/bin/sandbox-exec'
PASS, FAIL, NOT = 'PASS', 'FAIL', 'NOT-MEASURED'
GATES = ['1 Safety', '2 Reclaim', '3 Autopilot', '4 Speed', '5 No network', 'Docker (shim)', 'Models']
MB, GB, DAY = world.MB, world.GB, world.DAY
SCRATCH = world.HOME  # speed tree and growth files: outside every manifest root
SETTLE_S = 5  # let APFS finish freeing before free space is read after a big delete
DAYS = 30
QUIET_DAYS = 8.5  # the margin lasts 8 days of growth: room for a 7-day dry-run start
RECLAIM_SHARE = 0.8  # clean must free at least 80% of what the judge counts as eligible
STOCK_SHARE = 0.6  # 30 days of growth need 60% of what the judge counts as eligible
TOLERANCE = 64 * MB  # eligible bytes ignored when a shortfall says nothing is left
SPEED_TREE = (100, 100, 100)  # projects x node_modules dirs x empty files = 1,000,000 files
SCAN_LIMIT_S = 3.0
CPU_SECONDS, CPU_SETTLE_S, CPU_LIMIT = 60, 5, 1.0

DOCKER_VALUE_OPTS = {'--context', '-c', '--host', '-H', '--config', '--log-level', '-l'}
DOCKER_GROUPS = {'image', 'container', 'volume', 'system', 'builder', 'buildx', 'context', 'network', 'plugin'}
DOCKER_READ = {('images',), ('image', 'ls'), ('image', 'list'), ('image', 'inspect'), ('inspect',),
               ('system', 'df'), ('system', 'info'), ('builder', 'du'), ('buildx', 'du'), ('version',), ('info',),
               ('ps',), ('container', 'ls'), ('container', 'list'), ('volume', 'ls'), ('volume', 'list'),
               ('volume', 'inspect'), ('context', 'ls'), ('context', 'show'), ('context', 'inspect')}
DOCKER_ALLOWED = {('image', 'rm'), ('rmi',), ('image', 'prune'), ('builder', 'prune')}

SEEN = {'bad': set(), 'compared': 0}  # files lost or changed in any manifest comparison this run


def row(gate, status, numbers=None, reason=''):
    return {'gate': gate, 'status': status, 'numbers': numbers or {}, 'reason': reason}


def human(n):
    if not isinstance(n, (int, float)):
        return 'n/a'
    return f'{n / GB:.2f} GB' if abs(n) >= GB else f'{n / MB:.1f} MB'


def num(v):
    return v if isinstance(v, (int, float)) and not isinstance(v, bool) else 0


def sandboxed(cmd, sandbox=SANDBOX):
    return [sandbox, '-p', PROFILE] + cmd if os.path.exists(sandbox) else cmd


class Headroom:
    """The binary under test; every invocation is recorded for the no-network gate."""

    def __init__(self, path, home, docker_bin):
        os.makedirs(home, exist_ok=True)
        self.path = path
        path = os.environ.get('PATH', '')
        self.env = dict(os.environ, HEADROOM_HOME=home, PATH=docker_bin + os.pathsep + path if docker_bin else path)
        self.calls = []

    def cmd(self, args, sandbox=True):
        return sandboxed([self.path] + args) if sandbox else [self.path] + args

    def record(self, args, cmd, rc, secs, err=''):
        self.calls.append({'args': args, 'sandboxed': cmd[0] == SANDBOX, 'rc': rc, 'secs': round(secs, 2),
                           'stderr': err[-400:]})

    def run(self, args, sandbox=True, timeout=1800):
        cmd = self.cmd(args, sandbox)
        t = time.monotonic()
        try:
            r = subprocess.run(cmd, stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=timeout,
                               env=self.env)
            rc, out, err = r.returncode, r.stdout, r.stderr
        except subprocess.TimeoutExpired:
            rc, out, err = -1, '', f'timed out after {timeout}s'
        except OSError as e:
            rc, out, err = -2, '', str(e)
        self.record(args, cmd, rc, time.monotonic() - t, err)
        return rc, out


def parse_json(text):
    try:
        return json.loads(text)
    except ValueError:
        return None


def jsonl(text):
    """Events (JSON objects, one per line) and how many lines were not one."""
    events, bad = [], 0
    for line in text.splitlines():
        if line.strip():
            e = parse_json(line)
            if isinstance(e, dict):
                events.append(e)
            else:
                bad += 1
    return events, bad


def reclaim_verdict(freed, eligible_bytes):
    """clean must free at least RECLAIM_SHARE of the bytes the judge counts as eligible."""
    if not isinstance(eligible_bytes, (int, float)) or eligible_bytes <= 0:
        return None, NOT
    share = freed / eligible_bytes
    return round(share, 3), PASS if share >= RECLAIM_SHARE else FAIL


def tick_verdict(free, budget, events, eligible_left):
    """One autopilot day: at or above budget, or an honest 'nothing eligible left' shortfall."""
    if free >= budget:
        return True, ''
    shortfalls = [e for e in events if e.get('event') == 'shortfall']
    if not shortfalls:
        return False, 'below budget, no shortfall event'
    if not any(e.get('eligible_bytes') == 0 for e in shortfalls):
        return False, 'below budget while its shortfall still lists eligible bytes'
    if eligible_left > TOLERANCE:
        return False, f'shortfall says nothing is eligible, the judge counts {human(eligible_left)}'
    return True, 'shortfall, nothing eligible left'


def cpu_verdict(samples, alive):
    if not alive or not samples:
        return None, FAIL
    mean = sum(samples) / len(samples)
    return round(mean, 3), PASS if mean < CPU_LIMIT else FAIL


def docker_cmd(argv):
    """The subcommand of one docker call, global options skipped: ('image', 'rm'), ('rmi',), ..."""
    words, skip = [], False
    for a in argv:
        if skip:
            skip = False
        elif a in DOCKER_VALUE_OPTS:
            skip = True
        elif not a.startswith('-'):
            words.append(a)
    if not words:
        return ()
    return tuple(words[:2]) if words[0] in DOCKER_GROUPS else (words[0],)


def docker_calls(text):
    calls = []
    for line in text.splitlines():
        if line.strip():
            c = parse_json(line)
            ok = isinstance(c, list) and all(isinstance(x, str) for x in c)
            calls.append(c if ok else ['<unreadable log line>', line])
    return calls


def docker_violations(calls):
    """Every docker call that is neither read-only nor one of the allowed deletes."""
    allowed = DOCKER_READ | DOCKER_ALLOWED
    return [' '.join(c) for c in calls if docker_cmd(c) and docker_cmd(c) not in allowed]


def snap(truth, out, name):
    m = manifest.snapshot(*manifest.scope(truth))
    manifest.save(m, os.path.join(out, f'manifest-{name}.json'))
    return m


def compare(before, after):
    d = manifest.diff(before, after)
    SEEN['bad'].update(d['lost'], d['changed'])
    SEEN['compared'] += 1
    nums = {'files': len(before), 'lost': len(d['lost']), 'changed': len(d['changed']), 'added': len(d['added']),
            'examples': (d['lost'] + d['changed'])[:10]}
    return nums, bool(d['lost'] or d['changed'])


def settle_free():
    os.sync()
    time.sleep(SETTLE_S)
    return world.free_bytes()


def alive(pid):
    try:
        os.kill(pid, 0)
        return True
    except ProcessLookupError:
        return False
    except PermissionError:
        return True


def verify(truth):
    """Runs every touched project's own build/check command; returns the ones that failed."""
    env = dict(os.environ, **world.ENV)
    failed = []
    for p in truth['projects']:
        if p['verify_cmd']:
            try:
                rc = subprocess.run(p['verify_cmd'], shell=True, cwd=p['path'], env=env, capture_output=True,
                                    timeout=1800).returncode
            except (OSError, subprocess.TimeoutExpired) as e:
                rc = type(e).__name__
            if rc:
                failed.append(f"{p['name']} ({rc})")
    return failed


def eligible(truth):
    """Rebuildable entries still on disk that nothing protects: no keep file inside, not in a
    protected (busy, untracked work, ...) or recently opened path."""
    blocked = list(truth['recent_paths']) + [x['path'] for p in truth['projects'] for x in p.get('protected', [])]
    return [e for e in truth['rebuildable']
            if e['eligible'] and os.path.lexists(e['path'])
            and not any(world.inside(e['path'], b) for b in blocked)
            and not any(world.inside(k, e['path']) for k in truth['keep_files'])]


def eligible_numbers(truth):
    paths = [e['path'] for e in eligible(truth)]
    return {'eligible_bytes': world.alloc_bytes(*paths), 'eligible_items': len(paths)}


def safety_reclaim(hr, truth, out):
    dev = truth.get('dev_server') or {}
    dev_up = bool(dev.get('pid')) and alive(dev['pid'])
    m0 = snap(truth, out, '0-before')
    present = [e['path'] for e in truth['rebuildable'] if os.path.lexists(e['path'])]
    before = eligible_numbers(truth)  # measured right before clean
    f0 = settle_free()
    rc_clean, so = hr.run(['clean', truth['root'], '--apply', '--json'], timeout=3600)
    f1 = settle_free()
    claimed = parse_json(so)
    after_clean, bad_clean = compare(m0, snap(truth, out, '1-after-clean'))
    deleted = [p for p in present if not os.path.lexists(p)]
    rc_rebuild, _ = hr.run(['rebuild', '--all'], sandbox=False, timeout=5400)
    after_rebuild, bad_rebuild = compare(m0, snap(truth, out, '2-after-rebuild'))
    not_rebuilt = [p for p in deleted if not os.path.lexists(p)]
    failed = verify(truth)
    dev_killed = dev_up and not alive(dev['pid'])
    problems = [msg for bad, msg in [
        (rc_clean, f'clean exited {rc_clean}'),
        (bad_clean, 'clean lost or changed files that must survive'),
        (rc_rebuild, f'rebuild exited {rc_rebuild}'),
        (bad_rebuild, 'files lost or changed after rebuild'),
        (not_rebuilt, f'{len(not_rebuilt)} deleted folders not rebuilt'),
        (failed, 'build failed after rebuild: ' + ', '.join(failed)),
        (dev_killed, 'the running dev server was killed')] if bad]
    safety = row('1 Safety', FAIL if problems else PASS, {
        'after_clean': after_clean, 'after_rebuild': after_rebuild, 'rebuildable_deleted': len(deleted),
        'not_rebuilt': not_rebuilt, 'verified': sum(1 for p in truth['projects'] if p['verify_cmd']),
        'verify_failed': failed, 'dev_server_up_at_start': dev_up}, '; '.join(problems))

    freed = f1 - f0
    share, status = reclaim_verdict(freed, before['eligible_bytes'])
    reason = {PASS: '', NOT: 'nothing eligible to reclaim in this world',
              FAIL: f'freed {share:.0%} of the eligible bytes, needs {RECLAIM_SHARE:.0%}' if share is not None else ''}
    reclaim = row('2 Reclaim', status, dict(
        before, freed_bytes=freed, freed_share=share,
        claimed_bytes=claimed.get('freed_bytes') if isinstance(claimed, dict) else None,
        clean_json_ok=isinstance(claimed, dict)), reason[status])
    return [safety, reclaim]


def signature(paths):
    """(inode, size, mtime) of everything under paths: any delete, write or re-create shows."""
    sig = {}
    for top in paths:
        for d, dirs, files in os.walk(top):
            for n in [''] + dirs + files:
                p = os.path.join(d, n) if n else d
                try:
                    st = os.lstat(p)
                except OSError:
                    continue
                sig[p] = (st.st_ino, st.st_size, st.st_mtime_ns)
    return sig


def write_growth(path, n):
    chunk = b'\0' * (8 * MB)
    with open(path, 'wb') as f:
        left = n
        while left > 0:
            f.write(chunk[:left])
            left -= len(chunk)
        f.flush()
        os.fsync(f.fileno())


def autopilot(hr, truth, out):
    for e in truth['rebuildable']:  # gate 1's rebuild made these look fresh; the world says they are cold
        if e.get('mtime'):
            world.backdate_tree(e['path'], e['mtime'])
    base = snap(truth, out, '3-before-autopilot')
    stock = {e['path']: world.alloc_bytes(e['path']) for e in eligible(truth)}
    total = sum(stock.values())
    if not total:
        return [row('3 Autopilot', NOT, reason='nothing eligible to reclaim in this world')]
    growth = max(MB, int(total * STOCK_SHARE / (DAYS - QUIET_DAYS)))
    margin = int(growth * QUIET_DAYS)
    growth_dir = os.path.join(SCRATCH, 'judge-growth')
    shutil.rmtree(growth_dir, ignore_errors=True)
    os.makedirs(growth_dir)
    recent = truth['recent_paths']
    sig0 = signature(recent)
    t0 = int(time.time())
    budget = world.free_bytes() - margin
    days, touched, inside_recent = [], set(), []
    try:
        for d in range(DAYS):
            now = t0 + d * DAY
            write_growth(os.path.join(growth_dir, f'day-{d:02d}.bin'), growth)
            if truth.get('vscode_storage') and os.path.exists(truth['vscode_storage']):
                os.utime(truth['vscode_storage'], (now - 3600, now - 3600))  # the editor still has them open
            rc, so = hr.run(['autopilot', '--once', '--now', str(now), '--budget-bytes', str(budget),
                             '--recent-days', str(truth['recent_days'])], timeout=900)
            events, unreadable = jsonl(so)
            os.sync()
            free = world.free_bytes()
            left = sum(b for p, b in stock.items() if os.path.lexists(p))
            ok, why = tick_verdict(free, budget, events, left)
            if rc:
                ok, why = False, f'exited {rc}'
            reclaims = [e for e in events if e.get('event') == 'reclaim']
            inside_recent += [str(e.get('path')) for e in reclaims
                              if any(world.inside(str(e.get('path')), r) for r in recent)]
            now_sig = signature(recent)
            touched |= {p for p in set(sig0) | set(now_sig) if sig0.get(p) != now_sig.get(p)}
            days.append({'day': d, 'free_minus_budget': free - budget, 'reclaimed': sum(num(e.get('bytes')) for e in reclaims),
                         'shortfall': any(e.get('event') == 'shortfall' for e in events),
                         'unreadable_lines': unreadable, 'ok': ok, 'why': why})
        growth_kept = all(os.path.exists(os.path.join(growth_dir, f'day-{d:02d}.bin')) for d in range(DAYS))
        rc_undo, _ = hr.run(['undo', '--all'], sandbox=False, timeout=5400)
        after_undo, bad_undo = compare(base, snap(truth, out, '4-after-undo'))
    finally:
        shutil.rmtree(growth_dir, ignore_errors=True)
    failed = [x for x in days if not x['ok']]
    problems = [msg for bad, msg in [
        (failed, f"{len(failed)} of {DAYS} days failed, first: day {failed[0]['day']}: {failed[0]['why']}" if failed else ''),
        (inside_recent, f'{len(inside_recent)} reclaims inside recently opened projects'),
        (touched, f'{len(touched)} paths changed inside recently opened projects'),
        (not growth_kept, "deleted the judge's growth files (user data)"),
        (rc_undo, f'undo exited {rc_undo}'),
        (bad_undo, 'files lost or changed after undo')] if bad]
    r = row('3 Autopilot', FAIL if problems else PASS, {
        'budget_bytes': budget, 'margin_bytes': margin, 'growth_per_day_bytes': growth, 'eligible_stock_bytes': total,
        'min_free_minus_budget_bytes': min(x['free_minus_budget'] for x in days),
        'reclaimed_total_bytes': sum(x['reclaimed'] for x in days), 'shortfall_days': sum(x['shortfall'] for x in days),
        'failed_days': len(failed), 'touched_recent': sorted(touched)[:5] + inside_recent[:5],
        'after_undo': after_undo}, '; '.join(problems))
    r['days'] = days
    return [r]


def make_tree(top, projects, dirs, files):
    """Empty files shaped like many projects' node_modules; returns how many files it wrote."""
    n = 0
    for p in range(projects):
        proj = os.path.join(top, f'p{p:03d}')
        os.makedirs(proj)
        world.package_json(proj, {'left-pad': '^1'})
        for d in range(dirs):
            sub = os.path.join(proj, 'node_modules', f'm{d:03d}')
            os.makedirs(sub)
            for f in range(files):
                os.close(os.open(os.path.join(sub, f'f{f:03d}.js'), os.O_CREAT | os.O_WRONLY | os.O_EXCL, 0o644))
            n += files
    return n


def idle_cpu(hr, out):
    cmd = hr.cmd(['agent', '--idle'])
    t = time.monotonic()
    with open(os.path.join(out, 'agent.log'), 'ab') as log:
        p = subprocess.Popen(cmd, env=hr.env, stdin=subprocess.DEVNULL, stdout=log, stderr=log)
    samples = []
    try:
        time.sleep(CPU_SETTLE_S)  # skip start-up work: the gate is about idling
        for _ in range(CPU_SECONDS):
            if p.poll() is not None:
                break
            r = subprocess.run(['ps', '-o', '%cpu=', '-p', str(p.pid)], capture_output=True, text=True)
            try:
                samples.append(float(r.stdout.strip().replace(',', '.')))
            except ValueError:
                pass
            time.sleep(1)
        up = p.poll() is None
    finally:
        if p.poll() is None:
            p.terminate()
            try:
                p.wait(10)
            except subprocess.TimeoutExpired:
                p.kill()
                p.wait()
    hr.record(['agent', '--idle'], cmd, 0 if up else p.returncode, time.monotonic() - t)
    mean, status = cpu_verdict(samples, up)
    return mean, status, samples, up


def speed(hr, out):
    top = os.path.join(SCRATCH, 'judge-speed')
    shutil.rmtree(top, ignore_errors=True)
    t = time.monotonic()
    try:
        files = make_tree(top, *SPEED_TREE)
        tree_secs = time.monotonic() - t
        rc1, _ = hr.run(['scan', top, '--json'], timeout=1800)
        t = time.monotonic()
        rc2, so = hr.run(['scan', top, '--json'], timeout=1800)
        warm = time.monotonic() - t
    finally:
        subprocess.run(['rm', '-rf', top])
    items = parse_json(so)
    mean, cpu_status, samples, up = idle_cpu(hr, out)
    problems = [msg for bad, msg in [
        (rc1 or rc2, f'scan exited {rc1} then {rc2}'),
        (not isinstance(items, list), 'scan output is not a JSON array'),
        (warm >= SCAN_LIMIT_S, f'warm scan took {warm:.2f} s, limit {SCAN_LIMIT_S} s'),
        (not up, 'agent --idle exited during the sample'),
        (up and cpu_status == FAIL, f'idle CPU {mean}%, limit {CPU_LIMIT}%')] if bad]
    return [row('4 Speed', FAIL if problems else PASS, {
        'files': files, 'tree_secs': round(tree_secs, 1), 'warm_scan_secs': round(warm, 3),
        'candidates': len(items) if isinstance(items, list) else None, 'idle_cpu_mean_pct': mean,
        'cpu_samples': len(samples), 'cpu_max_pct': max(samples) if samples else None}, '; '.join(problems))]


def sandbox_self_test():
    """(curl reaches the network unsandboxed, the sandbox blocks it)."""
    curl = ['/usr/bin/curl', '-sS', '-m', '10', '-o', '/dev/null', 'https://github.com']

    def ok(c):
        return subprocess.run(c, capture_output=True, timeout=60).returncode == 0
    return ok(curl), not ok(sandboxed(curl))


def no_network(hr):
    if not os.path.exists(SANDBOX):
        return [row('5 No network', NOT, reason='sandbox-exec missing: Headroom ran with network')]
    control, blocked = sandbox_self_test()
    boxed = [c for c in hr.calls if c['sandboxed']]
    failed = [f"{' '.join(c['args'][:2])} ({c['rc']})" for c in boxed if c['rc'] != 0]
    nums = {'sandboxed_runs': len(boxed), 'failed': failed[:10],
            'unsandboxed': sorted({c['args'][0] for c in hr.calls if not c['sandboxed']}),
            'self_test_control_ok': control, 'self_test_blocked': blocked}
    if not (control and blocked):
        return [row('5 No network', NOT, nums, 'sandbox self-test inconclusive')]
    if not boxed:
        return [row('5 No network', FAIL, nums, 'no sandboxed runs')]
    return [row('5 No network', FAIL if failed else PASS, nums,
                f'{len(failed)} runs failed with network denied' if failed else '')]


def docker_gate(truth):
    log = (truth.get('docker') or {}).get('log')
    if not log:
        return [row('Docker (shim)', NOT, reason='shim not installed')]
    text = ''
    if os.path.exists(log):
        with open(log) as f:
            text = f.read()
    calls = docker_calls(text)
    bad = docker_violations(calls)
    return [row('Docker (shim)', FAIL if bad else PASS, {'calls': len(calls), 'violations': bad[:10]},
                'forbidden: ' + '; '.join(bad[:3]) if bad else 'shim, not measured live')]


def models_gate(truth):
    models = truth.get('models') or {}
    o = models.get('ollama')
    hf = models.get('hf')
    nums = {'hf': 'set up' if isinstance(hf, dict) else hf}
    if not isinstance(o, dict):
        return [row('Models', NOT, nums, o or 'Ollama not set up')]
    if not SEEN['compared']:
        return [row('Models', NOT, nums, 'no manifest comparison ran')]
    hit = sorted(set(o['custom_files']) & SEEN['bad'])
    nums.update(custom_files=len(o['custom_files']), custom_lost_or_changed=hit[:5],
                registry_files_reclaimable=len(o['reclaimable_files']))
    return [row('Models', FAIL if hit else PASS, nums, 'the ollama create model was touched' if hit else '')]


def missing(why, truth):
    rows = [row(g, FAIL, reason=why) for g in GATES]
    if truth:  # still show what a cleaner could have taken
        try:
            rows[1]['numbers'] = eligible_numbers(truth)
        except (KeyError, TypeError) as e:
            rows[1]['numbers'] = {'eligible_error': repr(e)}
    return rows


def guarded(names, fn, *args):
    """A gate that crashes reports FAIL with the error instead of taking the run down."""
    print(f'== {", ".join(names)}', flush=True)
    try:
        return fn(*args)
    except Exception as e:
        traceback.print_exc()
        return [row(n, FAIL, reason=f'judge error: {e!r}') for n in names]


def fmt(k, v):
    if k.endswith('_bytes') and isinstance(v, (int, float)):
        return human(v)
    if isinstance(v, dict):
        return '{' + ', '.join(f'{a}={fmt(a, b)}' for a, b in v.items()) + '}'
    if isinstance(v, list):
        return '[' + ', '.join(map(str, v[:3])) + (f', +{len(v) - 3}' if len(v) > 3 else '') + ']'
    return str(v)


def markdown(rep):
    s = rep.get('scale') or {}
    lines = ['## Headroom judge', '']
    if s:
        lines.append(f"World: {s.get('projects')} of {s.get('full_projects')} projects (scale {s.get('ratio')}), "
                     f"free at start {human(s.get('free_bytes_at_start'))}, world errors: {len(rep['world_errors'])}.")
    lines += [f"Binary: `{rep['binary']}`" if rep['binary'] else 'Binary: missing', '',
              '| Gate | Result | Numbers | Notes |', '|---|---|---|---|']
    for r in rep['gates']:
        nums = ', '.join(f'{k}={fmt(k, v)}' for k, v in r['numbers'].items())
        cells = [r['gate'], r['status'], nums, r['reason']]
        lines.append('| ' + ' | '.join(c.replace('|', '\\|').replace('\n', ' ') for c in cells) + ' |')
    return '\n'.join(lines) + '\n'


def load(path):
    try:
        with open(path) as f:
            return json.load(f)
    except (OSError, ValueError, TypeError):
        return None


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--bin', default='', help='Headroom.app/Contents/MacOS/Headroom (empty: missing)')
    p.add_argument('--truth', default=os.path.join(world.OUT, 'truth.json'))
    p.add_argument('--out', default=world.OUT)
    p.add_argument('--force-local', action='store_true', help='run outside GitHub Actions anyway')
    a = p.parse_args(argv)
    os.makedirs(a.out, exist_ok=True)
    truth = load(a.truth)
    binary = os.path.abspath(a.bin) if a.bin and os.path.isfile(a.bin) and os.access(a.bin, os.X_OK) else None
    SEEN['bad'], SEEN['compared'] = set(), 0
    hr = None
    if not binary:
        rows = missing('binary missing', truth)
    elif not truth:
        rows = missing('no truth.json: the world was not built', truth)
    else:
        if os.environ.get('GITHUB_ACTIONS') != 'true' and not a.force_local:
            sys.exit('run.py lets Headroom really delete things on this Mac: CI only (--force-local to override)')
        hr = Headroom(binary, os.path.join(a.out, 'headroom-home'), (truth.get('docker') or {}).get('bin_dir', ''))
        rows = guarded(GATES[:2], safety_reclaim, hr, truth, a.out)
        rows += guarded(GATES[2:3], autopilot, hr, truth, a.out)
        rows += guarded(GATES[3:4], speed, hr, a.out)
        rows += guarded(GATES[5:6], docker_gate, truth)
        rows += guarded(GATES[6:7], models_gate, truth)
        rows += guarded(GATES[4:5], no_network, hr)  # last: it reads every call made above
        rows.sort(key=lambda r: GATES.index(r['gate']))
    rep = {'created': int(time.time()), 'binary': binary, 'scale': (truth or {}).get('scale'),
           'world_errors': (truth or {}).get('errors', []), 'gates': rows,
           'headroom_calls': hr.calls if hr else []}
    with open(os.path.join(a.out, 'report.json'), 'w') as f:
        json.dump(rep, f, indent=2)
    md = markdown(rep)
    if os.environ.get('GITHUB_STEP_SUMMARY'):
        with open(os.environ['GITHUB_STEP_SUMMARY'], 'a') as f:
            f.write(md)
    print(md)
    return 1 if any(r['status'] == FAIL for r in rows) else 0


if __name__ == '__main__':
    sys.exit(main())
