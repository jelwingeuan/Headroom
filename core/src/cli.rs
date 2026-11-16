//! The command line (judge/CONTRACT.md). stdout carries only the JSON described there; logs
//! go to stderr. Exit 0 is success, 1 a failure, 2 a usage error.

use crate::json::Json;
use crate::state::{self, Ctx};
use crate::{agent, autopilot, mcp, rebuild, reclaim, scan, shrink};
use std::io::Write;
use std::path::Path;

const HELP: &str = "Headroom keeps your Mac's free space at or above a budget, deleting only what can be rebuilt.

  headroom scan <folder> --json             what could go, and why (changes nothing)
  headroom clean <folder> --apply --json    delete it for real and record how to rebuild it
  headroom rebuild --all                    run every recorded rebuild
  headroom undo --all | undo <id>           undo deletions (for a rebuildable folder: rebuild)
  headroom autopilot --once [--now <unix>] [--budget-bytes <n>] [--recent-days <d>]
  headroom agent --idle                     background agent, until SIGTERM
  headroom mcp                              MCP server for coding agents (stdio)
  headroom archive <project> --to <volume>  move a project to another drive, verified
  headroom restore <archive-folder>         put an archived project back

State lives in $HEADROOM_HOME (default ~/Library/Application Support/Headroom).";

fn out(j: &Json) {
    let mut o = std::io::stdout().lock();
    let _ = writeln!(o, "{j}");
    let _ = o.flush();
}

fn flag(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn value<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).map(String::as_str)
}

fn positional(args: &[String]) -> Option<&str> {
    let mut skip = false;
    for a in args.iter().skip(1) {
        if skip {
            skip = false;
        } else if matches!(a.as_str(), "--now" | "--budget-bytes" | "--recent-days" | "--to") {
            skip = true;
        } else if !a.starts_with("--") {
            return Some(a);
        }
    }
    None
}

fn usage() -> i32 {
    eprintln!("{HELP}");
    2
}

fn fail(e: impl std::fmt::Display) -> i32 {
    eprintln!("headroom: {e}");
    1
}

/// argv[0] included, as the C ABI passes it.
pub fn main(argv: Vec<String>) -> i32 {
    let args = &argv[1.min(argv.len())..];
    let Some(cmd) = args.first().map(String::as_str) else { return usage() };
    let mut ctx = Ctx::from_env();
    match cmd {
        "scan" => {
            let Some(root) = positional(args) else { return usage() };
            match scan::scan(&ctx, Path::new(root)) {
                Ok(s) => {
                    out(&Json::Arr(s.cands.iter().map(scan::Cand::to_json).collect()));
                    0
                }
                Err(e) => fail(e),
            }
        }
        "clean" => {
            let Some(root) = positional(args) else { return usage() };
            match reclaim::clean(&ctx, Path::new(root), flag(args, "--apply"), "clean") {
                Ok(j) => {
                    out(&j);
                    0
                }
                Err(e) => fail(e),
            }
        }
        "rebuild" | "undo" => {
            let id = positional(args);
            if !flag(args, "--all") && id.is_none() {
                return usage();
            }
            let (j, ok) = rebuild::undo_all(&ctx, if flag(args, "--all") { None } else { id });
            out(&j);
            i32::from(!ok)
        }
        "autopilot" => {
            if !flag(args, "--once") {
                return usage();
            }
            let num =
                |n: &str| value(args, n).map(|v| v.parse::<u64>().map_err(|_| format!("{n} needs a whole number")));
            let settings = state::settings(&ctx);
            let (now, budget, recent) = match (num("--now"), num("--budget-bytes"), num("--recent-days")) {
                (Some(Err(e)), _, _) | (_, Some(Err(e)), _) | (_, _, Some(Err(e))) => return fail(e),
                (n, b, r) => (n.map(Result::unwrap), b.map(Result::unwrap), r.map(Result::unwrap)),
            };
            ctx.now = now.unwrap_or(ctx.now);
            ctx.recent_days = recent.unwrap_or(settings.recent_days);
            let ok = autopilot::tick(&ctx, budget.unwrap_or(settings.budget_bytes), &mut |e| out(&e));
            i32::from(!ok)
        }
        "agent" if flag(args, "--idle") => agent::run(&ctx),
        "mcp" => mcp::serve(&ctx, std::io::stdin().lock(), std::io::stdout()),
        "archive" => {
            let (Some(project), Some(to)) = (positional(args), value(args, "--to")) else { return usage() };
            match shrink::archive(&ctx, Path::new(project), Path::new(to), true) {
                Ok(j) => {
                    out(&j);
                    0
                }
                Err(e) => fail(e),
            }
        }
        "restore" => {
            let Some(a) = positional(args) else { return usage() };
            match shrink::restore(Path::new(a)) {
                Ok(j) => {
                    out(&j);
                    0
                }
                Err(e) => fail(e),
            }
        }
        "help" | "--help" | "-h" => {
            println!("{HELP}");
            0
        }
        _ => usage(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn finds_the_positional_argument_after_flags() {
        assert_eq!(positional(&v(&["scan", "--json", "/x"])), Some("/x"));
        assert_eq!(positional(&v(&["archive", "--to", "/Volumes/d", "/p"])), Some("/p"));
        assert_eq!(positional(&v(&["undo", "--all"])), None);
        assert_eq!(value(&v(&["autopilot", "--now", "5"]), "--now"), Some("5"));
    }

    #[test]
    fn bad_usage_exits_2() {
        assert_eq!(main(v(&["headroom"])), 2);
        assert_eq!(main(v(&["headroom", "scan"])), 2);
        assert_eq!(main(v(&["headroom", "autopilot"])), 2);
        assert_eq!(main(v(&["headroom", "frobnicate"])), 2);
    }
}
