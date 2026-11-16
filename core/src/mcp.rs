//! `headroom mcp`: a Model Context Protocol server (JSON-RPC 2.0, one message per line on
//! stdio) for coding agents. It is the low-trust door (THREAT-MODEL invariant 4): two tools,
//! `scan` and `clean`, both fixed to the project the client started it in; no paths, no
//! settings, no undo, no force. Clean follows every rule, dry run included.

use crate::json::Json;
use crate::state::{self, Ctx};
use crate::{git, obj, reclaim, scan};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL: &str = "2025-06-18";

pub fn serve(ctx: &Ctx, input: impl BufRead, mut output: impl Write) -> i32 {
    let cwd = std::env::current_dir().and_then(|d| d.canonicalize()).unwrap_or_else(|_| PathBuf::from("/"));
    let project = git::toplevel(&cwd).unwrap_or(cwd);
    let mut ctx = ctx.clone();
    ctx.machine = false;
    for line in input.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle(&ctx, &project, &line) {
            if writeln!(output, "{reply}").and_then(|_| output.flush()).is_err() {
                break;
            }
        }
    }
    0
}

fn tool(name: &str, description: &str) -> Json {
    obj! {
        "name" => name, "description" => description,
        "inputSchema" => obj! {"type" => "object", "properties" => obj! {}, "additionalProperties" => false},
    }
}

/// The client's project must be a project: a git top level or a folder with a project file
/// at its top, and never the home folder or above (a dotfiles repo at ~ included).
fn check_project(ctx: &Ctx, p: &Path) -> Result<(), String> {
    if ctx.home.starts_with(p) {
        return Err(format!(
            "{} is the home folder or above it; start Headroom's MCP server in a project",
            p.display()
        ));
    }
    let names: Vec<String> = std::fs::read_dir(p)
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    if names.iter().any(|n| n == ".git") || crate::rules::rules_for(&names).next().is_some() {
        Ok(())
    } else {
        Err(format!("{} is not a project folder (no .git or project file at its top)", p.display()))
    }
}

fn text_result(text: String, is_error: bool) -> Json {
    obj! {"content" => vec![obj! {"type" => "text", "text" => text}], "isError" => is_error}
}

/// One JSON-RPC message in, at most one out (notifications get no reply).
pub fn handle(ctx: &Ctx, project: &Path, line: &str) -> Option<Json> {
    let reply = |id: Json, result: Json| Some(obj! {"jsonrpc" => "2.0", "id" => id, "result" => result});
    let error = |id: Json, code: i64, msg: &str| {
        Some(obj! {"jsonrpc" => "2.0", "id" => id, "error" => obj! {"code" => code, "message" => msg}})
    };
    let Ok(req) = Json::parse(line) else { return error(Json::Null, -32700, "parse error") };
    let id = req.get("id").cloned()?; // a notification: nothing to answer
    let params = req.get("params").cloned().unwrap_or(obj! {});
    match req.s("method") {
        "initialize" => {
            let v = params.s("protocolVersion");
            reply(
                id,
                obj! {
                    "protocolVersion" => if v.is_empty() { PROTOCOL } else { v },
                    "capabilities" => obj! {"tools" => obj! {"listChanged" => false}},
                    "serverInfo" => obj! {"name" => "headroom", "version" => env!("CARGO_PKG_VERSION")},
                    "instructions" => format!("Headroom reclaims rebuildable build output in {} only, under all of its safety rules.", project.display()),
                },
            )
        }
        "ping" => reply(id, obj! {}),
        "tools/list" => reply(
            id,
            obj! {"tools" => vec![
                tool("scan", "List what Headroom could reclaim in this project (dependencies and build output), with sizes and the reason for each. Changes nothing."),
                tool("clean", "Delete what `scan` marks reclaimable in this project and record how to rebuild each folder. Follows every rule: tracked, recent, busy and unprovable folders are kept; during Headroom's dry run nothing is deleted."),
            ]},
        ),
        "tools/call" => {
            if !matches!(params.s("name"), "scan" | "clean") {
                return error(id, -32602, "unknown tool");
            }
            if let Err(e) = check_project(ctx, project) {
                return reply(id, text_result(e, true));
            }
            let result = match params.s("name") {
                "scan" => match scan::scan(ctx, project) {
                    Ok(s) => {
                        text_result(Json::Arr(s.cands.iter().map(scan::Cand::to_json).collect()).to_string(), false)
                    }
                    Err(e) => text_result(e, true),
                },
                "clean" => {
                    let dry = state::dry_run(ctx).0;
                    match reclaim::clean(ctx, project, !dry, "mcp") {
                        Ok(mut j) => {
                            j.set("dry_run", dry);
                            text_result(j.to_string(), false)
                        }
                        Err(e) => text_result(e, true),
                    }
                }
                _ => return error(id, -32602, "unknown tool"),
            };
            reply(id, result)
        }
        m if m.starts_with("notifications/") => None,
        _ => error(id, -32601, "method not found"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{ctx, git, touch, world, write};

    fn is_error(r: &Json) -> bool {
        r.get("result").and_then(|x| x.get("isError")) == Some(&Json::Bool(true))
    }

    #[test]
    fn refuses_to_work_on_home_or_a_folder_that_is_not_a_project() {
        let (ctx, _, _) = setup("mcp-home");
        let scan = r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"clean","arguments":{}}}"#;
        // A dotfiles repo at ~ makes ~ the "project": still refused.
        git(&ctx.home, &["init", "-q"], Some(OLD));
        let r = call(&ctx, &ctx.home, scan);
        assert!(is_error(&r) && r.to_string().contains("home folder"), "{r}");
        let docs = ctx.home.join("Documents");
        write(&docs.join("notes.txt"), "x");
        let r = call(&ctx, &docs, scan);
        assert!(is_error(&r) && r.to_string().contains("not a project folder"), "{r}");
    }

    const OLD: u64 = 1_600_000_000;

    fn call(ctx: &Ctx, project: &Path, msg: &str) -> Json {
        handle(ctx, project, msg).expect("a reply")
    }

    fn setup(name: &str) -> (Ctx, PathBuf, PathBuf) {
        let ctx = ctx(name);
        let mk = |n: &str| {
            let p = world(&ctx).join(n);
            write(&p.join("package.json"), "{}");
            write(&p.join("package-lock.json"), "{}");
            write(&p.join(".gitignore"), "node_modules/\n");
            git(&p, &["init", "-q"], Some(OLD));
            git(&p, &["add", "-A"], Some(OLD));
            git(&p, &["commit", "-qm", "c"], Some(OLD));
            write(&p.join("node_modules/x/i.js"), "x");
            write(&p.join("node_modules/.package-lock.json"), "{}");
            touch(&p.join("node_modules/x/i.js"), OLD);
            p
        };
        (ctx.clone(), mk("mine"), mk("other"))
    }

    #[test]
    fn speaks_json_rpc_and_offers_only_scan_and_clean() {
        let (ctx, mine, _) = setup("mcp-proto");
        let r = call(
            &ctx,
            &mine,
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{}}}"#,
        );
        assert_eq!(r.get("result").unwrap().s("protocolVersion"), "2025-03-26");
        assert!(handle(&ctx, &mine, r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
        let r = call(&ctx, &mine, r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#);
        let names: Vec<&str> =
            r.get("result").unwrap().get("tools").unwrap().arr().iter().map(|t| t.s("name")).collect();
        assert_eq!(names, ["scan", "clean"]);
        let r = call(
            &ctx,
            &mine,
            r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"undo","arguments":{}}}"#,
        );
        assert_eq!(r.get("error").unwrap().get("code"), Some(&Json::Int(-32602)));
        let r = call(&ctx, &mine, r#"{"jsonrpc":"2.0","id":4,"method":"settings/set"}"#);
        assert_eq!(r.get("error").unwrap().get("code"), Some(&Json::Int(-32601)));
        assert_eq!(call(&ctx, &mine, "{nope").get("error").unwrap().get("code"), Some(&Json::Int(-32700)));
    }

    #[test]
    fn tools_stay_inside_the_clients_project_whatever_the_arguments() {
        let (mut ctx, mine, other) = setup("mcp-scope");
        let msg = format!(
            r#"{{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{{"name":"scan","arguments":{{"path":"{}"}}}}}}"#,
            other.display()
        );
        let r = call(&ctx, &mine, &msg);
        let text = r.get("result").unwrap().get("content").unwrap().arr()[0].s("text").to_string();
        let items = Json::parse(&text).unwrap();
        assert_eq!(items.arr().len(), 1);
        assert_eq!(items.arr()[0].s("path"), mine.join("node_modules").to_string_lossy());
        // First week: dry run, nothing deleted.
        let clean = r#"{"jsonrpc":"2.0","id":6,"method":"tools/call","params":{"name":"clean","arguments":{}}}"#;
        call(&ctx, &mine, clean);
        assert!(mine.join("node_modules").exists());
        // After it, only this project's folder goes.
        ctx.now += 8 * crate::state::DAY;
        let r = call(&ctx, &mine, clean);
        assert!(!mine.join("node_modules").exists(), "{r}");
        assert!(other.join("node_modules").exists());
    }
}
