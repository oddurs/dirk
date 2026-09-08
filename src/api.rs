// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.
//
// This program is distributed in the hope that it will be useful, but WITHOUT
// ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along with
// this program.  If not, see <https://www.gnu.org/licenses/>.

//! What an agent inside a pane can ask of the session it is in.
//!
//! This is the difference between a multiplexer agents happen to run in and one
//! they can work in. herdr's value is largely that `herdr pane split` works from
//! inside a pane; the same is true here.
//!
//! **Ids are opaque and stable.** `w7` is a workspace, `w7:p12` is a pane in it.
//! The number the nav shows beside a workspace is positional and changes when
//! spaces are reordered — it is for reading, not for addressing.
//!
//! **Reads do not mark an agent seen.** Focusing a workspace is what says you
//! have looked at it; asking about one over a socket is not looking. Without
//! that rule a status line polling the session would quietly clear every
//! notification it was built to show.

use crate::mux::{Focus, PaneId, Session};
use serde_json::{Value, json};

/// How far back a read goes when the caller does not say.
///
/// A screenful and a bit. Named because `pane.read` and `pane.wait-output` have
/// to agree: a caller that waits for a line and then reads to get its context
/// should not find that the wait was looking further back than the read.
pub const READ_LINES: u16 = 50;

/// `w7`, or `w7:p12`.
pub fn workspace_id(ws: u64) -> String {
    format!("w{ws}")
}

pub fn pane_id(ws: u64, pane: PaneId) -> String {
    format!("w{ws}:p{pane}")
}

pub fn tab_id(ws: u64, tab: u64) -> String {
    format!("w{ws}:t{tab}")
}

/// Read a tab handle, in either form.
pub fn parse_tab(text: &str) -> Option<u64> {
    let tail = text.rsplit(':').next()?;
    tail.strip_prefix('t')?.parse().ok()
}

/// Read a workspace or pane handle in either form.
///
/// A pane id is unique on its own, so the workspace half is a courtesy to the
/// reader rather than something the lookup needs — and accepting the short form
/// means an id copied out of one reply works in the next command.
fn parse_pane(text: &str) -> Option<PaneId> {
    let tail = text.rsplit(':').next()?;
    tail.strip_prefix('p')?.parse().ok()
}

fn parse_workspace(text: &str) -> Option<u64> {
    text.split(':').next()?.strip_prefix('w')?.parse().ok()
}

/// Everything one workspace is, as a caller sees it.
fn workspace_json(session: &Session, p: usize, w: usize) -> Value {
    let Some(proj) = session.projects.get(p) else {
        return Value::Null;
    };
    let Some(ws) = proj.workspaces.get(w) else {
        return Value::Null;
    };
    let tokens = session.tokens(p, w);
    json!({
        "id": workspace_id(ws.id),
        "label": ws.label,
        "project": proj.name,
        // The space's own checkout, not the project's. A project is a
        // repository now and a repository has more than one directory; saying
        // the repository proper for a space that is in a worktree is a wrong
        // answer rather than a vague one.
        "path": ws.at,
        "repository": proj.path,
        "state": ws.state.glyph_name(),
        "seen": ws.seen,
        "held": ws.naming.held,
        "focused": session.focus == Focus::Ws { p, w },
        "panes": ws.tree().leaves().len(),
        "branch": tokens.get("branch"),
        "worktree": tokens.get("worktree").is_some(),
        // Null rather than zero when there is no upstream to compare against.
        // Read straight from the checkout rather than through the tokens: how
        // far a branch has drifted is not a thing to name a workspace after.
        "ahead": proj.repo_of(w).and_then(|r| r.track).map(|(a, _)| a),
        "behind": proj.repo_of(w).and_then(|r| r.track).map(|(_, b)| b),
        "agent": tokens.get("agent"),
        "since": tokens.get("since"),
        "age": tokens.get("age"),
    })
}

fn pane_json(ws_id: u64, pane: &crate::mux::Pane, focused: bool) -> Value {
    // How big it is, which a caller writing into one has to know: a command
    // that wraps at the wrong column reads as a different command, and there
    // was no way to ask. Also how the smallest-client rule is checked.
    let (rows, cols) = pane.term.lock().ok().map_or((0, 0), |t| t.screen().size());
    json!({
        "id": pane_id(ws_id, pane.id),
        "workspace": workspace_id(ws_id),
        "rows": rows,
        "cols": cols,
        "cwd": pane.cwd,
        "program": pane.argv.first(),
        "agent": pane.occupant.agent().map(|k| k.name.clone()),
        "agent_name": pane.agent_name,
        "available": pane.occupant.available(),
        "dead": pane.dead,
        "exit": pane.exit,
        "focused": focused,
    })
}

/// One agent, as a caller sees it, or `None` if this workspace holds none.
///
/// Written once so that what `agent.list` says and what a wait answers with
/// cannot describe the same agent differently.
pub fn agent_json(session: &Session, p: usize, w: usize) -> Option<Value> {
    let ws = session.workspace(p, w)?;
    let pane = ws.active_pane()?;
    // A harness dirk does not recognise but which reports its own state is
    // still an agent -- being told is the best signal there is, and a list that
    // ignored it would make rank one worth less than the guessing it replaced.
    let kind = pane.occupant.agent();
    if kind.is_none() && ws.reported.is_none() {
        return None;
    }
    Some(json!({
        "name": pane.agent_name,
        "kind": kind.map(|k| k.name.clone()),
        "pane": pane_id(ws.id, pane.id),
        "workspace": workspace_id(ws.id),
        "state": ws.state.glyph_name(),
        // Which signal decided it. A badge you cannot explain is a badge you
        // stop believing, and when one is wrong this is what says which source
        // was wrong.
        "why": ws.source.name(),
        "available": pane.occupant.available(),
        "intent": ws.intent,
    }))
}

/// What a caller may ask, and what it gets back.
///
/// Split from the mutating half so a read is obviously a read — including to
/// the seen rule, which must not be touched here.
pub fn read(
    session: &Session,
    kinds: &[crate::agent::Kind],
    cmd: &str,
    args: &[String],
) -> Option<crate::wire::Reply> {
    use crate::wire::Reply;
    Some(match cmd {
        "workspace.list" => {
            let list: Vec<Value> = session
                .flat()
                .into_iter()
                .map(|(p, w)| workspace_json(session, p, w))
                .collect();
            Reply::ok(json!({ "workspaces": list }))
        }

        "pane.list" => {
            // An argument that is present but unreadable is a mistake, not an
            // absent filter -- the caller asked about one workspace and would
            // otherwise be handed every pane in the session, successfully.
            let want = match args.first() {
                None => None,
                Some(a) => match parse_workspace(a) {
                    Some(id) => Some(id),
                    None => return Some(Reply::err("not a workspace id")),
                },
            };
            let mut list = Vec::new();
            for (p, w) in session.flat() {
                let Some(ws) = session.workspace(p, w) else {
                    continue;
                };
                if want.is_some_and(|id| id != ws.id) {
                    continue;
                }
                for id in ws.tree().leaves() {
                    let Some(pane) = ws.pane(id) else { continue };
                    list.push(pane_json(ws.id, pane, ws.focus() == id));
                }
            }
            Reply::ok(json!({ "panes": list }))
        }

        "pane.read" => {
            let Some(target) = args.first().and_then(|a| parse_pane(a)) else {
                return Some(Reply::err("pane.read needs a pane id"));
            };
            let lines: u16 = args
                .get(1)
                .and_then(|n| n.parse().ok())
                .unwrap_or(READ_LINES);
            match session.pane_text(target, lines) {
                Some(text) => Reply::ok(json!({ "text": text })),
                None => Reply::err("no such pane"),
            }
        }

        "layout.list" => {
            let list: Vec<Value> = session
                .layouts
                .iter()
                .map(|l| json!({ "name": l.def.name, "key": l.def.key, "open": l.ws.is_some() }))
                .collect();
            Reply::ok(json!({ "layouts": list }))
        }

        // Agents are workspaces holding one, which is the only thing that can
        // be addressed by name.
        "tab.list" => {
            let want = match args.first() {
                None => None,
                Some(a) => match parse_workspace(a) {
                    Some(id) => Some(id),
                    None => return Some(Reply::err("not a workspace id")),
                },
            };
            let mut list = Vec::new();
            for (p, w) in session.flat() {
                let Some(ws) = session.workspace(p, w) else {
                    continue;
                };
                if want.is_some_and(|id| id != ws.id) {
                    continue;
                }
                for (i, tab) in ws.tabs.iter().enumerate() {
                    list.push(json!({
                        "id": tab_id(ws.id, tab.id),
                        "workspace": workspace_id(ws.id),
                        "name": ws.tab_label(i),
                        "panes": tab.panes.len(),
                        "focused": ws.tab == i,
                    }));
                }
            }
            Reply::ok(json!({ "tabs": list }))
        }

        // Which rules a harness is being recognised by, and where they came
        // from. When a state is wrong the first question is which rules decided
        // it, and "the ones dirk ships" and "the ones in the file you wrote
        // last week" are very different answers.
        "agent.rules" => {
            let list: Vec<Value> = kinds
                .iter()
                .map(|k| {
                    json!({
                        "name": k.name,
                        "from": k.from.name(),
                        "names": k.names,
                        "argv": k.argv,
                        "command": k.command,
                        "blocked": { "menu": k.choices, "match": k.blocked },
                    })
                })
                .collect();
            Reply::ok(json!({ "agents": list }))
        }

        // Why this pane is in the state it is. Four ranked signals decide one,
        // and `agent list` names only the winner -- which is not enough when
        // the answer is wrong, because then what you need is what the other
        // three said and which marker did or did not match.
        "agent.explain" => {
            let Some(target) = args.first() else {
                return Some(Reply::err("agent.explain needs a workspace or a pane"));
            };
            let Some((p, w)) = target_workspace(session, target) else {
                return Some(Reply::err(format!("no such workspace or pane: {target}")));
            };
            let Some(ws) = session.workspace(p, w) else {
                return Some(Reply::err("no such workspace"));
            };
            let seen = crate::mux::session::explain(ws, std::time::Instant::now());
            let signals: Vec<Value> = seen
                .signals
                .iter()
                .enumerate()
                .map(|(rank, (name, claim, note))| {
                    json!({
                        "rank": rank + 1,
                        "signal": name,
                        "claims": claim.map(|s| s.glyph_name()),
                        "note": note,
                    })
                })
                .collect();
            let pane = ws.active_pane();
            let kind = pane.and_then(|p| p.occupant.agent());
            Reply::ok(json!({
                "workspace": workspace_id(ws.id),
                "pane": pane.map(|p| pane_id(ws.id, p.id)),
                "agent": kind.map(|k| k.name.clone()),
                // Which rules were in play, since a marker that does not match
                // is as often a rules problem as a screen one.
                "rules": kind.map(|k| k.from.name()),
                // What is on the row, which is what this is really about.
                "state": ws.state.glyph_name(),
                "why": ws.source.name(),
                "seen": ws.seen,
                "signals": signals,
                "screen": seen.found.as_ref().map(|f| json!({
                    "marker": f.marker,
                    "line": f.line,
                    "menu_required": f.needs_menu,
                    "menu_found": f.menu,
                    "blocked": f.blocked(),
                })),
                "window": seen.window,
            }))
        }

        "agent.list" => {
            let list: Vec<Value> = session
                .flat()
                .into_iter()
                .filter_map(|(p, w)| agent_json(session, p, w))
                .collect();
            Reply::ok(json!({ "agents": list }))
        }

        // The surface, from the surface. A caller that has to read the source
        // to find out what it may ask is one that will guess.
        "session.commands" => Reply::ok(json!({ "commands": COMMANDS })),

        // Also answered without a session, by the CLI. Here as well so that a
        // caller already holding a socket does not have to shell out to ask
        // what it may say down it.
        "api.schema" => Reply::ok(schema()),

        _ => return None,
    })
}

/// One command, as the table below records it.
#[derive(Debug, Clone, Copy, serde::Serialize)]
pub struct Command {
    /// The noun and the verb, as they go over the socket.
    pub name: &'static str,
    /// What it takes, in the notation a usage line uses.
    pub args: &'static str,
    /// The keys its answer carries, comma-separated. A `[]` suffix is a list.
    ///
    /// Names rather than types: what a caller needs from this is which key to
    /// read, and a type would be a promise about a value dirk does not police.
    pub answer: &'static str,
}

/// The nouns a session answers to.
///
/// Here rather than beside the argument parser so the table below and the
/// routing cannot disagree -- which they did, silently, the moment a noun was
/// added to one of them.
pub const NOUNS: &[&str] = &[
    "api",
    "workspace",
    "pane",
    "layout",
    "agent",
    "session",
    "worktree",
    "tab",
];

/// Every command: what it takes, and what its answer carries.
///
/// Kept beside the handlers so the two cannot drift without somebody noticing,
/// and read by three things — the agent skill, the shell completions and
/// `dirk api schema` — so that none of them is a second copy of this one.
pub const COMMANDS: &[Command] = &[
    Command {
        name: "api.schema",
        args: "",
        answer: "version, nouns[], commands[], reply",
    },
    Command {
        name: "workspace.list",
        args: "",
        answer: "workspaces[]",
    },
    Command {
        name: "workspace.focus",
        args: "<workspace|pane>",
        answer: "focused",
    },
    Command {
        name: "workspace.create",
        args: "[path]",
        answer: "workspace",
    },
    Command {
        name: "workspace.rename",
        args: "<workspace> <name...>",
        answer: "label",
    },
    Command {
        name: "workspace.close",
        args: "<workspace>",
        answer: "closed",
    },
    Command {
        name: "pane.list",
        args: "[workspace]",
        answer: "panes[]",
    },
    Command {
        name: "pane.focus",
        args: "<pane>",
        answer: "focused",
    },
    Command {
        name: "pane.split",
        args: "<pane> [cols|rows]",
        answer: "pane",
    },
    Command {
        name: "pane.read",
        args: "<pane> [lines]",
        answer: "text",
    },
    Command {
        name: "pane.run",
        args: "<pane> <command...>",
        answer: "sent",
    },
    Command {
        name: "pane.send-text",
        args: "<pane> <text...>",
        answer: "sent",
    },
    Command {
        name: "pane.send-keys",
        args: "<pane> <key...>",
        answer: "sent[]",
    },
    Command {
        name: "pane.wait-output",
        args: "<pane> <text...> [--regex] [--lines N] [--timeout MS]",
        answer: "pane, matched",
    },
    Command {
        name: "pane.metadata",
        args: "<pane> [key=value]... (an empty value clears)",
        answer: "pane, said",
    },
    Command {
        name: "pane.move",
        args: "<pane> --tab <tab> | --new-tab | --new-workspace",
        answer: "pane, previous",
    },
    Command {
        name: "pane.attach",
        args: "<pane> [--takeover]",
        answer: "pane, rows, cols, then that pane's screen until you detach",
    },
    Command {
        name: "pane.close",
        args: "<pane>",
        answer: "closed",
    },
    Command {
        name: "layout.list",
        args: "",
        answer: "layouts[]",
    },
    Command {
        name: "layout.open",
        args: "<name>",
        answer: "opened",
    },
    Command {
        name: "tab.list",
        args: "[workspace]",
        answer: "tabs[]",
    },
    Command {
        name: "tab.new",
        args: "[workspace]",
        answer: "tab",
    },
    Command {
        name: "tab.focus",
        args: "<tab>",
        answer: "focused",
    },
    Command {
        name: "tab.rename",
        args: "<tab> <name...>",
        answer: "renamed",
    },
    Command {
        name: "tab.close",
        args: "<tab>",
        answer: "closed",
    },
    Command {
        name: "worktree.list",
        args: "",
        answer: "worktrees[]",
    },
    Command {
        name: "worktree.add",
        args: "<branch>",
        answer: "branch, path, workspace",
    },
    Command {
        name: "worktree.remove",
        args: "<branch|path> [--force]",
        answer: "removed",
    },
    Command {
        name: "agent.list",
        args: "",
        answer: "agents[]",
    },
    Command {
        name: "agent.state",
        args: "<blocked|working|done|idle|starting> [workspace|pane]",
        answer: "state, workspace, seen",
    },
    Command {
        name: "agent.explain",
        args: "<workspace|pane>",
        answer: "workspace, pane, agent, rules, state, why, seen, signals[], screen, window",
    },
    Command {
        name: "agent.rules",
        args: "",
        answer: "agents[]",
    },
    Command {
        name: "agent.start",
        args: "<kind> [pane]",
        answer: "agent, pane, workspace",
    },
    Command {
        name: "agent.prompt",
        args: "<workspace|pane> <text...> [--wait] [--until STATE]... [--timeout MS]",
        answer: "agent",
    },
    Command {
        name: "agent.wait",
        args: "<workspace|pane> [--until STATE]... [--timeout MS]",
        answer: "agent",
    },
    Command {
        name: "agent.hooks",
        args: "<kind>",
        answer: "kind, hooks",
    },
    Command {
        name: "session.info",
        args: "",
        answer: "version, workspaces, layouts, clients, attached, waiting",
    },
    Command {
        name: "session.commands",
        args: "",
        answer: "commands[]",
    },
    Command {
        name: "session.notify",
        args: "<workspace|pane> <text...> [--blocked]",
        answer: "notified, workspace, why",
    },
    Command {
        name: "session.reload",
        args: "",
        answer: "reloaded",
    },
    Command {
        name: "session.quit",
        args: "",
        answer: "quit",
    },
];

/// A pane's full handle, from the pane alone.
///
/// A caller that passed the short form gets the long one back, because the
/// answer is what goes into its next command and the long form says which
/// workspace it is now in.
pub fn pane_id_of(session: &Session, pane: PaneId) -> Option<String> {
    let (p, w) = locate(session, pane)?;
    Some(pane_id(session.workspace(p, w)?.id, pane))
}

/// The whole surface, as data.
///
/// Generated from the table above rather than written beside it, for the same
/// reason the skill is: a description of a surface that no longer exists is
/// worse than no description. Checked into the repository as `doc/api.json` so
/// that "the API changed" arrives as a diff in a pull request rather than as a
/// bug report from whoever was speaking it.
///
/// Ordered as the table is, which is by noun and then by the order the verbs
/// were added — stable, so the diff is about what changed.
pub fn schema() -> Value {
    json!({
        "version": env!("CARGO_PKG_VERSION"),
        "nouns": NOUNS,
        "commands": COMMANDS.iter().map(|c| json!({
            "name": c.name,
            "args": c.args,
            "answer": c.answer.split(", ").filter(|k| !k.is_empty()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        // Every answer is one of these two, which is the thing a caller most
        // needs to be told and the thing a usage line cannot say.
        "reply": {
            "ok": { "ok": true, "result": "the keys named by `answer`" },
            "err": { "ok": false, "error": "what went wrong, as a sentence" },
        },
    })
}

/// Look a pane up wherever it is, with the workspace that holds it.
pub fn locate(session: &Session, pane: PaneId) -> Option<(usize, usize)> {
    session.flat().into_iter().find(|&(p, w)| {
        session
            .workspace(p, w)
            .is_some_and(|ws| ws.panes().iter().any(|x| x.id == pane))
    })
}

pub fn find_workspace(session: &Session, id: u64) -> Option<(usize, usize)> {
    session
        .flat()
        .into_iter()
        .find(|&(p, w)| session.workspace(p, w).is_some_and(|ws| ws.id == id))
}

/// Resolve a target that may be a workspace, a pane, or `--current`.
pub fn target_workspace(session: &Session, arg: &str) -> Option<(usize, usize)> {
    if let Some(id) = parse_workspace(arg)
        && let Some(at) = find_workspace(session, id)
    {
        return Some(at);
    }
    parse_pane(arg).and_then(|p| locate(session, p))
}

pub fn target_pane(session: &Session, arg: &str) -> Option<PaneId> {
    parse_pane(arg).filter(|id| locate(session, *id).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_schema_is_the_one_this_binary_speaks() {
        // The point of checking it in: a change to the surface arrives as a
        // diff in a pull request rather than as a bug report from whoever was
        // speaking the old one.
        let want = include_str!("../doc/api.json");
        let have = serde_json::to_string_pretty(&schema()).expect("the schema serialises");
        assert_eq!(
            have.trim(),
            want.trim(),
            "doc/api.json is stale — run `make api`"
        );
    }

    #[test]
    fn every_command_the_code_answers_is_in_the_table() {
        // The drift this caught: `pane.wait-output` was answered, documented
        // and tested, and was not in the table — so the skill, the schema and
        // the completions all quietly stopped mentioning it. The table is read
        // by three things and written by hand, which is exactly the shape that
        // rots.
        //
        // Scanned out of the source because there is no way to ask the match
        // arms what they match. A literal that looks like a command is one.
        let sources = [include_str!("main.rs"), include_str!("api.rs")];
        let listed: Vec<&str> = COMMANDS.iter().map(|c| c.name).collect();
        for text in sources {
            for literal in text.split('"').skip(1).step_by(2) {
                let Some((noun, verb)) = literal.split_once('.') else {
                    continue;
                };
                if !NOUNS.contains(&noun) || verb.is_empty() {
                    continue;
                }
                // Decided on rather than merely mentioned. `"api.rs"` is a
                // filename and `"agent.state"` in a sentence is prose; what
                // makes a literal a command is that something branches on it.
                let branched = [
                    format!("\"{literal}\" =>"),
                    format!("\"{literal}\" |"),
                    format!("== \"{literal}\""),
                    format!("!= \"{literal}\""),
                ];
                if !branched
                    .iter()
                    .any(|pattern| text.contains(pattern.as_str()))
                {
                    continue;
                }
                assert!(
                    listed.contains(&literal),
                    "{literal} is answered somewhere and is not in COMMANDS"
                );
            }
        }
    }

    #[test]
    fn the_listed_commands_are_the_ones_that_exist() {
        // A surface that says it accepts something it does not is worse than
        // one that says nothing.
        let session = None::<()>;
        let _ = session;
        for cmd in COMMANDS {
            let name = cmd.name;
            assert!(name.contains('.'), "{name} is not a noun and a verb");
            let (noun, _) = name.split_once('.').unwrap();
            assert!(
                NOUNS.contains(&noun),
                "{name} has a noun the CLI does not route"
            );
        }
    }

    #[test]
    fn an_id_reads_in_either_form() {
        assert_eq!(parse_workspace("w7"), Some(7));
        assert_eq!(parse_workspace("w7:p12"), Some(7));
        // A pane id is unique on its own, so the workspace half is a courtesy
        // to whoever is reading it.
        assert_eq!(parse_pane("w7:p12"), Some(12));
        assert_eq!(parse_pane("p12"), Some(12));
    }

    #[test]
    fn nonsense_is_refused_rather_than_guessed_at() {
        assert_eq!(parse_workspace("7"), None);
        assert_eq!(parse_workspace(""), None);
        assert_eq!(parse_pane("w7"), None, "a workspace is not a pane");
        assert_eq!(parse_pane("w7:t1"), None, "nor is a tab");
        assert_eq!(parse_pane("p"), None);
    }

    #[test]
    fn an_id_survives_being_printed_and_read_back() {
        // Ids come back to dirk in the next command, copied out of a reply.
        let text = pane_id(7, 12);
        assert_eq!(text, "w7:p12");
        assert_eq!(parse_workspace(&text), Some(7));
        assert_eq!(parse_pane(&text), Some(12));
        assert_eq!(parse_workspace(&workspace_id(7)), Some(7));
    }
}
