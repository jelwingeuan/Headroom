# Contributing

## Build and test

```bash
cd core && cargo test
```

```bash
python3 -I -m unittest discover -s judge
```

```bash
scripts/bundle.sh
```

## The judge is frozen

`judge/` scores Headroom from the outside. It's a black box that only uses the CLI contract in
`judge/CONTRACT.md` and the volume's free space. To keep it honest:

- A pull request may change the judge (`judge/`, `.github/workflows/judge.yml`) **or**
  `core/`/`app/`, never both. CI enforces this.
- Changing the judge needs its own PR that explains why the old check was wrong.
- Report the judge's real numbers in every PR, including misses.

## Rules

- Commits use [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, …).
- No runtime dependencies without a strong reason.
- Every rule that lets Headroom delete something needs a test that shows what it must *not* delete.
- GitHub Actions are pinned to full commit SHAs. There's no Dependabot, so update pins by hand.
