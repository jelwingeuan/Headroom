"""Unit tests for the judge's pure parts, plus one end-to-end run against a fake Headroom.

  python3 -I -m unittest discover -s judge -v
"""
import contextlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import manifest  # noqa: E402
import run  # noqa: E402
import world  # noqa: E402

GB, DAY = world.GB, world.DAY
HAS_SANDBOX = os.path.exists(run.SANDBOX)


def put(path, text='x'):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(text)
    return path


class Tmp(unittest.TestCase):
    def setUp(self):
        # realpath: on macOS /var is a symlink to /private/var and the judge compares exact paths.
        self.tmp = os.path.realpath(tempfile.mkdtemp(prefix='judge-test-'))
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def p(self, *parts):
        return os.path.join(self.tmp, *parts)


class ManifestTest(Tmp):
    def tree(self):
        root = self.p('root')
        put(self.p('root/src/a.txt'), 'a')
        keep = put(self.p('root/node_modules/dep/index.js'), 'patched by hand')
        put(self.p('root/node_modules/other.js'), 'generated')
        os.symlink('src/a.txt', self.p('root/link'))
        return root, keep

    def test_snapshot_skips_rebuildable_but_keeps_keep_files(self):
        root, keep = self.tree()
        m = manifest.snapshot([root], [self.p('root/node_modules')], [keep])
        self.assertEqual(sorted(m), sorted([self.p('root/src/a.txt'), keep, self.p('root/link')]))
        self.assertEqual(m[self.p('root/link')], 'link:src/a.txt')
        self.assertEqual(len(m[keep]), 64)
        self.assertEqual(list(m), sorted(m))

    def test_diff_reports_lost_changed_added(self):
        root, keep = self.tree()
        args = ([root], [self.p('root/node_modules')], [keep])
        before = manifest.snapshot(*args)
        shutil.rmtree(self.p('root/node_modules'))  # the keep file goes with it
        put(self.p('root/src/a.txt'), 'changed')
        put(self.p('root/src/new.txt'))
        d = manifest.diff(before, manifest.snapshot(*args))
        self.assertEqual(d['lost'], [keep])
        self.assertEqual(d['changed'], [self.p('root/src/a.txt')])
        self.assertEqual(d['added'], [self.p('root/src/new.txt')])

    def test_file_roots_missing_roots_and_scope(self):
        f = put(self.p('storage.json'), '{}')
        m = manifest.snapshot([f, self.p('nope')], [], [])
        self.assertEqual(list(m), [f])
        truth = {'manifest_roots': [f], 'rebuildable': [{'path': 'x'}], 'keep_files': ['k']}
        self.assertEqual(manifest.scope(truth), ([f], ['x'], ['k']))

    def test_cli_diff_exit_code(self):
        a, b, c = self.p('a.json'), self.p('b.json'), self.p('c.json')
        manifest.save({'/f': '1', '/g': '2'}, a)
        manifest.save({'/f': '1', '/g': '2', '/h': '3'}, b)
        manifest.save({'/f': '9'}, c)
        with contextlib.redirect_stdout(io.StringIO()) as out:
            self.assertEqual(manifest.main(['diff', a, b]), 0)
            self.assertEqual(manifest.main(['diff', a, c]), 1)
        self.assertIn('lost: 1', out.getvalue())
        self.assertIn('changed: 1', out.getvalue())


class VerdictTest(unittest.TestCase):
    def test_reclaim_needs_twice_shed(self):
        self.assertEqual(run.reclaim_verdict(4 * GB, 2 * GB), (2.0, run.PASS))
        self.assertEqual(run.reclaim_verdict(3 * GB, 2 * GB), (1.5, run.FAIL))
        for shed in (None, 0, -5, 'x'):
            self.assertEqual(run.reclaim_verdict(GB, shed), (None, run.NOT))

    def test_tick(self):
        short0 = [{'event': 'shortfall', 'needed_bytes': 5, 'eligible_bytes': 0}]
        self.assertTrue(run.tick_verdict(10, 10, [], 10 * GB)[0])
        self.assertFalse(run.tick_verdict(9, 10, [], 0)[0])
        self.assertTrue(run.tick_verdict(9, 10, short0, run.TOLERANCE)[0])
        self.assertFalse(run.tick_verdict(9, 10, short0, run.TOLERANCE + 1)[0])  # a false "nothing left"
        self.assertFalse(run.tick_verdict(9, 10, [{'event': 'shortfall', 'eligible_bytes': 7}], 0)[0])

    def test_cpu(self):
        self.assertEqual(run.cpu_verdict([0.1, 0.3], True), (0.2, run.PASS))
        self.assertEqual(run.cpu_verdict([2.0, 0.0], True), (1.0, run.FAIL))
        self.assertEqual(run.cpu_verdict([], True), (None, run.FAIL))
        self.assertEqual(run.cpu_verdict([0.0], False), (None, run.FAIL))

    def test_jsonl(self):
        events, bad = run.jsonl('{"event": "reclaim", "bytes": 3}\n\nnot json\n[1]\n')
        self.assertEqual((events, bad), ([{'event': 'reclaim', 'bytes': 3}], 2))

    def test_markdown_formats_bytes(self):
        rep = {'scale': None, 'world_errors': [], 'binary': None,
               'gates': [run.row('2 Reclaim', run.FAIL, {'shed_moved_bytes': 3 * GB, 'x': [1, 2, 3, 4]}, 'a|b')]}
        md = run.markdown(rep)
        self.assertIn('shed_moved_bytes=3.00 GB', md)
        self.assertIn('x=[1, 2, 3, +1]', md)


class SandboxTest(Tmp):
    def test_profile_and_command(self):
        self.assertEqual(run.PROFILE, '(version 1)(allow default)(deny network*)(allow network* (local unix))')
        fake = put(self.p('sandbox-exec'))
        self.assertEqual(run.sandboxed(['hr', 'scan'], fake), [fake, '-p', run.PROFILE, 'hr', 'scan'])
        self.assertEqual(run.sandboxed(['hr', 'scan'], self.p('missing')), ['hr', 'scan'])

    @unittest.skipUnless(HAS_SANDBOX, 'needs macOS sandbox-exec')
    def test_profile_is_accepted_and_blocks_tcp(self):
        self.assertEqual(subprocess.run(run.sandboxed(['/usr/bin/true'])).returncode, 0)
        code = 'import socket; socket.create_connection(("127.0.0.1", 9), 2)'
        r = subprocess.run(run.sandboxed([sys.executable, '-I', '-c', code]), capture_output=True, text=True)
        self.assertIn('Operation not permitted', r.stderr)


class DockerTest(Tmp):
    def test_subcommand(self):
        self.assertEqual(run.docker_cmd(['--context', 'default', 'image', 'rm', '-f', 'abc']), ('image', 'rm'))
        self.assertEqual(run.docker_cmd(['rmi', 'abc']), ('rmi',))
        self.assertEqual(run.docker_cmd(['images', '--format', 'json']), ('images',))
        self.assertEqual(run.docker_cmd(['-H', 'unix:///x', 'system', 'df']), ('system', 'df'))
        self.assertEqual(run.docker_cmd(['--version']), ())

    def test_violations(self):
        ok = [['rmi', 'a'], ['image', 'rm', 'a'], ['image', 'prune', '-a', '-f'], ['builder', 'prune', '--all'],
              ['images', '--format', 'json'], ['system', 'df'], ['builder', 'du'], ['volume', 'ls'], ['--version']]
        bad = [['volume', 'rm', 'judge_pgdata'], ['volume', 'prune', '-f'], ['system', 'prune', '--volumes'],
               ['system', 'prune'], ['container', 'rm', 'c'], ['rm', 'c']]
        self.assertEqual(run.docker_violations(ok), [])
        self.assertEqual(run.docker_violations(bad), [' '.join(c) for c in bad])
        self.assertEqual(len(run.docker_violations(run.docker_calls('garbage\n["rmi", "x"]\n'))), 1)

    def test_shim_records_and_answers(self):
        log = self.p('docker.log')
        shim = world.write_docker_shim(self.p('bin'), log)
        out = subprocess.run([shim, 'images', '--format', 'json'], capture_output=True, text=True, check=True).stdout
        self.assertEqual(json.loads(out.splitlines()[0])['Repository'], 'judge/api')
        self.assertIn('Build Cache', subprocess.run([shim, 'system', 'df'], capture_output=True, text=True).stdout)
        r = subprocess.run([shim, '--context', 'default', 'volume', 'rm', 'judge_pgdata'], capture_output=True, text=True)
        self.assertEqual((r.returncode, r.stdout), (0, ''))
        with open(log) as f:
            calls = run.docker_calls(f.read())
        self.assertEqual(calls[0], ['images', '--format', 'json'])
        self.assertEqual(run.docker_violations(calls), ['--context default volume rm judge_pgdata'])


class WorldTest(Tmp):
    def test_scale(self):
        self.assertEqual(world.counts(1.0), world.FULL)
        self.assertEqual(set(world.counts(0.0).values()), {1})
        self.assertEqual(world.counts(0.5)['node'], 5)
        self.assertEqual(world.fit_ratio(1000 * GB), 1.0)
        self.assertEqual(world.fit_ratio(2 * GB), 0.0)
        self.assertLess(world.fit_ratio(8 * GB), 1.0)

    def test_plan_spreads_activity_over_kinds(self):
        order = world.plan(world.FULL)
        self.assertEqual(len(order), sum(world.FULL.values()))
        self.assertEqual(len({k for k, _ in order[:5]}), 5)
        by_kind = {}
        for i, (kind, _) in enumerate(order):
            by_kind.setdefault(kind, set()).add(world.activity(i)[0])
        self.assertTrue(all('recent' in s and 'cold' in s for s in by_kind.values()), by_kind)
        self.assertIn('recent', [world.activity(i)[0] for i, _ in enumerate(world.plan(world.counts(0.0)))])
        for i in range(40):
            label, days = world.activity(i)
            self.assertTrue({'recent': days == 2, 'warm': 20 <= days < 30, 'cold': 60 <= days <= 200}[label])

    def test_backdate_and_alloc(self):
        proj = self.p('proj')
        f = put(os.path.join(proj, 'node_modules/dep/index.js'), 'x' * 10000)
        os.link(f, os.path.join(proj, 'hardlink.js'))
        os.symlink('missing-target', os.path.join(proj, 'dangling'))
        t = time.time() - 100 * DAY
        world.backdate({'backdate': [[proj, t], [f, t + DAY]]})
        self.assertEqual(int(os.lstat(os.path.join(proj, 'node_modules')).st_mtime), int(t))
        self.assertEqual(int(os.lstat(os.path.join(proj, 'dangling')).st_mtime), int(t))
        self.assertEqual(int(os.stat(f).st_mtime), int(t + DAY))
        one = os.stat(f).st_blocks * 512
        self.assertGreaterEqual(one, 10000)
        self.assertLess(world.alloc_bytes(proj), 2 * one)  # the hard link counts once

    def test_inside(self):
        self.assertTrue(world.inside('/a/b/c', '/a/b'))
        self.assertTrue(world.inside('/a/b', '/a/b/'))
        self.assertFalse(world.inside('/a/bc', '/a/b'))

    def truth(self):
        return {'root': self.p('world'), 'created': 1_000_000_000, 'projects': [], 'rebuildable': [],
                'keep_files': [], 'manifest_roots': [], 'backdate': [], 'recent_paths': [], 'errors': []}

    def test_add_project_bookkeeping(self):
        truth = self.truth()
        dd = self.p('DerivedData/App-abc')

        def fake(path, when):
            return {'steps': [{'cmd': 'true', 'secs': 1.0}], 'verify': 'true', 'keep': [os.path.join(path, 'k')],
                    'rebuild': [{'path': os.path.join(path, 'node_modules'), 'cmd': 'npm ci', 'cwd': path},
                                {'path': dd, 'cmd': 'xcodebuild', 'cwd': path},
                                {'path': self.p('DerivedData/ModuleCache.noindex'), 'cmd': 'x', 'cwd': path, 'shared': True}]}
        with contextlib.redirect_stdout(io.StringIO()):
            cold = world.add_project(truth, 'cold', 'node', 'cold', 100, fake)
            recent = world.add_project(truth, 'recent', 'node', 'recent', 2, fake)
            self.assertIsNone(world.add_project(truth, 'broken', 'node', 'cold', 100, lambda p, w: 1 / 0))
        self.assertEqual(cold['verify_cmd'], 'true')
        self.assertIsNone(recent['verify_cmd'])  # nothing of a recent project may be deleted, so nothing to verify
        self.assertEqual([e['project'] for e in truth['rebuildable']], ['cold', 'cold', None, None])
        when = 1_000_000_000 - 100 * DAY
        self.assertEqual([e['mtime'] for e in truth['rebuildable']], [when, when, None, None])
        self.assertEqual(len(recent['protected']), 2)
        self.assertEqual(truth['manifest_roots'], [dd])  # a recent project's DerivedData is hashed
        self.assertEqual(truth['recent_paths'], [dd, recent['path']])
        order = [b for b in truth['backdate'] if world.inside(b[0], cold['path'])]
        self.assertEqual(order, [[cold['path'], when], [os.path.join(cold['path'], 'k'), when + DAY]])  # keep file last
        self.assertFalse(os.path.exists(self.p('world/broken')))
        self.assertEqual(len(truth['errors']), 1)

    def git_out(self, cwd, *args):
        return subprocess.run(['git', *args], cwd=cwd, capture_output=True, text=True, check=True).stdout.strip()

    def test_git_traps(self):
        # The fixtures' commit messages must not meet this machine's global hooks.
        empty = put(self.p('gitconfig'), '')
        for k, v in [('GIT_CONFIG_GLOBAL', empty), ('GIT_CONFIG_NOSYSTEM', '1')]:
            self.addCleanup(lambda k=k, old=os.environ.get(k): os.environ.pop(k) if old is None else os.environ.__setitem__(k, old))
            os.environ[k] = v
        self.addCleanup(setattr, world, 'REMOTES', world.REMOTES)
        world.REMOTES = self.p('remotes')
        when = 1_600_000_000
        repo = self.p('agent-repo')
        os.makedirs(repo)
        rec = world.agent_repo(repo, when)
        wt = os.path.join(repo, '.claude/worktrees')
        self.assertEqual(self.git_out(os.path.join(wt, 'merged'), 'status', '--porcelain'), '')
        merged = self.git_out(repo, 'branch', '--merged', 'origin/main')
        self.assertIn('agent/merged', merged)
        self.assertIn('agent/env', merged)
        self.assertNotEqual(self.git_out(repo, 'log', '--oneline', 'origin/main..agent/unpushed'), '')
        self.assertEqual(self.git_out(os.path.join(wt, 'env'), 'check-ignore', '.env'), '.env')
        self.assertEqual(self.git_out(repo, 'log', '-1', '--format=%ct', 'main'), str(when + 3600))
        self.assertTrue(all(os.path.isdir(r['path']) for r in rec['rebuild']))
        self.assertTrue(all(os.path.isdir(r['path']) for r in rec['protected']))

        site = self.p('web-untracked')
        os.makedirs(site)
        world.untracked_build(site, when)
        self.assertEqual(self.git_out(site, 'status', '--porcelain'), '?? build/\n?? dist/')
        tracked = self.p('tracked-build')
        os.makedirs(tracked)
        world.tracked_build(tracked, when)
        self.assertEqual(self.git_out(tracked, 'ls-files', 'build'), 'build/output.js')

        # After backdating and refresh_git, a plain `git status` has nothing to write.
        t = {'projects': [{'path': d} for d in (repo, site, tracked)], 'backdate': [[d, when] for d in (repo, site, tracked)]}
        world.backdate(t)
        world.refresh_git(t)
        self.assertEqual(int(os.stat(os.path.join(repo, 'src/app.js')).st_mtime), when)
        self.assertNotEqual(int(os.stat(os.path.join(repo, '.git/HEAD')).st_mtime), when)
        indexes = [os.path.join(repo, '.git/index'), os.path.join(site, '.git/index'), os.path.join(tracked, '.git/index')]
        indexes += [os.path.join(repo, '.git/worktrees', n, 'index') for n in ('merged', 'unpushed', 'env')]
        before = [manifest.digest(i) for i in indexes]
        for d in (repo, site, tracked, *(os.path.join(wt, n) for n in ('merged', 'unpushed', 'env'))):
            self.git_out(d, 'status', '--porcelain')
        self.assertEqual([manifest.digest(i) for i in indexes], before)

    def test_refuses_to_run_outside_ci(self):
        old = os.environ.pop('GITHUB_ACTIONS', None)
        self.addCleanup(lambda: old is not None and os.environ.__setitem__('GITHUB_ACTIONS', old))
        with self.assertRaises(SystemExit):
            world.main(['--root', self.p('world')])
        self.assertFalse(os.path.exists(self.p('world')))


class TreeTest(Tmp):
    def test_make_tree_and_signature(self):
        top = self.p('tree')
        self.assertEqual(run.make_tree(top, 2, 3, 4), 24)
        files = sum(len(fs) for _, _, fs in os.walk(top))
        self.assertEqual(files, 24 + 2)  # plus one package.json per project
        sig = run.signature([top])
        self.assertEqual(run.signature([top]), sig)
        os.remove(os.path.join(top, 'p000/node_modules/m000/f000.js'))
        self.assertNotEqual(run.signature([top]), sig)


FAKE = r'''#!/bin/sh
# A fake Headroom: deletes app/node_modules on clean, brings it back on rebuild/undo.
case "$1" in
  scan) echo '[]' ;;
  clean)
    echo "$2" > "$HEADROOM_HOME/root"
    rm -rf "$2/app/node_modules" @EXTRA@
    docker system df --format json > /dev/null
    echo '{"freed_bytes": 0, "actions": []}' ;;
  rebuild|undo)
    r=$(cat "$HEADROOM_HOME/root")
    mkdir -p "$r/app/node_modules/dep" && echo 'module.exports = 1' > "$r/app/node_modules/dep/index.js" ;;
  autopilot) echo '{"event": "shortfall", "needed_bytes": 1, "eligible_bytes": 0}' ;;
  agent) exec sleep 600 ;;
esac
'''


@unittest.skipUnless(HAS_SANDBOX, 'needs macOS sandbox-exec')
class EndToEndTest(Tmp):
    def setUp(self):
        super().setUp()
        for name, value in [('SCRATCH', self.tmp), ('SETTLE_S', 0), ('SPEED_TREE', (2, 3, 4)),
                            ('CPU_SECONDS', 2), ('CPU_SETTLE_S', 0)]:
            self.addCleanup(setattr, run, name, getattr(run, name))
            setattr(run, name, value)
        root, app, recent = self.p('world'), self.p('world/app'), self.p('world/recent')
        put(os.path.join(app, 'src/main.js'), 'main')
        put(os.path.join(app, 'node_modules/dep/index.js'), 'module.exports = 1\n')
        put(os.path.join(recent, 'notes.txt'), 'opened yesterday')
        now = int(time.time())
        self.truth = {
            'root': root, 'created': now, 'recent_days': 14, 'errors': [], 'history': [],
            'scale': {'ratio': 1.0, 'projects': 2, 'full_projects': 2, 'free_bytes_at_start': world.free_bytes()},
            'projects': [{'name': 'app', 'path': app, 'verify_cmd': 'test -f node_modules/dep/index.js'},
                         {'name': 'recent', 'path': recent, 'verify_cmd': None}],
            'rebuildable': [{'path': os.path.join(app, 'node_modules'), 'cmd': 'npm ci', 'cwd': app,
                             'project': 'app', 'mtime': now - 90 * DAY, 'eligible': True, 'bytes': 0}],
            'keep_files': [], 'manifest_roots': [root], 'backdate': [[app, now - 90 * DAY]],
            'recent_paths': [recent], 'models': {'ollama': 'not measured: test', 'hf': 'not measured: test'},
            'docker': {'bin_dir': self.p('bin'), 'log': self.p('docker.log')}, 'dev_server': None,
            'vscode_storage': None}
        world.write_docker_shim(self.p('bin'), self.p('docker.log'))
        self.truth_path = put(self.p('truth.json'), json.dumps(self.truth))
        self.shed_path = put(self.p('shed.json'), json.dumps(
            {'moved_delta_bytes': 0, 'emptied_delta_bytes': 10 * GB, 'trash': 'emptied', 'scale': {'ratio': 1.0}}))

    def fake(self, extra=''):
        path = put(self.p('Headroom'), FAKE.replace('@EXTRA@', extra))
        os.chmod(path, 0o755)
        return path

    def test_full_run_against_a_fake_headroom(self):
        out = self.p('out')
        with contextlib.redirect_stdout(io.StringIO()):
            rc = run.main(['--bin', self.fake(), '--truth', self.truth_path, '--shed-report', self.shed_path,
                           '--out', out, '--force-local'])
        with open(os.path.join(out, 'report.json')) as f:
            rep = json.load(f)
        got = {r['gate']: r for r in rep['gates']}
        self.assertEqual(list(got), run.GATES)
        self.assertFalse([r for r in rep['gates'] if 'judge error' in r['reason']], rep['gates'])
        self.assertEqual(rc, 1)  # reclaim is far below 2x Shed's 10 GB
        self.assertEqual(got['2 Reclaim']['status'], run.FAIL)
        for gate in ('1 Safety', '3 Autopilot', '4 Speed', 'Docker (shim)'):
            self.assertEqual(got[gate]['status'], run.PASS, got[gate])
        self.assertEqual(got['1 Safety']['numbers']['rebuildable_deleted'], 1)
        self.assertEqual(got['Docker (shim)']['numbers']['calls'], 1)
        self.assertEqual(got['Models']['status'], run.NOT)
        self.assertIn(got['5 No network']['status'], (run.PASS, run.NOT))
        self.assertEqual(got['5 No network']['numbers']['unsandboxed'], ['rebuild', 'undo'])
        self.assertFalse(os.path.exists(self.p('judge-growth')) or os.path.exists(self.p('judge-speed')))

    def test_safety_fails_when_clean_deletes_work(self):
        hr = run.Headroom(self.fake('"$2/app/src"'), self.p('home'), self.p('bin'))
        safety, _ = run.safety_reclaim(hr, self.truth, {}, self.tmp)
        self.assertEqual(safety['status'], run.FAIL)
        self.assertEqual(safety['numbers']['after_clean']['lost'], 1)
        self.assertIn(self.p('world/app/src/main.js'), run.SEEN['bad'])


class MissingBinaryTest(Tmp):
    def test_every_gate_fails_and_shed_numbers_are_kept(self):
        shed = put(self.p('shed.json'), json.dumps({'moved_delta_bytes': 1, 'emptied_delta_bytes': 2 * GB,
                                                     'trash': 'emptied'}))
        summary = self.p('summary.md')
        os.environ['GITHUB_STEP_SUMMARY'] = summary
        self.addCleanup(os.environ.pop, 'GITHUB_STEP_SUMMARY', None)
        with contextlib.redirect_stdout(io.StringIO()):
            rc = run.main(['--bin', self.p('nope'), '--truth', self.p('none.json'), '--shed-report', shed,
                           '--out', self.p('out')])
        self.assertEqual(rc, 1)
        with open(self.p('out/report.json')) as f:
            gates = json.load(f)['gates']
        self.assertEqual([(g['gate'], g['status'], g['reason']) for g in gates],
                         [(n, run.FAIL, 'binary missing') for n in run.GATES])
        self.assertEqual(gates[1]['numbers']['shed_after_empty_bytes'], 2 * GB)
        with open(summary) as f:
            text = f.read()
        self.assertIn('| 1 Safety | FAIL |', text)
        self.assertIn('shed_after_empty_bytes=2.00 GB', text)


if __name__ == '__main__':
    unittest.main()
