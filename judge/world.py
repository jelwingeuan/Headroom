#!/usr/bin/env python3
"""Builds the judge's ground truth on a fresh CI runner and writes judge/out/truth.json.

Real projects (Node, Next.js, Rust, Python venvs, Swift packages built with xcodebuild), the
traps Headroom must not fall into, and the fixtures it reads (git dates, VS Code's open
windows, Ollama and Hugging Face models, a recording docker shim). It installs software,
starts a dev server and writes into ~/.claude, ~/.codex and VS Code's settings, so it refuses
to run outside GitHub Actions unless --force-local is passed.

  python3 -I judge/world.py [--root ~/judge-world] [--tauri]
"""
import argparse
import binascii
import json
import os
import shutil
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time
import urllib.parse
import zlib

HOME = os.path.expanduser('~')
OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), 'out')
MB, GB, DAY = 1 << 20, 1 << 30, 86400
RECENT_DAYS = 14
DEV_PORT = 5173
DERIVED = os.path.join(HOME, 'Library/Developer/Xcode/DerivedData')
VSCODE_STORAGE = os.path.join(HOME, 'Library/Application Support/Code/User/globalStorage/storage.json')
DOCKER_DESKTOP = os.path.join(HOME, 'Library/Containers/com.docker.docker')
OLLAMA_MODELS = os.path.join(HOME, '.ollama/models')
HF_REPO = 'hf-internal-testing/tiny-random-bert'
HF_CACHE = os.path.join(HOME, '.cache/huggingface/hub')
REMOTES = os.path.join(HOME, 'judge-remotes')
# Quiet, non-interactive package managers, for setup and for the judge's own checks.
ENV = {
    'npm_config_audit': 'false', 'npm_config_fund': 'false', 'npm_config_update_notifier': 'false',
    'NEXT_TELEMETRY_DISABLED': '1', 'PIP_DISABLE_PIP_VERSION_CHECK': '1', 'HF_HUB_DISABLE_TELEMETRY': '1',
    'HOMEBREW_NO_AUTO_UPDATE': '1', 'HOMEBREW_NO_INSTALL_CLEANUP': '1', 'HOMEBREW_NO_ANALYTICS': '1',
}

# The full-scale mix; counts() scales it down to the runner's free disk.
NODE = [{'express': '^4'}, {'lodash': '^4', 'dayjs': '^1'}, {'typescript': '^5'}, {'commander': '^12', 'chalk': '^5'},
        {'zod': '^3', 'typescript': '^5'}, {'axios': '^1'}, {'date-fns': '^3'}, {'eslint': '^9'},
        {'prettier': '^3'}, {'webpack': '^5'}]
RUST = ['serde = { version = "1", features = ["derive"] }\nserde_json = "1"', 'regex = "1"', 'rand = "0.8"',
        'anyhow = "1"\nthiserror = "1"', 'clap = { version = "4", features = ["derive"] }',
        'tokio = { version = "1", features = ["rt", "macros"] }']
PYTHON = [['requests'], ['rich'], ['click', 'pyyaml'], ['attrs', 'packaging'], ['httpx']]
SWIFT = ['SwiftAlpha', 'SwiftBravo', 'SwiftCharlie', 'SwiftDelta']
FULL = {'node': len(NODE), 'next': 3, 'rust': len(RUST), 'python': len(PYTHON), 'swift': len(SWIFT)}
TRAPS = 5
EST_MB = {'node': 40, 'next': 450, 'rust': 250, 'python': 45, 'swift': 150}
FIXED_MB = 800  # traps, models, package-manager caches


def free_bytes(path=HOME):
    st = os.statvfs(path)
    return st.f_bavail * st.f_frsize


def fit_ratio(free):
    # Later the disk holds about twice the world (package caches, rebuilds, autopilot growth)
    # plus ~3 GB (the 1M-file speed tree, Headroom's own build).
    full = (sum(FULL[k] * EST_MB[k] for k in FULL) + FIXED_MB) * MB
    return max(0.0, min(1.0, (free - 3 * GB) / 2 / full))


def counts(ratio):
    return {k: max(1, int(n * ratio + 1e-9)) for k, n in FULL.items()}


def plan(c):
    """Round-robin over kinds, so recent/warm/cold land on every kind."""
    queues = {k: list(range(c[k])) for k in ('node', 'rust', 'python', 'swift', 'next')}
    out = []
    while any(queues.values()):
        for kind, q in queues.items():
            if q:
                out.append((kind, q.pop(0)))
    return out


def activity(i):
    """(label, days since last commit): recent = opened 2 days ago, warm 20-29, cold 60-200."""
    if i % 4 == 1:
        return 'recent', 2
    if i % 4 == 3:
        return 'warm', 20 + i % 10
    return 'cold', 60 + (i * 37) % 141


def alloc_bytes(*paths):
    """Allocated bytes under all paths together (st_blocks), each hard link counted once."""
    seen, total, stack = set(), 0, list(paths)
    while stack:
        p = stack.pop()
        try:
            st = os.lstat(p)
        except OSError:
            continue
        if (st.st_dev, st.st_ino) in seen:
            continue
        seen.add((st.st_dev, st.st_ino))
        total += st.st_blocks * 512
        if stat.S_ISDIR(st.st_mode):
            try:
                stack.extend(os.path.join(p, n) for n in os.listdir(p))
            except OSError:
                pass
    return total


def backdate_tree(path, when):
    """Sets every mtime under path to when. Git's own files keep theirs (see refresh_git)."""
    if '/.git/' in path + '/':
        return
    for d, dirs, files in os.walk(path):
        dirs[:] = [n for n in dirs if n != '.git']
        for n in dirs + files:
            try:
                os.utime(os.path.join(d, n), (when, when), follow_symlinks=False)
            except OSError:
                pass
    if os.path.lexists(path):
        os.utime(path, (when, when), follow_symlinks=False)


def backdate(truth):
    """Gives cold projects, their build output and old models cold mtimes."""
    for path, when in truth['backdate']:
        backdate_tree(path, when)


def refresh_git(truth):
    # Backdating changed file times under git's feet. Refresh each index now, so a later
    # `git status` has nothing to write and .git/index (which the manifest hashes) stays put.
    for p in truth['projects']:
        wt = os.path.join(p['path'], '.claude', 'worktrees')
        tops = [p['path']] + ([os.path.join(wt, n) for n in sorted(os.listdir(wt))] if os.path.isdir(wt) else [])
        for d in tops:
            if os.path.exists(os.path.join(d, '.git')):
                subprocess.run(['git', '-C', d, 'update-index', '-q', '--refresh'], capture_output=True)


def inside(path, top):
    top = top.rstrip('/')
    return path == top or path.startswith(top + '/')


def write(path, name, text):
    p = os.path.join(path, name)
    os.makedirs(os.path.dirname(p), exist_ok=True)
    with open(p, 'w') as f:
        f.write(text)
    return p


def sh(cmd, cwd, tries=1, timeout=1800):
    """Runs one setup command; returns {cmd, cwd, secs} or raises with the tail of its output."""
    for _ in range(tries):
        t = time.monotonic()
        r = subprocess.run(cmd, shell=True, cwd=cwd, capture_output=True, text=True, timeout=timeout)
        if r.returncode == 0:
            return {'cmd': cmd, 'cwd': cwd, 'secs': round(time.monotonic() - t, 1)}
    raise RuntimeError(f'`{cmd}` exited {r.returncode}: {(r.stdout + r.stderr)[-1500:]}')


def git(cwd, *args, when=None):
    env = dict(os.environ, GIT_AUTHOR_NAME='Judge', GIT_AUTHOR_EMAIL='judge@example.invalid',
               GIT_COMMITTER_NAME='Judge', GIT_COMMITTER_EMAIL='judge@example.invalid')
    if when:
        env['GIT_AUTHOR_DATE'] = env['GIT_COMMITTER_DATE'] = f'@{int(when)} +0000'
    r = subprocess.run(['git', *args], cwd=cwd, env=env, capture_output=True, text=True)
    if r.returncode:
        raise RuntimeError(f'git {" ".join(args)}: {r.stderr.strip()}')


def commit(path, when, msg='Initial commit'):
    if not os.path.isdir(os.path.join(path, '.git')):
        git(path, 'init', '-q', '-b', 'main')
    git(path, 'add', '-A')
    git(path, 'commit', '-q', '-m', msg, when=when)


def package_json(path, deps, **extra):
    pkg = dict({'name': os.path.basename(path), 'version': '1.0.0', 'private': True}, **extra, dependencies=deps)
    write(path, 'package.json', json.dumps(pkg, indent=2) + '\n')


# Project builders: write files, install, commit; return what the judge must know about them.
# rebuild = dirs that may be deleted (with the command that brings them back); protected =
# dirs that look reclaimable but must survive.

def node(path, when, deps, edited):
    package_json(path, deps)
    write(path, 'index.js', "console.log('hello');\n")
    write(path, '.gitignore', 'node_modules/\n')
    steps = [sh('npm install', path, tries=2)]
    commit(path, when)
    nm = os.path.join(path, 'node_modules')
    rec = {'steps': steps, 'verify': 'npm ls --depth=0', 'rebuild': [{'path': nm, 'cmd': 'npm ci', 'cwd': path}]}
    if edited:
        # Trap: a hand-made fix inside node_modules. A rebuild would silently drop it.
        f = os.path.join(nm, next(iter(deps)), 'index.js')
        with open(f, 'a') as fh:
            fh.write('\n// patched by hand, keep: judge fixture\n')
        rec['keep'] = [f]
        rec['rebuild'][0]['eligible'] = False
    return rec


def nextjs(path, when):
    package_json(path, {'next': '^15', 'react': '^19', 'react-dom': '^19'}, scripts={'build': 'next build'})
    write(path, 'app/layout.js', 'export default function RootLayout({ children }) {\n'
                                 '  return <html lang="en"><body>{children}</body></html>\n}\n')
    write(path, 'app/page.js', 'export default function Page() {\n  return <h1>Hello</h1>\n}\n')
    write(path, '.gitignore', 'node_modules/\n.next/\n')
    steps = [sh('npm install', path, tries=2), sh('npm run build', path)]
    commit(path, when)
    return {'steps': steps, 'verify': 'npm ls --depth=0 && npm run build',
            'rebuild': [{'path': os.path.join(path, 'node_modules'), 'cmd': 'npm ci', 'cwd': path},
                        {'path': os.path.join(path, '.next'), 'cmd': 'npm run build', 'cwd': path}]}


def rust(path, when, deps):
    write(path, 'Cargo.toml', f'[package]\nname = "{os.path.basename(path)}"\nversion = "0.1.0"\n'
                              f'edition = "2021"\n\n[dependencies]\n{deps}\n')
    write(path, 'src/main.rs', 'fn main() {\n    println!("hello");\n}\n')
    write(path, '.gitignore', 'target/\n')
    steps = [sh('cargo build', path, tries=2)]
    commit(path, when)
    return {'steps': steps, 'verify': 'cargo build',
            'rebuild': [{'path': os.path.join(path, 'target'), 'cmd': 'cargo build', 'cwd': path}]}


def python(path, when, pkgs, pyproject):
    mods = ', '.join({'pyyaml': 'yaml'}.get(p, p) for p in pkgs)
    write(path, 'main.py', f'import {mods}\n')
    write(path, '.gitignore', '.venv/\n')
    if pyproject:
        write(path, 'pyproject.toml', f'[project]\nname = "{os.path.basename(path)}"\nversion = "0.1.0"\n'
                                      f'dependencies = {json.dumps(pkgs)}\n')
        cmd = 'python3 -m venv .venv && .venv/bin/pip install -q ' + ' '.join(pkgs)
    else:
        write(path, 'requirements.txt', '\n'.join(pkgs) + '\n')
        cmd = 'python3 -m venv .venv && .venv/bin/pip install -q -r requirements.txt'
    steps = [sh(cmd, path, tries=2)]
    commit(path, when)
    return {'steps': steps, 'verify': f'.venv/bin/python -B -c "import {mods}"',
            'rebuild': [{'path': os.path.join(path, '.venv'), 'cmd': cmd, 'cwd': path}]}


def swift(path, when, name):
    write(path, 'Package.swift', f'// swift-tools-version:5.9\nimport PackageDescription\n\nlet package = Package(\n'
                                 f'    name: "{name}",\n    platforms: [.macOS(.v13)],\n'
                                 f'    targets: [.executableTarget(name: "{name}")]\n)\n')
    write(path, f'Sources/{name}/main.swift', 'print("hello")\n')
    write(path, '.gitignore', '.build/\n')
    cmd = f"xcodebuild -scheme {name} -destination 'platform=macOS' -quiet build"
    before = set(os.listdir(DERIVED)) if os.path.isdir(DERIVED) else set()
    steps = [sh(cmd, path)]
    commit(path, when)
    new = sorted(set(os.listdir(DERIVED)) - before)
    if not any(n.startswith(name + '-') for n in new):
        raise RuntimeError('xcodebuild left no DerivedData folder')
    # Other new entries (ModuleCache.noindex, ...) are shared by every Xcode build.
    rebuild = [{'path': os.path.join(DERIVED, n), 'cmd': cmd, 'cwd': path, 'shared': not n.startswith(name + '-')}
               for n in new]
    if os.path.isdir(os.path.join(path, '.build')):  # in case xcodebuild also resolves into .build
        rebuild.append({'path': os.path.join(path, '.build'), 'cmd': cmd, 'cwd': path})
    return {'steps': steps, 'verify': cmd, 'rebuild': rebuild}


def untracked_build(path, when):
    write(path, 'index.html', '<h1>site</h1>\n')
    write(path, '.gitignore', 'node_modules/\n')  # build/ and dist/ deliberately not ignored
    commit(path, when)
    write(path, 'build/notes.md', 'Hand-written release notes. Nothing generates this file.\n')
    write(path, 'dist/report.html', '<p>Exported by hand from a tool that no longer exists.</p>\n')
    why = 'untracked files that git does not ignore'
    return {'protected': [{'path': os.path.join(path, d), 'why': why} for d in ('build', 'dist')]}


def tracked_build(path, when):
    write(path, 'README.md', '# Vendored build output\n')
    write(path, 'build/output.js', 'module.exports = 42;\n')
    write(path, '.gitignore', 'node_modules/\n')
    commit(path, when)
    return {'protected': [{'path': os.path.join(path, 'build'), 'why': 'tracked by git'}]}


def live_server(path, when):
    package_json(path, {'express': '^4'})
    write(path, 'server.js', "const app = require('express')();\napp.get('/', (req, res) => res.send('ok'));\n"
                             f"app.listen({DEV_PORT}, '127.0.0.1');\n")
    write(path, '.gitignore', 'node_modules/\n')
    steps = [sh('npm install', path, tries=2)]
    commit(path, when)
    return {'steps': steps, 'protected': [{'path': os.path.join(path, 'node_modules'),
                                           'why': 'a running dev server has its cwd in this project'}]}


def agent_repo(path, when):
    def wt(n):
        return os.path.join(path, '.claude', 'worktrees', n)
    write(path, 'README.md', '# Agent repo\n')
    write(path, 'src/app.js', "console.log('app');\n")
    write(path, '.gitignore', '.env\n.claude/worktrees/\n')
    commit(path, when)
    bare = os.path.join(REMOTES, os.path.basename(path) + '.git')
    os.makedirs(REMOTES, exist_ok=True)
    git(REMOTES, 'init', '-q', '--bare', bare)
    git(path, 'remote', 'add', 'origin', bare)
    git(path, 'push', '-q', 'origin', 'main')
    # merged + clean: its work is on main and main is pushed; the only reclaimable worktree.
    git(path, 'worktree', 'add', '-b', 'agent/merged', wt('merged'))
    write(wt('merged'), 'src/feature.js', "console.log('feature');\n")
    git(wt('merged'), 'add', '-A')
    git(wt('merged'), 'commit', '-q', '-m', 'Add feature', when=when + 3600)
    git(path, 'merge', '-q', '--ff-only', 'agent/merged')
    git(path, 'push', '-q', 'origin', 'main')
    # unpushed: a commit that exists nowhere else.
    git(path, 'worktree', 'add', '-b', 'agent/unpushed', wt('unpushed'))
    write(wt('unpushed'), 'src/wip.js', "console.log('work in progress');\n")
    git(wt('unpushed'), 'add', '-A')
    git(wt('unpushed'), 'commit', '-q', '-m', 'WIP', when=when + 7200)
    # clean and merged, but holds an ignored .env that nothing can rebuild.
    git(path, 'worktree', 'add', '-b', 'agent/env', wt('env'))
    write(wt('env'), '.env', 'API_URL=http://localhost:8080\n')
    add = 'git worktree add .claude/worktrees/merged agent/merged'
    return {'verify': 'git -C .claude/worktrees/merged rev-parse --verify HEAD',
            'rebuild': [{'path': wt('merged'), 'cmd': add, 'cwd': path},
                        {'path': os.path.join(path, '.git', 'worktrees', 'merged'), 'cmd': add, 'cwd': path}],
            'protected': [{'path': wt('unpushed'), 'why': 'worktree with unpushed commits'},
                          {'path': wt('env'), 'why': 'worktree with an ignored .env'}]}


def nomanifest_venv(path, when):
    write(path, 'main.py', 'import six\n')
    write(path, '.gitignore', 'env/\n')
    steps = [sh('python3 -m venv env && env/bin/pip install -q six', path, tries=2)]
    commit(path, when)
    return {'steps': steps, 'protected': [{'path': os.path.join(path, 'env'),
                                           'why': 'venv without a manifest: nothing records what was installed'}]}


def png(size, rgba):
    """A solid-colour RGBA PNG: Tauri will not compile without an app icon."""
    def chunk(kind, data):
        return struct.pack('>I', len(data)) + kind + data + struct.pack('>I', binascii.crc32(kind + data))
    rows = b''.join(b'\0' + bytes(rgba) * size for _ in range(size))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', size, size, 8, 6, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(rows)) + chunk(b'IEND', b''))


def tauri(path, when):
    """A minimal hand-written Tauri 2 app (no template download, so nothing drifts)."""
    src = os.path.join(path, 'src-tauri')
    write(path, 'ui/index.html', '<!doctype html>\n<title>tauri-app</title>\n<h1>Hello</h1>\n')
    write(src, 'Cargo.toml', '[package]\nname = "tauri-app"\nversion = "0.1.0"\nedition = "2021"\n\n'
                             '[build-dependencies]\ntauri-build = { version = "2", features = [] }\n\n'
                             '[dependencies]\ntauri = { version = "2", features = [] }\n')
    write(src, 'build.rs', 'fn main() {\n    tauri_build::build()\n}\n')
    write(src, 'src/main.rs', 'fn main() {\n    tauri::Builder::default()\n        .run(tauri::generate_context!())\n'
                              '        .expect("error while running the app");\n}\n')
    conf = {'productName': 'tauri-app', 'version': '0.1.0', 'identifier': 'com.example.judgetauri',
            'build': {'frontendDist': '../ui'}, 'app': {'windows': [{'title': 'tauri-app'}]},
            'bundle': {'active': False, 'icon': ['icons/icon.png']}}
    write(src, 'tauri.conf.json', json.dumps(conf, indent=2) + '\n')
    os.makedirs(os.path.join(src, 'icons'))
    with open(os.path.join(src, 'icons', 'icon.png'), 'wb') as f:
        f.write(png(32, (40, 120, 200, 255)))
    write(path, '.gitignore', 'src-tauri/target/\nsrc-tauri/gen/\n')
    steps = [sh('cargo build', src, tries=2, timeout=3600)]
    commit(path, when)
    rebuild = [{'path': os.path.join(src, d), 'cmd': 'cargo build', 'cwd': src} for d in ('target', 'gen')
               if os.path.isdir(os.path.join(src, d))]
    return {'steps': steps, 'verify': 'cd src-tauri && cargo build', 'rebuild': rebuild}


def add_project(truth, name, kind, act, days, fn, *args):
    path = os.path.join(truth['root'], name)
    when = truth['created'] - days * DAY
    os.makedirs(path)
    try:
        rec = fn(path, when, *args)
    except Exception as e:  # one broken install must not sink the world; it is reported instead
        truth['errors'].append(f'{name}: {e}')
        shutil.rmtree(path, ignore_errors=True)
        print(f'  {name}: FAILED {e}', flush=True)
        return None
    recent = act == 'recent'
    p = {'name': name, 'kind': kind, 'path': path, 'activity': act, 'last_commit': when,
         'recently_opened': recent, 'steps': rec.get('steps', []), 'verify_cmd': None,
         'protected': rec.get('protected', [])}
    for r in rec.get('rebuild', []):
        mine = not r.get('shared')
        if recent and mine:
            p['protected'].append({'path': r['path'], 'why': 'recently opened project'})
            if not inside(r['path'], path):
                truth['manifest_roots'].append(r['path'])
                truth['recent_paths'].append(r['path'])
            continue
        truth['rebuildable'].append({'path': r['path'], 'cmd': r['cmd'], 'cwd': r['cwd'],
                                     'project': name if mine else None, 'mtime': when if mine else None,
                                     'eligible': r.get('eligible', True), 'bytes': 0})
        if mine:
            p['verify_cmd'] = rec.get('verify')
            if not inside(r['path'], path):
                truth['backdate'].append([r['path'], when])
    if recent:
        truth['recent_paths'].append(path)
    else:
        truth['backdate'].append([path, when])
    for f in rec.get('keep', []):
        truth['keep_files'].append(f)
        truth['backdate'].append([f, when + DAY])  # edited a day after the install
    truth['projects'].append(p)
    print(f'  {name}: {act}, {sum(s["secs"] for s in p["steps"]):.0f}s', flush=True)
    return p


def wait_for(check, seconds):
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        if check():
            return True
        time.sleep(0.5)
    return False


def port_open(port):
    try:
        with socket.create_connection(('127.0.0.1', port), 1):
            return True
    except OSError:
        return False


def ollama_files(name):
    """The manifest of an Ollama model plus every blob it references."""
    m = os.path.join(OLLAMA_MODELS, 'manifests', 'registry.ollama.ai', 'library', name, 'latest')
    with open(m) as f:
        data = json.load(f)
    digests = [data['config']['digest']] + [layer['digest'] for layer in data['layers']]
    return [m] + [os.path.join(OLLAMA_MODELS, 'blobs', d.replace(':', '-')) for d in digests]


def ollama(truth):
    """One pulled registry model (may go) and one `ollama create` model on top of it (must survive)."""
    try:
        if not shutil.which('ollama'):
            sh('brew install ollama', HOME, timeout=1500)
        exe = shutil.which('ollama') or '/opt/homebrew/bin/ollama'
        log = open(os.path.join(OUT, 'ollama.log'), 'ab')
        srv = subprocess.Popen([exe, 'serve'], stdin=subprocess.DEVNULL, stdout=log, stderr=log, start_new_session=True)
        try:
            if not wait_for(lambda: subprocess.run([exe, 'list'], capture_output=True).returncode == 0, 60):
                raise RuntimeError('ollama serve did not come up')
            sh(f'{exe} pull all-minilm', HOME, tries=2, timeout=900)
            tmp = tempfile.mkdtemp(prefix='judge-modelfile-')
            write(tmp, 'Modelfile', 'FROM all-minilm\nPARAMETER num_ctx 256\n')
            sh(f'{exe} create judge-custom -f Modelfile', tmp, timeout=900)
        finally:
            srv.terminate()
            try:
                srv.wait(30)
            except subprocess.TimeoutExpired:
                srv.kill()
        custom = ollama_files('judge-custom')
        registry = ollama_files('all-minilm')
        # The custom model shares the registry model's weights blob, so that blob must stay.
        reclaim = [registry[0]] + [b for b in registry[1:] if b not in custom]
        old = truth['created'] - 90 * DAY
        for path in reclaim:
            truth['rebuildable'].append({'path': path, 'cmd': 'ollama pull all-minilm', 'cwd': HOME,
                                         'project': None, 'mtime': old, 'eligible': True, 'bytes': 0})
            truth['backdate'].append([path, old])
        truth['manifest_roots'].append(OLLAMA_MODELS)
        truth['models']['ollama'] = {'registry': 'all-minilm', 'custom': 'judge-custom',
                                     'custom_files': custom, 'reclaimable_files': reclaim}
    except Exception as e:
        truth['models']['ollama'] = f'not measured: {e}'


def huggingface(truth):
    venv = tempfile.mkdtemp(prefix='judge-hf-')
    try:
        sh(f'python3 -m venv "{venv}" && "{venv}/bin/pip" install -q huggingface_hub', HOME, tries=2)
        sh(f'"{venv}/bin/python" -c "from huggingface_hub import snapshot_download as s; s(\'{HF_REPO}\')"',
           HOME, tries=2)
        path = os.path.join(HF_CACHE, 'models--' + HF_REPO.replace('/', '--'))
        if not os.path.isdir(path):
            raise RuntimeError(f'no cache folder at {path}')
        old = truth['created'] - 90 * DAY
        truth['rebuildable'].append({'path': path, 'cmd': f'huggingface_hub.snapshot_download("{HF_REPO}")',
                                     'cwd': HOME, 'project': None, 'mtime': old, 'eligible': True, 'bytes': 0})
        truth['backdate'].append([path, old])
        truth['models']['hf'] = {'repo': HF_REPO, 'path': path}
    except Exception as e:
        truth['models']['hf'] = f'not measured: {e}'
    finally:
        shutil.rmtree(venv, ignore_errors=True)


DOCKER_SHIM = r'''#!@PYTHON@ -I
# The judge's recording docker: logs argv, answers read-only commands with canned output.
import json, sys
argv = sys.argv[1:]
with open(@LOG@, 'a') as f:
    f.write(json.dumps(argv) + '\n')
words, skip = [], False
for a in argv:
    if skip:
        skip = False
    elif a in ('--context', '-c', '--host', '-H', '--config', '--log-level', '-l'):
        skip = True
    elif not a.startswith('-'):
        words.append(a)
key = ' '.join(words[:2] if words[:1] in (['image'], ['system'], ['builder'], ['buildx'], ['volume'], ['container']) else words[:1])
images = [{"Repository": "judge/api", "Tag": "latest", "ID": "4f1a2b3c4d5e", "CreatedSince": "5 months ago", "Size": "1.21GB"},
          {"Repository": "<none>", "Tag": "<none>", "ID": "9e8d7c6b5a4f", "CreatedSince": "7 months ago", "Size": "845MB"}]
df = [{"Type": "Images", "TotalCount": "2", "Active": "0", "Size": "2.06GB", "Reclaimable": "2.06GB (100%)"},
      {"Type": "Containers", "TotalCount": "0", "Active": "0", "Size": "0B", "Reclaimable": "0B"},
      {"Type": "Local Volumes", "TotalCount": "1", "Active": "0", "Size": "3.2GB", "Reclaimable": "3.2GB (100%)"},
      {"Type": "Build Cache", "TotalCount": "14", "Active": "0", "Size": "1.5GB", "Reclaimable": "1.5GB"}]
builder = [{"ID": "k3j4h5g6", "Reclaimable": "true", "Size": "1.5GB", "LastUsed": "3 months ago"}]
volumes = [{"Driver": "local", "Name": "judge_pgdata"}]
tables = {'images': images, 'image ls': images, 'image list': images, 'system df': df, 'builder du': builder,
          'buildx du': builder, 'volume ls': volumes, 'volume list': volumes, 'ps': [], 'container ls': [], 'container list': []}
as_json = any('json' in a for a in argv)
if key in tables:
    rows = tables[key]
    if as_json:
        for r in rows:
            print(json.dumps(r))
    elif rows:
        print('\t'.join(rows[0]))
        for r in rows:
            print('\t'.join(r.values()))
elif key == 'version' or '--version' in argv:
    print(json.dumps({"Client": {"Version": "27.3.1"}, "Server": {"Version": "27.3.1"}}) if as_json else 'Docker version 27.3.1, build judge')
elif key == 'info':
    print(json.dumps({"ServerVersion": "27.3.1", "DockerRootDir": "/var/lib/docker"}) if as_json else 'Server Version: 27.3.1')
# Anything else (rm, prune, ...) "succeeds" silently; the judge reads the log afterwards.
'''


def write_docker_shim(bin_dir, log):
    os.makedirs(bin_dir, exist_ok=True)
    path = os.path.join(bin_dir, 'docker')
    with open(path, 'w') as f:
        f.write(DOCKER_SHIM.replace('@PYTHON@', sys.executable).replace('@LOG@', repr(log)))
    os.chmod(path, 0o755)
    return path


def docker(truth):
    """arm64 runners have no Docker: a recording shim on PATH, and a Docker Desktop VM disk to leave alone."""
    bin_dir, log = os.path.join(OUT, 'bin'), os.path.join(OUT, 'docker.log')
    write_docker_shim(bin_dir, log)
    truth['docker'] = {'bin_dir': bin_dir, 'log': log, 'vm_disk': None}
    raw = os.path.join(DOCKER_DESKTOP, 'Data/vms/0/data/Docker.raw')
    try:
        os.makedirs(os.path.dirname(raw), exist_ok=True)
        with open(raw, 'wb') as f:
            f.truncate(64 * MB)  # sparse: 64 MB long, almost nothing on disk
        truth['manifest_roots'].append(DOCKER_DESKTOP)
        truth['docker']['vm_disk'] = raw
    except OSError as e:
        truth['errors'].append(f'docker VM disk fixture: {e}')


def history(truth):
    """Agent chat history: never reclaimable, whatever its size."""
    lines = {os.path.join(HOME, '.claude/projects/x/a.jsonl'): {'type': 'user', 'message': {'role': 'user', 'content': 'judge fixture, keep'}},
             os.path.join(HOME, '.codex/sessions/a.jsonl'): {'type': 'message', 'role': 'user', 'content': 'judge fixture, keep'}}
    for path, line in lines.items():
        if not os.path.exists(path):
            write(os.path.dirname(path), os.path.basename(path), (json.dumps(line) + '\n') * 3)
    truth['manifest_roots'] += [os.path.join(HOME, '.claude/projects'), os.path.join(HOME, '.codex/sessions')]
    truth['history'] = sorted(lines)


def vscode(truth):
    """VS Code's record of its open windows: the 'recently opened' signal for recent projects."""
    recent = [p['path'] for p in truth['projects'] if p['recently_opened']]

    def uri(p):
        return 'file://' + urllib.parse.quote(p)
    windows = {'openedWindows': [{'folder': uri(p)} for p in recent[1:]]}
    if recent:
        windows['lastActiveWindow'] = {'folder': uri(recent[0]),
                                       'uiState': {'mode': 1, 'x': 0, 'y': 25, 'width': 1440, 'height': 875}}
    data = {'windowsState': windows,
            'backupWorkspaces': {'workspaces': [], 'folders': [{'folderUri': uri(p)} for p in recent], 'emptyWindows': []},
            'profileAssociations': {'workspaces': {uri(p): '__default__profile__' for p in recent}, 'emptyWindows': {}}}
    write(os.path.dirname(VSCODE_STORAGE), os.path.basename(VSCODE_STORAGE), json.dumps(data, indent=2) + '\n')
    t = truth['created'] - 2 * DAY
    os.utime(VSCODE_STORAGE, (t, t))
    truth['vscode_storage'] = VSCODE_STORAGE
    truth['manifest_roots'].append(VSCODE_STORAGE)


def dev_server(truth, path):
    log = open(os.path.join(OUT, 'dev-server.log'), 'ab')
    # Own session so it outlives this script; later CI steps check it is still alive.
    proc = subprocess.Popen(['node', 'server.js'], cwd=path, stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                            start_new_session=True)
    truth['dev_server'] = {'pid': proc.pid, 'cwd': path, 'port': DEV_PORT, 'up': wait_for(lambda: port_open(DEV_PORT), 20)}


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument('--root', default=os.path.join(HOME, 'judge-world'))
    p.add_argument('--tauri', action='store_true', help='add one small Tauri app: ~3 GB, ~10 min more')
    p.add_argument('--force-local', action='store_true', help='run outside GitHub Actions anyway')
    a = p.parse_args(argv)
    if os.environ.get('GITHUB_ACTIONS') != 'true' and not a.force_local:
        sys.exit('world.py installs software and rewrites ~/.claude, ~/.codex and VS Code settings: CI only')
    root = os.path.abspath(os.path.expanduser(a.root))
    if os.path.exists(root):
        sys.exit(f'{root} exists; the world must start empty')
    os.environ.update(ENV)
    os.makedirs(OUT, exist_ok=True)

    free = free_bytes()
    ratio = fit_ratio(free)
    c = counts(ratio)
    truth = {'version': 1, 'created': int(time.time()), 'root': root, 'recent_days': RECENT_DAYS,
             'scale': {'ratio': round(ratio, 3), 'counts': c, 'full_counts': FULL, 'free_bytes_at_start': free,
                       'full_projects': sum(FULL.values()) + TRAPS + int(a.tauri)},
             'projects': [], 'rebuildable': [], 'keep_files': [], 'manifest_roots': [root, REMOTES],
             'backdate': [], 'recent_paths': [], 'models': {}, 'docker': None, 'dev_server': None,
             'vscode_storage': None, 'history': [], 'errors': []}
    os.makedirs(root)
    print(f'free {free / GB:.1f} GB -> scale {ratio:.2f}: {c}', flush=True)

    for i, (kind, k) in enumerate(plan(c)):
        act, days = activity(i)
        name = f'{kind}-{k + 1:02d}'
        if kind == 'node':
            add_project(truth, name, kind, act, days, node, NODE[k], k == 0)
        elif kind == 'next':
            add_project(truth, name, kind, act, days, nextjs)
        elif kind == 'rust':
            add_project(truth, name, kind, act, days, rust, RUST[k])
        elif kind == 'python':
            add_project(truth, name, kind, act, days, python, PYTHON[k], k == 1)
        else:
            add_project(truth, name, kind, act, days, swift, SWIFT[k])
    add_project(truth, 'web-untracked', 'trap', 'cold', 120, untracked_build)
    add_project(truth, 'tracked-build', 'trap', 'cold', 150, tracked_build)
    live = add_project(truth, 'live-server', 'trap', 'cold', 90, live_server)
    add_project(truth, 'agent-repo', 'trap', 'cold', 75, agent_repo)
    add_project(truth, 'py-nomanifest', 'trap', 'cold', 180, nomanifest_venv)
    if a.tauri:
        add_project(truth, 'tauri-app', 'tauri', 'cold', 100, tauri)
    ollama(truth)
    huggingface(truth)
    docker(truth)
    history(truth)

    for e in truth['rebuildable']:
        e['bytes'] = alloc_bytes(e['path'])
    backdate(truth)
    refresh_git(truth)
    vscode(truth)
    if live:
        dev_server(truth, live['path'])
    truth['scale']['projects'] = len(truth['projects'])
    with open(os.path.join(OUT, 'truth.json'), 'w') as f:
        json.dump(truth, f, indent=2)
    total = sum(e['bytes'] for e in truth['rebuildable'])
    print(f'{len(truth["projects"])} projects, {total / GB:.2f} GB rebuildable, '
          f'{len(truth["errors"])} errors, models: {truth["models"]}, dev server: {truth["dev_server"]}')
    for err in truth['errors']:
        print(f'error: {err}')
    return 0 if truth['projects'] else 1


if __name__ == '__main__':
    sys.exit(main())
