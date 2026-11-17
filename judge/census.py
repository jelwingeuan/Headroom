#!/usr/bin/env python3
"""Independent, read-only census of the rebuildable disk usage in a macOS home folder.

  python3 -I judge/census.py --home ~ --out FILE.json [--recent-days 14]

A second opinion on Headroom, derived only from the filesystem, git, ps, lsof, xcrun (and docker
when installed). It opens nothing for writing except --out: git runs as /usr/bin/git with optional
locks off and fsmonitor and hooks disabled, and SQLite is opened read-only and immutable.

Each item is {path, category, bytes, status, why}: bytes is allocated space (st_blocks * 512) with a
hard-linked file counted once per census. Statuses, most cautious first: protected, untracked_work,
hand_edited, no_manifest, busy, recent, cold (safe to rebuild or delete), unknown.
"""
import sys

sys.dont_write_bytecode = True  # leave no .pyc behind

import argparse  # noqa: E402
import json  # noqa: E402
import os  # noqa: E402
import plistlib  # noqa: E402
import re  # noqa: E402
import shutil  # noqa: E402
import sqlite3  # noqa: E402
import stat  # noqa: E402
import subprocess  # noqa: E402
import time  # noqa: E402
from datetime import datetime, timezone  # noqa: E402
from urllib.parse import quote, unquote, urlparse  # noqa: E402

DAY = 86400
SIM_IDLE_DAYS = 30
HAND_EDIT_SLACK_S = 60
GIT = '/usr/bin/git'
GIT_ARGS = ['-c', 'core.fsmonitor=false', '-c', 'core.hooksPath=/dev/null']
GIT_ENV = {'GIT_OPTIONAL_LOCKS': '0', 'GIT_TERMINAL_PROMPT': '0'}
AMBIGUOUS = {'build', 'dist', 'out'}
# Build output and VCS data: never "the project" when asking whether it was used lately.
BUILD_NAMES = {'node_modules', '.next', 'target', '.venv', 'venv', '.build', 'build', 'dist', 'out',
               '__pycache__', 'DerivedData', '.git'}
JUNK = {'.DS_Store'}
PY_MANIFESTS = {'pyproject.toml', 'Pipfile', 'setup.py'}
SHELLS = {'sh', 'bash', 'zsh', 'fish', 'tcsh', 'csh', 'ksh', 'dash', 'nu', 'xonsh', 'login'}
AI_APPS = ('Claude', 'Codex', 'Cursor')
AI_CACHES = ('Cache', 'Code Cache', 'GPUCache')
EDITORS = {'Code': 'VS Code', 'Cursor': 'Cursor'}
HISTORY = ('.claude/projects', '.codex/sessions', '.codex/archived_sessions')
DOCKER_PATHS = ('/usr/local/bin/docker', '/opt/homebrew/bin/docker',
                '/Applications/Docker.app/Contents/Resources/bin/docker')
DOCKER_UNITS = {'B': 1, 'KB': 10 ** 3, 'MB': 10 ** 6, 'GB': 10 ** 9, 'TB': 10 ** 12, 'PB': 10 ** 15}
REFLOG = re.compile(rb'> (\d+) [+-]\d{4}\t([^:]*)')
XCODE = re.compile(r'/Xcode[^/]*\.app/Contents/MacOS/Xcode$')


def inside(path, root):
    return path == root or path.startswith(root.rstrip('/') + '/')


def err(e):
    return e.strerror or type(e).__name__


def run(cmd, env=None, timeout=120):
    """(returncode, stdout bytes) of a command run from /; returncode None if it could not run."""
    try:
        p = subprocess.run(cmd, cwd='/', env=env, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                           stderr=subprocess.DEVNULL, timeout=timeout)
        return p.returncode, p.stdout
    except (OSError, subprocess.SubprocessError):
        return None, b''


def git(repo, *args):
    return run([GIT, *GIT_ARGS, '-C', repo, *args], env=dict(os.environ, **GIT_ENV))


def mtime(path):
    try:
        return os.lstat(path).st_mtime
    except OSError:
        return 0.0


def stamp(st, touch):
    return max(st.st_mtime, st.st_atime) if touch else st.st_mtime


def read_json(path):
    try:
        with open(path, 'rb') as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def iso_time(s):
    try:
        return datetime.fromisoformat(s.replace('Z', '+00:00')).timestamp()
    except (AttributeError, ValueError):
        return 0.0


def file_uris(obj):
    """Local paths of every file:// string anywhere inside a JSON value."""
    if isinstance(obj, str):
        if obj.startswith('file://'):
            yield os.path.normpath(unquote(urlparse(obj).path))
    elif isinstance(obj, dict):
        for v in obj.values():
            yield from file_uris(v)
    elif isinstance(obj, list):
        for v in obj:
            yield from file_uris(v)


def version_key(name):
    return tuple(int(x) if x.isdigit() else -1 for x in re.split(r'[.+-]', name))


def docker_bytes(text):
    m = re.match(r'\s*([\d.]+)\s*([kKMGTP]?B)', text or '')
    return int(float(m[1]) * DOCKER_UNITS[m[2].upper()]) if m else 0


def rebuildable(rel):
    """True for a git-ignored path that is build output or OS junk (fine to lose with a worktree)."""
    parts = rel.rstrip('/').split('/')
    return bool(BUILD_NAMES & set(parts)) or parts[-1] in JUNK or parts[-1].endswith('.pyc')


def worktree_parent(d):
    """*/.claude/worktrees, */.codex/worktrees and */.worktrees, or a non-repo folder right below one."""
    def container(x):
        b = os.path.basename(x)
        return b == '.worktrees' or (b == 'worktrees' and os.path.basename(os.path.dirname(x)) in ('.claude', '.codex'))
    return container(d) or (container(os.path.dirname(d)) and not os.path.lexists(os.path.join(d, '.git')))


def interactive_shell(p):
    return os.path.basename(p['comm']).lstrip('-') in SHELLS and p['tty'] not in ('??', '')


def processes():
    """Every process this user can inspect, except this one: pid, tty, comm (executable), args, cwd."""
    procs = {}
    _, out = run(['/bin/ps', '-axww', '-o', 'pid=,tty=,comm='])
    for line in os.fsdecode(out).splitlines():
        f = line.split(None, 2)
        if len(f) == 3 and f[0].isdigit():
            procs[int(f[0])] = {'pid': int(f[0]), 'tty': f[1], 'comm': f[2], 'args': '', 'cwd': None}
    _, out = run(['/bin/ps', '-axww', '-o', 'pid=,args='])
    for line in os.fsdecode(out).splitlines():
        f = line.split(None, 1)
        if len(f) == 2 and f[0].isdigit() and int(f[0]) in procs:
            procs[int(f[0])]['args'] = f[1]
    _, out = run(['/usr/sbin/lsof', '-nP', '-w', '-d', 'cwd', '-Fpn'])
    pid = None
    for line in os.fsdecode(out).splitlines():
        if line[:1] == 'p' and line[1:].isdigit():
            pid = int(line[1:])
        elif line[:1] == 'n' and pid in procs:
            procs[pid]['cwd'] = line[1:]
    procs.pop(os.getpid(), None)
    return list(procs.values())


def totals(items):
    """Count and bytes by status and by category; space outside the Mac's disk (Docker VM) apart."""
    out = {'by_status': {}, 'by_category': {}, 'off_host': {'count': 0, 'bytes': 0}}
    for it in items:
        if it.get('frees_host') is False:
            out['off_host']['count'] += 1
            out['off_host']['bytes'] += it['bytes']
            continue
        for key, k in (('by_status', it['status']), ('by_category', it['category'])):
            t = out[key].setdefault(k, {'count': 0, 'bytes': 0})
            t['count'] += 1
            t['bytes'] += it['bytes']
    return out


class Census:
    def __init__(self, home, recent_days=14, procs=None, now=None):
        self.home = os.path.realpath(home)
        self.days = recent_days
        self.now = time.time() if now is None else now
        self.cutoff = self.now - recent_days * DAY
        self.procs = processes() if procs is None else procs
        self.items, self.skipped = [], []
        self.seen = set()  # (st_dev, st_ino) of hard-linked files already counted
        self.tops = {}  # folder -> enclosing git work tree, or None
        self.used = {}  # project root -> (used within the window?, why)
        self.pending = []  # agent worktrees that are not cold: sized after the walk, minus items inside
        self.editor = {}

    # ---- helpers

    def h(self, path):
        return '~' + path[len(self.home):] if inside(path, self.home) else path

    def ago(self, t):
        return f'{(self.now - t) / DAY:.1f} d ago'

    def skip(self, path, reason):
        self.skipped.append({'path': path, 'reason': reason})

    def add(self, path, category, status, why, size=None, **extra):
        size = self.du(path)[0] if size is None else size
        self.items.append({'path': path, 'category': category, 'bytes': size, 'status': status, 'why': why, **extra})

    def blocks(self, st):
        if st.st_nlink > 1 and not stat.S_ISDIR(st.st_mode):
            key = (st.st_dev, st.st_ino)
            if key in self.seen:
                return 0
            self.seen.add(key)
        return st.st_blocks * 512

    def du(self, root, exclude=frozenset(), skip_newest=frozenset(), touch=False):
        """(allocated bytes, newest file time, newest file) under root. Paths in skip_newest (and all
        below them) are sized but never newest; touch takes max(mtime, atime) as a file's time."""
        try:
            st = os.lstat(root)
        except OSError as e:
            self.skip(root, err(e))
            return 0, 0.0, None
        total = self.blocks(st)
        if not stat.S_ISDIR(st.st_mode):
            return total, stamp(st, touch), root
        newest, newest_path = 0.0, None
        stack = [(root, True)]
        while stack:
            d, count = stack.pop()
            try:
                with os.scandir(d) as it:
                    entries = list(it)
            except OSError as e:
                self.skip(d, err(e))
                continue
            for e in entries:
                if e.path in exclude:
                    continue
                try:
                    st = e.stat(follow_symlinks=False)
                except OSError:
                    continue
                total += self.blocks(st)
                if stat.S_ISDIR(st.st_mode):
                    stack.append((e.path, count and e.path not in skip_newest))
                elif count and stat.S_ISREG(st.st_mode) and stamp(st, touch) > newest and e.path not in skip_newest:
                    newest, newest_path = stamp(st, touch), e.path
        return total, newest, newest_path

    def subdirs(self, path):
        """Sorted child folders of path (no symlinks); [] when it is missing, noted when unreadable."""
        try:
            with os.scandir(path) as it:
                return sorted(e.path for e in it if e.is_dir(follow_symlinks=False))
        except FileNotFoundError:
            return []
        except OSError as e:
            self.skip(path, err(e))
            return []

    def files(self, root):
        out, stack = [], [root]
        while stack:
            d = stack.pop()
            try:
                with os.scandir(d) as it:
                    for e in it:
                        if e.is_dir(follow_symlinks=False):
                            stack.append(e.path)
                        elif e.is_file(follow_symlinks=False):
                            out.append(e.path)
            except FileNotFoundError:
                continue
            except OSError as e:
                self.skip(d, err(e))
        return sorted(out)

    # ---- projects

    def toplevel(self, d):
        """The git work tree enclosing d (nearest folder with a .git entry), or None."""
        chain, x = [], d
        while True:
            if x in self.tops:
                top = self.tops[x]
                break
            chain.append(x)
            if os.path.lexists(os.path.join(x, '.git')):
                top = x
                break
            parent = os.path.dirname(x)
            if parent == x:
                top = None
                break
            x = parent
        for c in chain:
            self.tops[c] = top
        return top

    def project_root(self, d):
        top = self.toplevel(d)
        return top if top and not inside(self.home, top) else d

    def busy(self, root):
        for p in self.procs:
            if p['cwd'] and inside(p['cwd'], root) and not interactive_shell(p):
                return f"pid {p['pid']} ({os.path.basename(p['comm'])}) works in {self.h(p['cwd'])}"
        return None

    def recency(self, root):
        """(used within the window?, why) for a project folder."""
        if root not in self.used:
            self.used[root] = self._recency(root)
        return self.used[root]

    def _recency(self, root):
        for path, (t, app) in self.editor.items():
            if t >= self.cutoff and (inside(path, root) or (inside(root, path) and not inside(self.home, path))):
                return True, f'{app} had {self.h(path)} open {self.ago(t)}'
        facts = []
        if os.path.lexists(os.path.join(root, '.git')):
            rc, out = git(root, 'log', '-1', '--format=%ct', 'HEAD')
            if rc == 0 and out.strip().isdigit():
                t = int(out)
                if t >= self.cutoff:
                    return True, f'last commit {self.ago(t)}'
                facts.append(f'last commit {self.ago(t)}')
            t, action = self.last_reflog(root)
            if t >= self.cutoff:
                return True, f'reflog "{action}" {self.ago(t)}'
        t, path = self.newest_file(root)
        if t >= self.cutoff:
            return True, f'{self.h(path)} modified {self.ago(t)}'
        facts.append(f'newest file {self.ago(t)}' if path else 'no files')
        return False, f'{self.h(root)} idle {self.days} d+: ' + ', '.join(facts)

    def last_reflog(self, root):
        """(time, action) of the newest HEAD reflog entry (commit, checkout, merge, pull, reset...)."""
        gitdir = os.path.join(root, '.git')
        if os.path.isfile(gitdir):
            try:
                with open(gitdir, 'rb') as f:
                    line = f.readline().decode(errors='replace').strip()
            except OSError:
                return 0, ''
            if not line.startswith('gitdir:'):
                return 0, ''
            gitdir = os.path.join(root, line[7:].strip())
        try:
            with open(os.path.join(gitdir, 'logs', 'HEAD'), 'rb') as f:
                f.seek(0, 2)
                f.seek(max(0, f.tell() - 65536))
                tail = f.read()
        except OSError:
            return 0, ''
        best = (0, '')
        for line in tail.splitlines():
            m = REFLOG.search(line)
            if m and int(m[1]) > best[0]:
                best = (int(m[1]), m[2].decode(errors='replace'))
        return best

    def newest_file(self, root):
        """Newest regular file in a project outside build output and .git; stops at the first one
        inside the window."""
        best, best_path, stack = 0.0, None, [root]
        while stack:
            d = stack.pop()
            try:
                with os.scandir(d) as it:
                    entries = list(it)
            except OSError:
                continue
            for e in entries:
                try:
                    if e.is_dir(follow_symlinks=False):
                        if e.name not in BUILD_NAMES and not e.name.endswith('.app'):
                            stack.append(e.path)
                    elif e.is_file(follow_symlinks=False) and e.name not in JUNK:
                        t = e.stat(follow_symlinks=False).st_mtime
                        if t > best:
                            best, best_path = t, e.path
                        if t >= self.cutoff:
                            return best, best_path
                except OSError:
                    continue
        return best, best_path

    def candidate(self, d, names, n, p, tool):
        """Lists p when it is project build output or deps; True when listed (the walk skips it then)."""
        if n == 'node_modules':
            cat = 'node_modules'
        elif n == '.next':
            cat = 'next'
        elif n == 'target' and 'Cargo.toml' in names:
            cat = 'cargo_target'
        elif n in ('.venv', 'venv') and os.path.isfile(os.path.join(p, 'pyvenv.cfg')):
            cat = 'venv'
        elif n == '.build' and 'Package.swift' in names:
            cat = 'swift_build'
        elif n in AMBIGUOUS and not tool:
            cat = 'build_output'
        else:
            return False
        if tool:
            self.add(p, cat, 'protected', f'inside {self.h(tool)}: tool or app data, not a project')
            return True
        top = self.toplevel(d)
        if top:
            rel = os.path.relpath(p, top)
            rc, out = git(top, '--literal-pathspecs', 'ls-files', '-z', '--', rel)
            if rc != 0:
                self.add(p, cat, 'unknown', f'git ls-files failed in {self.h(top)}')
                return True
            if out:
                count, first = out.count(b'\0'), os.fsdecode(out.split(b'\0')[0])
                self.add(p, cat, 'protected', f'{count} git-tracked file(s) inside, e.g. {first}')
                return True
            if cat == 'build_output' and git(top, 'check-ignore', '-q', '--', rel + '/')[0] != 0:
                _, out = git(top, '--literal-pathspecs', 'ls-files', '-z', '--others', '--exclude-standard',
                             '--directory', '--no-empty-directory', '--', rel)
                if not out:
                    return False
                first = os.fsdecode(out.split(b'\0')[0])
                self.add(p, cat, 'untracked_work', f'untracked files git does not ignore, e.g. {first}')
                return True
        elif cat == 'build_output':
            return False
        skip = frozenset()
        if cat == 'node_modules':  # top-level dot entries (.cache, .vite, .prisma...) are tool output, not packages
            try:
                skip = {os.path.join(p, x) for x in os.listdir(p) if x.startswith('.') and x != '.pnpm'}
            except OSError:
                pass
        size, newest, newest_path = self.du(p, skip_newest=skip)
        status, why = self.judge(cat, names, p, self.project_root(d), newest, newest_path)
        self.add(p, cat, status, why, size)
        return True

    def judge(self, cat, names, p, root, newest, newest_path):
        if cat == 'node_modules':
            lock = mtime(os.path.join(p, '.package-lock.json'))
            if lock and newest > lock + HAND_EDIT_SLACK_S:
                return 'hand_edited', (f'{os.path.relpath(newest_path, p)} is {newest - lock:.0f} s newer '
                                       'than .package-lock.json')
        if cat in ('node_modules', 'next') and 'package.json' not in names:
            return 'no_manifest', 'no package.json beside it'
        if cat == 'venv' and not (names & PY_MANIFESTS or any(
                x.startswith('requirements') and x.endswith('.txt') for x in names)):
            return 'no_manifest', 'no requirements*.txt, pyproject.toml, Pipfile or setup.py beside it'
        b = self.busy(root)
        if b:
            return 'busy', b
        recent, why = self.recency(root)
        return ('recent' if recent else 'cold'), why

    def worktree(self, w):
        """Judges agent worktree w; True when it is cold (listed whole, the walk skips it)."""
        reasons, merged = [], None
        rc, out = git(w, 'status', '--porcelain=v1', '-z', '--untracked-files=normal', '--ignored=traditional')
        if rc != 0:
            reasons.append('git status failed (worktree metadata missing?)')
        dirty, ignored, recs, i = [], [], out.split(b'\0'), 0
        while i < len(recs):
            r = recs[i]
            i += 1
            if len(r) < 4:
                continue
            code, path = r[:2].decode(errors='replace'), os.fsdecode(r[3:])
            if 'R' in code or 'C' in code:
                i += 1
            if code == '!!':
                if not rebuildable(path):
                    ignored.append(path)
            else:
                dirty.append(path)
        if dirty:
            reasons.append(f'{len(dirty)} uncommitted change(s), e.g. {dirty[0]}')
        if ignored:
            reasons.append(f'{len(ignored)} ignored non-rebuildable file(s), e.g. {ignored[0]}')
        rc, out = git(w, 'rev-list', '--count', 'HEAD', '--not', '--remotes')
        if rc != 0 or not out.strip().isdigit():
            reasons.append('HEAD has no commit')
        elif int(out):
            merged = next((b for b in ('main', 'master', 'origin/main', 'origin/master')
                           if git(w, 'merge-base', '--is-ancestor', 'HEAD', b)[0] == 0), None)
            if not merged:
                reasons.append(f'{int(out)} unpushed commit(s) not merged into main/master')
        if reasons:
            self.pending.append((w, 'protected', '; '.join(reasons)))
            return False
        b = self.busy(w)
        if b:
            self.pending.append((w, 'busy', b))
            return False
        self.add(w, 'worktree', 'cold', 'clean, nothing ignored but build output, '
                 + (f'HEAD merged into {merged}' if merged else 'no unpushed commits'))
        return True

    def walk(self):
        home = self.home
        special = {os.path.join(home, p) for p in ('Library', '.ollama/models', '.cache/huggingface/hub',
                                                    '.lmstudio/models', '.local/share/claude/versions', *HISTORY)}
        stack = [(home, None)]  # (folder, home dot-folder it lies in, if any)
        while stack:
            d, tool = stack.pop()
            try:
                with os.scandir(d) as it:
                    entries = sorted(it, key=lambda e: e.name)
            except OSError as e:
                self.skip(d, err(e))
                continue
            names = {e.name for e in entries}
            agents = worktree_parent(d)
            for e in reversed(entries):
                try:
                    if not e.is_dir(follow_symlinks=False):
                        continue
                except OSError:
                    continue
                p, n = e.path, e.name
                if p in special or n == '.git' or n.endswith('.app'):
                    continue
                if agents and os.path.lexists(os.path.join(p, '.git')):
                    if not self.worktree(p):
                        stack.append((p, None))
                    continue
                if self.candidate(d, names, n, p, tool) or n == 'node_modules':
                    continue
                stack.append((p, tool or (p if d == home and n.startswith('.') else None)))
        for w, status, why in self.pending:
            inner = {i['path'] for i in self.items if inside(i['path'], w) and i['path'] != w}
            if inner:
                why += f'; {len(inner)} item(s) inside listed separately and not counted here'
            self.add(w, 'worktree', status, why, self.du(w, exclude=inner)[0])

    # ---- editors

    def editor_recents(self):
        """{path: (time, editor)} for folders and files VS Code or Cursor had open."""
        found = {}

        def note(paths, t, app):
            for p in paths:
                if p not in found or found[p][0] < t:
                    found[p] = (t, app)

        for app, label in EDITORS.items():
            user = os.path.join(self.home, 'Library', 'Application Support', app, 'User')
            if not os.path.isdir(user):
                continue
            for d in self.subdirs(os.path.join(user, 'workspaceStorage')):
                t = max(mtime(os.path.join(d, 'state.vscdb')), mtime(d))
                note(file_uris(read_json(os.path.join(d, 'workspace.json'))), t, label)
            storage = os.path.join(user, 'globalStorage', 'storage.json')
            data = read_json(storage)
            if isinstance(data, dict):
                note(file_uris([data.get('windowsState'), data.get('backupWorkspaces')]), mtime(storage), label)
            db = os.path.join(user, 'globalStorage', 'state.vscdb')
            if not os.path.isfile(db):
                continue
            try:
                con = sqlite3.connect(f'file:{quote(db)}?mode=ro&immutable=1', uri=True)
                try:
                    row = con.execute("SELECT value FROM ItemTable WHERE key = 'history.recentlyOpenedPathsList'"
                                      ).fetchone()
                finally:
                    con.close()
                for p in file_uris(json.loads(row[0]) if row else None):
                    if p not in found:  # no per-entry time: fall back to when the list was last written
                        found[p] = (mtime(db), label)
            except (sqlite3.Error, ValueError) as e:
                self.skip(db, f'sqlite: {e}')
        return found

    # ---- fixed places

    def derived_data(self):
        xcode = next((p for p in self.procs if XCODE.search(p['comm'])), None)
        for d in self.subdirs(os.path.join(self.home, 'Library/Developer/Xcode/DerivedData')):
            try:
                with open(os.path.join(d, 'info.plist'), 'rb') as f:
                    info = plistlib.load(f)
            except (OSError, plistlib.InvalidFileException, ValueError):
                info = {}
            ws, last = info.get('WorkspacePath'), info.get('LastAccessedDate')
            last = last.replace(tzinfo=timezone.utc).timestamp() if isinstance(last, datetime) else 0.0
            if not isinstance(ws, str):
                status, why = ('busy', f"Xcode is running (pid {xcode['pid']})") if xcode else (
                    'cold', 'shared Xcode cache, no workspace')
            elif not os.path.exists(ws):
                status, why = 'cold', f'workspace {self.h(ws)} no longer exists'
            else:
                root = self.project_root(os.path.dirname(ws) if ws.endswith(('.xcodeproj', '.xcworkspace')) else ws)
                b = self.busy(root)
                if b:
                    status, why = 'busy', b
                elif last >= self.cutoff:
                    status, why = 'recent', f'Xcode opened {self.h(ws)} {self.ago(last)}'
                else:
                    recent, why = self.recency(root)
                    status = 'recent' if recent else 'cold'
            self.add(d, 'derived_data', status, why)

    def simulators(self):
        devdir = os.path.join(self.home, 'Library/Developer/CoreSimulator/Devices')
        folders = self.subdirs(devdir)
        if not folders:
            return
        rc, out = run(['/usr/bin/xcrun', 'simctl', 'list', '-j', 'devices'])
        try:
            devices = [(rt, dev) for rt, devs in json.loads(out)['devices'].items() for dev in devs]
        except (ValueError, KeyError, TypeError, AttributeError):
            devices = []
            self.skip(devdir, 'xcrun simctl list failed')
        known = {}
        for rt, dev in devices:
            folder = os.path.join(devdir, str(dev.get('udid')))
            if folder in folders:
                try:
                    born = os.stat(folder).st_birthtime
                except OSError:
                    born = 0.0
                known[folder] = (rt, dev, iso_time(dev.get('lastUsedAt')) or born)
        newest = {}  # deviceTypeIdentifier -> the folder of its most recently used device
        for folder, (_, dev, used) in known.items():
            t = dev.get('deviceTypeIdentifier')
            if t not in newest or used > known[newest[t]][2]:
                newest[t] = folder
        for folder in folders:
            if folder not in known:
                self.add(folder, 'simulator', 'unknown', 'device folder not in xcrun simctl list')
                continue
            rt, dev, used = known[folder]
            label = f"{dev.get('name')} ({rt.rsplit('.', 1)[-1]})"
            src = 'last used' if dev.get('lastUsedAt') else 'created (never used)'
            if dev.get('state') != 'Shutdown':
                status, why = 'busy', f"{label} is {dev.get('state')}"
            elif newest.get(dev.get('deviceTypeIdentifier')) == folder:
                status, why = 'protected', f'{label}: most recently used of its device type, {src} {self.ago(used)}'
            elif self.now - used < SIM_IDLE_DAYS * DAY:
                status, why = 'recent', f'{label}: {src} {self.ago(used)}'
            else:
                status, why = 'cold', f'{label}: shut down, {src} {self.ago(used)}'
            self.add(folder, 'simulator', status, why)

    def app_caches(self):
        for app in AI_APPS:
            running = next((p for p in self.procs if f'/{app}.app/Contents/MacOS/' in p['comm']), None)
            for c in AI_CACHES:
                p = os.path.join(self.home, 'Library', 'Application Support', app, c)
                if os.path.isdir(p) and not os.path.islink(p):
                    if running:
                        self.add(p, 'app_cache', 'busy', f"{app} is running (pid {running['pid']})")
                    else:
                        self.add(p, 'app_cache', 'cold', f'{app} is not running; Chromium cache, rebuilt on demand')

    def claude_versions(self):
        vdir = os.path.join(self.home, '.local/share/claude/versions')
        try:
            with os.scandir(vdir) as it:
                entries = sorted(e.path for e in it if not e.name.startswith('.'))
        except FileNotFoundError:
            return
        except OSError as e:
            self.skip(vdir, err(e))
            return
        if not entries:
            return
        link = os.path.join(self.home, '.local/bin/claude')
        target = os.path.realpath(link) if os.path.lexists(link) else None
        newest = max(entries, key=lambda p: (version_key(os.path.basename(p)), mtime(p)))
        regular = [p for p in entries if os.path.isfile(p) and not os.path.islink(p)]
        _, out = run(['/usr/sbin/lsof', '-nP', '-w', '-Fpn', '--', *regular]) if regular else (0, b'')
        open_now = {line[1:] for line in os.fsdecode(out).splitlines() if line[:1] == 'n'}
        for p in entries:
            user = next((q for q in self.procs if inside(q['comm'], p)
                         or any(inside(tok, p) for tok in q['args'].split())), None)
            if os.path.realpath(p) == target:
                status, why = 'protected', 'target of ~/.local/bin/claude'
            elif p == newest:
                status, why = 'protected', 'newest installed version'
            elif p in open_now or user:
                status, why = 'busy', f"in use by pid {user['pid']}" if user else 'open in a running process (lsof)'
            else:
                status, why = 'cold', 'not the ~/.local/bin/claude target, not the newest, no process runs it'
            self.add(p, 'claude_cli', status, why)

    def ollama(self):
        models = os.path.join(self.home, '.ollama/models')
        mdir, counted = os.path.join(models, 'manifests'), set()
        for f in self.files(mdir):
            rel = os.path.relpath(f, mdir)
            data = read_json(f)
            layers = [data.get('config') or {}] + list(data.get('layers') or []) if isinstance(data, dict) else []
            blobs = [os.path.join(models, 'blobs', x['digest'].replace(':', '-')) for x in layers
                     if isinstance(x, dict) and isinstance(x.get('digest'), str)]
            size, touched = 0, mtime(f)
            for b in blobs:
                try:
                    touched = max(touched, stamp(os.lstat(b), True))
                except OSError:
                    continue
                if b not in counted:  # a layer shared by several models counts with the first
                    counted.add(b)
                    size += self.du(b)[0]
            name = ':'.join(rel.rsplit('/', 1))
            if not rel.startswith(('registry.ollama.ai/library/', 'hf.co/')):
                status, why = 'protected', f'{name}: not from registry.ollama.ai/library or hf.co, may not re-pull'
            elif touched >= self.cutoff:
                status, why = 'recent', f'{name}: touched {self.ago(touched)}'
            else:
                status, why = 'cold', f'{name}: re-pullable, untouched since {self.ago(touched)}'
            self.add(f, 'ollama', status, why, size)

    def huggingface(self):
        for p in self.subdirs(os.path.join(self.home, '.cache/huggingface/hub')):
            if os.path.basename(p).startswith('models--'):
                size, touched, _ = self.du(p, touch=True)
                status = 'recent' if touched >= self.cutoff else 'cold'
                self.add(p, 'huggingface', status, f're-downloadable, touched {self.ago(touched)}', size)

    def lmstudio(self):
        for pub in self.subdirs(os.path.join(self.home, '.lmstudio/models')):
            for p in self.subdirs(pub):
                size, touched, _ = self.du(p, touch=True)
                if os.path.basename(pub) != 'lmstudio-community':
                    status, why = 'protected', 'not provably a download (only lmstudio-community/* counts as one)'
                else:
                    status = 'recent' if touched >= self.cutoff else 'cold'
                    why = f'LM Studio download, touched {self.ago(touched)}'
                self.add(p, 'lmstudio', status, why, size)

    def history(self):
        for rel in HISTORY:
            p = os.path.join(self.home, rel)
            if os.path.isdir(p) and not os.path.islink(p):
                self.add(p, 'history', 'protected', 'conversation history: not rebuildable')

    def docker(self):
        extra = (os.path.join(self.home, '.docker/bin/docker'),)
        exe = shutil.which('docker') or next((p for p in DOCKER_PATHS + extra if os.access(p, os.X_OK)), None)
        if not exe:
            return
        rc, out = run([exe, 'system', 'df', '--format', '{{json .}}'], timeout=60)
        if rc != 0:
            self.skip('docker', 'docker system df failed (daemon not running?)')
            return
        for line in out.splitlines():
            try:
                row = json.loads(line)
            except ValueError:
                continue
            kind, size, reclaim = row.get('Type', ''), row.get('Size', ''), row.get('Reclaimable', '')
            path = 'docker:' + kind.lower().replace(' ', '-')
            if kind in ('Images', 'Build Cache'):
                self.items.append({'path': path, 'category': 'docker', 'bytes': docker_bytes(reclaim), 'status': 'cold',
                                   'why': f'{reclaim} reclaimable of {size}, inside the Docker VM disk',
                                   'frees_host': False})
            elif kind == 'Local Volumes':
                self.items.append({'path': path, 'category': 'docker', 'bytes': docker_bytes(size),
                                   'status': 'protected', 'why': 'volumes hold data', 'frees_host': False})

    def run(self):
        t0 = time.monotonic()
        self.editor = editor = self.editor_recents()
        self.walk()
        for step in (self.derived_data, self.simulators, self.app_caches, self.claude_versions,
                     self.ollama, self.huggingface, self.lmstudio, self.history, self.docker):
            step()
        self.items.sort(key=lambda i: (-i['bytes'], i['path']))
        return {'home': self.home, 'recent_days': self.days,
                'generated_at': datetime.fromtimestamp(self.now, timezone.utc).isoformat(timespec='seconds'),
                'editor_recent_paths': len(editor), 'items': self.items, 'skipped': self.skipped,
                'totals': totals(self.items), 'elapsed_secs': round(time.monotonic() - t0, 2)}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument('--home', default=os.path.expanduser('~'))
    ap.add_argument('--out', required=True)
    ap.add_argument('--recent-days', type=int, default=14)
    a = ap.parse_args(argv)
    result = Census(a.home, a.recent_days).run()
    with open(a.out, 'w') as f:
        json.dump(result, f, indent=1)
    cold = result['totals']['by_status'].get('cold', {'count': 0, 'bytes': 0})
    print(f"{len(result['items'])} items, {cold['count']} cold ({cold['bytes'] / 1e9:.2f} GB), "
          f"{result['elapsed_secs']} s -> {a.out}")


if __name__ == '__main__':
    main()
