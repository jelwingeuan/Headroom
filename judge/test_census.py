"""Tests for the read-only census: one temp home with a case for every status, plus small helpers.

  python3 -I -m unittest discover -s judge -p 'test_census.py'
"""
import json
import os
import plistlib
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time
import unittest

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import census  # noqa: E402

DAY = census.DAY
NOW = time.time()
OLD = NOW - 60 * DAY
KEEP_ENV = {}


def setUpModule():
    # The census inherits os.environ for git: keep the user's global and system git config out of it.
    for k, v in (('GIT_CONFIG_GLOBAL', '/dev/null'), ('GIT_CONFIG_NOSYSTEM', '1')):
        KEEP_ENV[k] = os.environ.get(k)
        os.environ[k] = v


def tearDownModule():
    for k, v in KEEP_ENV.items():
        if v is None:
            os.environ.pop(k, None)
        else:
            os.environ[k] = v


def put(path, text='x', t=None):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'w') as f:
        f.write(text)
    if t:
        os.utime(path, (t, t))
    return path


def backdate(root, t):
    """Sets every file and folder under root (but .git) to time t."""
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if x != '.git']
        for n in dirs + files:
            os.utime(os.path.join(d, n), (t, t), follow_symlinks=False)
    os.utime(root, (t, t))


def git(cwd, *args, when=NOW):
    stamp = f'@{int(when)} +0000'
    env = dict(os.environ, GIT_AUTHOR_NAME='t', GIT_AUTHOR_EMAIL='t@example.com', GIT_COMMITTER_NAME='t',
               GIT_COMMITTER_EMAIL='t@example.com', GIT_AUTHOR_DATE=stamp, GIT_COMMITTER_DATE=stamp)
    subprocess.run(['/usr/bin/git', '-c', 'init.defaultBranch=main', '-c', 'commit.gpgsign=false', *args],
                   cwd=cwd, env=env, check=True, capture_output=True)


def node_project(root, when=OLD, ignore='node_modules/\n'):
    """package.json + src + node_modules installed at `when`, committed at `when`."""
    put(os.path.join(root, 'package.json'), '{}')
    put(os.path.join(root, 'src/index.js'), 'x')
    put(os.path.join(root, '.gitignore'), ignore)
    put(os.path.join(root, 'node_modules/dep/index.js'), 'dep')
    put(os.path.join(root, 'node_modules/.package-lock.json'), '{}')
    git(root, 'init', '-q')
    git(root, 'add', '-A')
    git(root, 'commit', '-qm', 'init', when=when)
    backdate(root, when)


def tree(root):
    """Everything an accidental write would change: (mode, size, mtime, inode) of every path."""
    out = {}
    for d, dirs, files in os.walk(root):
        for n in dirs + files:
            st = os.lstat(os.path.join(d, n))
            out[os.path.join(d, n)] = (st.st_mode, st.st_size, st.st_mtime_ns, st.st_ino)
    return out


class CensusWorld(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = os.path.realpath(tempfile.mkdtemp(prefix='census-test-'))
        home = cls.home = os.path.join(cls.tmp, 'home')
        p = cls.p

        node_project(p('cold'), ignore='node_modules/\n.venv/\nbuild/\n')
        put(p('cold/node_modules/dep/big.bin'), 'b' * 100_000, OLD)
        os.link(p('cold/node_modules/dep/big.bin'), p('cold/node_modules/dep/big-link.bin'))
        put(p('cold/requirements.txt'), 'x', OLD)
        put(p('cold/.venv/pyvenv.cfg'), 'home = /usr/bin', OLD)
        put(p('cold/build/app.js'), 'x', OLD)

        node_project(p('fresh'))
        put(p('fresh/src/index.js'), 'edited today')
        node_project(p('committed'))
        put(p('committed/src/more.js'), 'x')
        git(p('committed'), 'add', '-A')
        git(p('committed'), 'commit', '-qm', 'today')
        backdate(p('committed'), OLD)

        node_project(p('untracked'))
        put(p('untracked/build/notes.txt'), 'mine', OLD)
        node_project(p('tracked'))
        put(p('tracked/build/keep.txt'), 'kept')
        git(p('tracked'), 'add', '-A')
        git(p('tracked'), 'commit', '-qm', 'track build', when=OLD)
        backdate(p('tracked'), OLD)

        node_project(p('edited'))
        put(p('edited/node_modules/dep/index.js'), 'patched', OLD + 600)
        node_project(p('cachey'))
        put(p('cachey/node_modules/.cache/babel.json'), '{}')
        put(p('cachey/node_modules/.vitest-cache/results.json'), '{}')

        put(p('novenv/.venv/pyvenv.cfg'), 'x', OLD)
        put(p('novenv/main.py'), 'x', OLD)
        put(p('loose/build/out.txt'), 'x', OLD)
        put(p('stray/node_modules/x/index.js'), 'x', OLD)

        node_project(p('busy'))
        cls.sleeper = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(300)'], cwd=p('busy', 'src'))

        node_project(p('vscode'))
        put(p('Library/Application Support/Code/User/globalStorage/storage.json'), json.dumps(
            {'windowsState': {'lastActiveWindow': {'folder': 'file://' + p('vscode').replace(' ', '%20')}}}))
        node_project(p('vscdb'))
        con = sqlite3.connect(p('Library/Application Support/Code/User/globalStorage/state.vscdb'))
        con.execute('CREATE TABLE ItemTable (key TEXT UNIQUE ON CONFLICT REPLACE, value BLOB)')
        con.execute('INSERT INTO ItemTable VALUES (?, ?)', ('history.recentlyOpenedPathsList', json.dumps(
            {'entries': [{'folderUri': 'file://' + p('vscdb')}]})))
        con.commit()
        con.close()

        repo = p('repo')
        put(os.path.join(repo, 'README.md'), 'x')
        put(os.path.join(repo, 'package.json'), '{}')
        put(os.path.join(repo, '.gitignore'), 'node_modules/\n.env\n')
        git(repo, 'init', '-q')
        git(repo, 'add', '-A')
        git(repo, 'commit', '-qm', 'init', when=OLD)
        for name in ('clean', 'dirty', 'ahead', 'secret'):
            git(repo, 'worktree', 'add', '-q', '-b', name, f'.claude/worktrees/{name}')
        wt = lambda *parts: p('repo/.claude/worktrees', *parts)  # noqa: E731
        put(wt('clean/node_modules/dep/index.js'))
        put(wt('dirty/notes.md'), 'not committed')
        put(wt('dirty/node_modules/dep/index.js'))
        put(wt('ahead/feature.txt'))
        git(wt('ahead'), 'add', '-A')
        git(wt('ahead'), 'commit', '-qm', 'unmerged work')
        put(wt('secret/.env'), 'TOKEN=test')

        put(p('.vscode/extensions/foo/package.json'), '{}')
        put(p('.vscode/extensions/foo/node_modules/dep/index.js'))
        for v in ('1.0.9', '1.0.10', '1.0.11'):
            put(p('.local/share/claude/versions', v), 'bin')
        os.makedirs(p('.local/bin'))
        os.symlink(p('.local/share/claude/versions/1.0.10'), p('.local/bin/claude'))
        put(p('.claude/projects/x/session.jsonl'), '{}')
        put(p('Library/Application Support/Cursor/Cache/data_0'), 'x')
        put(p('Library/Application Support/Codex/GPUCache/data_0'), 'x')
        dd = p('Library/Developer/Xcode/DerivedData/Gone-abc')
        os.makedirs(dd)
        with open(os.path.join(dd, 'info.plist'), 'wb') as f:
            plistlib.dump({'WorkspacePath': p('gone/Gone.xcodeproj')}, f)

        # Real processes (the sleeper among them) minus real apps, plus a pretend running Cursor.
        procs = [x for x in census.processes() if not any(f'/{a}.app/' in x['comm'] for a in census.AI_APPS)]
        procs.append({'pid': 1, 'tty': '??', 'comm': '/Applications/Cursor.app/Contents/MacOS/Cursor',
                      'args': '', 'cwd': '/'})
        cls.before = tree(cls.tmp)
        cls.result = census.Census(home, 14, procs=procs).run()
        cls.after = tree(cls.tmp)
        cls.by_path = {i['path']: i for i in cls.result['items']}

    @classmethod
    def p(cls, *parts):
        return os.path.join(cls.home, *parts)

    @classmethod
    def tearDownClass(cls):
        cls.sleeper.kill()
        cls.sleeper.wait()
        shutil.rmtree(cls.tmp, True)

    def status(self, *parts):
        item = self.by_path.get(self.p(*parts))
        self.assertIsNotNone(item, f'{parts} not listed')
        return item['status']

    def test_census_writes_nothing(self):
        self.assertEqual(self.before, self.after)

    def test_cold_project_output(self):
        self.assertEqual(self.status('cold/node_modules'), 'cold')
        self.assertEqual(self.status('cold/.venv'), 'cold')
        self.assertEqual(self.status('cold/build'), 'cold')

    def test_hard_link_counted_once(self):
        size = self.by_path[self.p('cold/node_modules')]['bytes']
        self.assertGreaterEqual(size, 100_000)
        self.assertLess(size, 200_000)

    def test_recent_by_file_commit_and_editor(self):
        self.assertEqual(self.status('fresh/node_modules'), 'recent')
        self.assertEqual(self.status('committed/node_modules'), 'recent')
        self.assertIn('last commit', self.by_path[self.p('committed/node_modules')]['why'])
        self.assertEqual(self.status('vscode/node_modules'), 'recent')
        self.assertEqual(self.status('vscdb/node_modules'), 'recent')
        self.assertIn('VS Code', self.by_path[self.p('vscdb/node_modules')]['why'])

    def test_build_folders(self):
        self.assertEqual(self.status('untracked/build'), 'untracked_work')
        self.assertEqual(self.status('tracked/build'), 'protected')
        self.assertNotIn(self.p('loose/build'), self.by_path)  # no git: ambiguous name, not listed

    def test_hand_edited_node_modules(self):
        self.assertEqual(self.status('edited/node_modules'), 'hand_edited')
        self.assertEqual(self.status('cachey/node_modules'), 'cold')  # only the build cache is newer

    def test_no_manifest(self):
        self.assertEqual(self.status('novenv/.venv'), 'no_manifest')
        self.assertEqual(self.status('stray/node_modules'), 'no_manifest')

    def test_busy(self):
        self.assertEqual(self.status('busy/node_modules'), 'busy')
        self.assertIn(str(self.sleeper.pid), self.by_path[self.p('busy/node_modules')]['why'])

    def test_worktrees(self):
        wt = 'repo/.claude/worktrees'
        self.assertEqual(self.status(wt, 'clean'), 'cold')
        self.assertNotIn(self.p(wt, 'clean/node_modules'), self.by_path)  # inside the cold worktree
        self.assertEqual(self.status(wt, 'dirty'), 'protected')
        self.assertIn('uncommitted', self.by_path[self.p(wt, 'dirty')]['why'])
        self.assertEqual(self.status(wt, 'dirty/node_modules'), 'recent')  # listed on its own
        self.assertEqual(self.status(wt, 'ahead'), 'protected')
        self.assertIn('unpushed', self.by_path[self.p(wt, 'ahead')]['why'])
        self.assertEqual(self.status(wt, 'secret'), 'protected')
        self.assertIn('.env', self.by_path[self.p(wt, 'secret')]['why'])

    def test_tool_data_and_history(self):
        self.assertEqual(self.status('.vscode/extensions/foo/node_modules'), 'protected')
        self.assertEqual(self.status('.claude/projects'), 'protected')

    def test_claude_versions(self):
        self.assertEqual(self.status('.local/share/claude/versions/1.0.9'), 'cold')
        self.assertEqual(self.status('.local/share/claude/versions/1.0.10'), 'protected')
        self.assertEqual(self.status('.local/share/claude/versions/1.0.11'), 'protected')

    def test_app_caches_and_derived_data(self):
        self.assertEqual(self.status('Library/Application Support/Cursor/Cache'), 'busy')
        self.assertEqual(self.status('Library/Application Support/Codex/GPUCache'), 'cold')
        self.assertEqual(self.status('Library/Developer/Xcode/DerivedData/Gone-abc'), 'cold')

    def test_totals_add_up(self):
        t = self.result['totals']['by_status']
        self.assertEqual(sum(v['count'] for v in t.values()), len(self.result['items']))
        self.assertEqual(sum(v['bytes'] for v in t.values()), sum(i['bytes'] for i in self.result['items']))


class HelperTest(unittest.TestCase):
    def test_docker_bytes(self):
        self.assertEqual(census.docker_bytes('1.5GB (40%)'), 1_500_000_000)
        self.assertEqual(census.docker_bytes('12.3kB'), 12_300)
        self.assertEqual(census.docker_bytes('0B'), 0)

    def test_interactive_shell(self):
        self.assertTrue(census.interactive_shell({'comm': '-zsh', 'tty': 'ttys001'}))
        self.assertFalse(census.interactive_shell({'comm': '/bin/zsh', 'tty': '??'}))
        self.assertFalse(census.interactive_shell({'comm': 'node', 'tty': 'ttys001'}))

    def test_rebuildable_and_versions(self):
        self.assertTrue(census.rebuildable('node_modules/'))
        self.assertTrue(census.rebuildable('pkg/dist/'))
        self.assertFalse(census.rebuildable('.env'))
        self.assertGreater(census.version_key('1.0.10'), census.version_key('1.0.9'))

    def test_main_writes_json(self):
        tmp = os.path.realpath(tempfile.mkdtemp(prefix='census-main-'))
        self.addCleanup(shutil.rmtree, tmp, True)
        os.makedirs(os.path.join(tmp, 'home'))
        out = os.path.join(tmp, 'out.json')
        census.main(['--home', os.path.join(tmp, 'home'), '--out', out])
        with open(out) as f:
            data = json.load(f)
        self.assertEqual(data['items'], [])
        self.assertEqual(set(data) >= {'items', 'skipped', 'totals', 'elapsed_secs'}, True)


if __name__ == '__main__':
    unittest.main()
