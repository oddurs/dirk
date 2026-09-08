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
        // The process itself, which is what tells a pane that was kept across
        // a handoff from one that was restarted -- and what a caller wanting to
        // signal something in a pane has otherwise no way to name.
        "pid": pane.pid(),
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

/// What a caller may ask that changes something, and what it gets back.
///
/// The twin of `read`, and split from it for the same reason: a read is
/// obviously a read, and this is obviously not one.
///
/// It takes the session rather than the whole program because that is all these
/// answers need, and taking only that is what makes them reachable from a test
/// without a terminal — which is what six hundred lines of them inside one
/// method on a ninety-field struct were not. `None` means this is not one of
/// them; the caller tries what is left.
pub fn write(
    session: &mut Session,
    area: ratatui::layout::Rect,
    cmd: &str,
    args: &[String],
) -> Option<crate::wire::Reply> {
    use crate::mux::session::Focus;
    use crate::wire::Reply;
    let arg = |n: usize| args.get(n).cloned().unwrap_or_default();
    Some(match cmd {
        "workspace.focus" => match target_workspace(session, &arg(0)) {
            Some((p, w)) => {
                session.focus = Focus::Ws { p, w };
                Reply::ok(serde_json::json!({ "focused": arg(0) }))
            }
            None => Reply::err("no such workspace"),
        },

        "workspace.create" => {
            let path = if arg(0).is_empty() {
                std::env::current_dir().unwrap_or_else(|_| crate::config::home())
            } else {
                crate::config::expand(&arg(0))
            };
            if !path.is_dir() {
                return Some(Reply::err("no such directory"));
            }
            let p = session.open_project(&path);
            // Saved and put back, as `pane.split` does: a background
            // command should not pull the attached human away from what
            // they were doing, nor resize their panes doing it.
            let was = session.focus;
            let made = session.new_workspace_at(p, &path, area.height, area.width);
            session.focus = was;
            session.refocus();
            match made {
                Some(()) => {
                    let w = session.projects[p].workspaces.len() - 1;
                    let id = session.projects[p].workspaces[w].id;
                    Reply::ok(serde_json::json!({ "workspace": workspace_id(id) }))
                }
                None => Reply::err("could not start a shell there"),
            }
        }

        "workspace.rename" => match target_workspace(session, &arg(0)) {
            Some((p, w)) => {
                let name = args[1..].join(" ");
                if name.trim().is_empty() {
                    return Some(Reply::err("a name, or nothing to hand it back"));
                }
                let Some(ws) = session.workspace_mut(p, w) else {
                    return Some(Reply::err("no such workspace"));
                };
                ws.label = name.clone();
                // Named from outside is named by a human: naming stands
                // down until the hold is released.
                ws.naming.held = true;
                ws.naming.applied = Some(name.clone());
                Reply::ok(serde_json::json!({ "label": name }))
            }
            None => Reply::err("no such workspace"),
        },

        "workspace.close" => match target_workspace(session, &arg(0)) {
            Some((p, w)) => {
                let ids: Vec<_> = session
                    .workspace(p, w)
                    .map(|ws| ws.panes().iter().map(|x| x.id).collect())
                    .unwrap_or_default();
                for id in ids {
                    if let Some(ws) = session.workspace_mut(p, w)
                        && let Some(pane) = ws.pane_mut(id)
                    {
                        pane.close();
                    }
                }
                Reply::ok(serde_json::json!({ "closed": arg(0) }))
            }
            None => Reply::err("no such workspace"),
        },

        "pane.focus" => match target_pane(session, &arg(0)) {
            Some(id) => match locate(session, id) {
                Some((p, w)) => {
                    session.focus = Focus::Ws { p, w };
                    if let Some(ws) = session.workspace_mut(p, w) {
                        ws.set_focus(id);
                    }
                    Reply::ok(serde_json::json!({ "focused": arg(0) }))
                }
                None => Reply::err("no such pane"),
            },
            None => Reply::err("no such pane"),
        },

        "pane.split" => {
            let Some(id) = target_pane(session, &arg(0)) else {
                return Some(Reply::err("no such pane"));
            };
            let Some((p, w)) = locate(session, id) else {
                return Some(Reply::err("no such pane"));
            };
            let dir = if arg(1).eq_ignore_ascii_case("rows") {
                crate::mux::tree::Dir::Rows
            } else {
                crate::mux::tree::Dir::Cols
            };

            // Split where asked, not wherever the human happens to be
            // looking. A caller that meant "here" said so with an id.
            let was = session.focus;
            session.focus = Focus::Ws { p, w };
            if let Some(ws) = session.workspace_mut(p, w) {
                ws.set_focus(id);
            }
            session.split(dir, area.height, area.width);
            let new = session.workspace(p, w).map(|ws| ws.focus());
            session.focus = was;

            match new {
                Some(pane) => {
                    let ws_id = session.workspace(p, w).map(|x| x.id).unwrap_or(0);
                    Reply::ok(serde_json::json!({ "pane": pane_id(ws_id, pane) }))
                }
                None => Reply::err("could not split"),
            }
        }

        // Three verbs where there was one. A command, literal text and a
        // keystroke fail in different ways -- text can be pasted, a key
        // cannot, and a command needs both in an order that is guaranteed
        // -- so a single verb meant every caller wrote the ordering itself
        // and got it wrong against anything slow to read.
        "pane.run" | "pane.send-text" => {
            let Some(id) = target_pane(session, &arg(0)) else {
                return Some(Reply::err("no such pane"));
            };
            if session.pane_alive(id) == Some(false) {
                return Some(Reply::err("that pane's program has exited"));
            }
            let text = args[1..].join(" ");
            if text.is_empty() {
                return Some(Reply::err(format!("{} needs something to send", cmd)));
            }
            // The submitting return goes in the same write as the command.
            // Two writes is two chances for a program reading slowly to
            // see a bare newline and run whatever it had, which is how a
            // caller ends up having typed half a command.
            let mut bytes = text.clone().into_bytes();
            if cmd == "pane.run" {
                bytes.push(b'\r');
            }
            match session.write_to(id, &bytes) {
                true => Reply::ok(serde_json::json!({ "sent": text })),
                false => Reply::err("no such pane"),
            }
        }

        "pane.send-keys" => {
            let Some(id) = target_pane(session, &arg(0)) else {
                return Some(Reply::err("no such pane"));
            };
            if session.pane_alive(id) == Some(false) {
                return Some(Reply::err("that pane's program has exited"));
            }
            if args.len() < 2 {
                return Some(Reply::err("pane.send-keys needs a key"));
            }
            let mut keys = Vec::new();
            for name in &args[1..] {
                match crate::keys::named(name) {
                    Some(k) => keys.push(k),
                    None => return Some(Reply::err(format!("no such key: {name}"))),
                }
            }
            match session.keys_to(id, &keys) {
                true => Reply::ok(serde_json::json!({ "sent": args[1..] })),
                false => Reply::err("that key has no sequence on this terminal"),
            }
        }

        // Display, deliberately not state. `agent state` is a small closed
        // set dirk reasons about — it drives waits, notifications, ordering
        // and the attention column — and it has to stay that way. This is
        // where everything a program wants to *show* goes instead, so an
        // indexer's progress stops having to become a state in order to be
        // visible.
        "pane.metadata" => {
            let Some(id) = target_pane(session, &arg(0)) else {
                return Some(Reply::err("no such pane"));
            };
            for pair in &args[1..] {
                let Some((key, value)) = pair.split_once('=') else {
                    return Some(Reply::err(format!("{pair:?} is not key=value")));
                };
                if key.is_empty() {
                    return Some(Reply::err("a token needs a name"));
                }
                session.report_metadata(id, key, value);
            }
            match session.metadata(id) {
                Some(said) => Reply::ok(serde_json::json!({
                    "pane": arg(0),
                    "said": said,
                })),
                None => Reply::err("no such pane"),
            }
        }

        // A pane keeps its process, its scrollback and its agent identity
        // across the move: the `Pane` itself travels, because none of
        // those live anywhere else. Its handle does not change either —
        // dirk's pane ids are session-wide — so anything already holding
        // `p12`, a wait included, keeps working.
        "pane.move" => {
            let (words, opts) = crate::wait::options(args);
            if let Some(bad) = crate::wait::unknown(&opts, &["tab", "new-tab", "new-workspace"]) {
                return Some(Reply::err(format!("pane.move takes no --{bad}")));
            }
            let Some(target) = words.first() else {
                return Some(Reply::err("pane.move needs a pane"));
            };
            let Some(id) = target_pane(session, target) else {
                return Some(Reply::err("no such pane"));
            };
            let was = pane_id_of(session, id);
            let to = match opts
                .iter()
                .find(|(k, _)| k.starts_with("tab") || k.starts_with("new"))
            {
                Some((k, v)) if k == "tab" => match parse_tab(v) {
                    Some(tab) => crate::mux::session::Move::Tab(tab),
                    None => return Some(Reply::err(format!("not a tab id: {v}"))),
                },
                Some((k, _)) if k == "new-tab" => crate::mux::session::Move::NewTab,
                Some(_) => crate::mux::session::Move::NewWorkspace,
                None => {
                    return Some(Reply::err(
                        "pane.move needs --tab, --new-tab or --new-workspace",
                    ));
                }
            };
            match session.move_pane(id, to, area) {
                Ok(moved) => Reply::ok(serde_json::json!({
                    "pane": pane_id_of(session, moved),
                    // The workspace half of the id is a statement about
                    // where the pane is, so a caller holding the old one
                    // is told rather than left to find out.
                    "previous": was,
                })),
                Err(why) => Reply::err(why),
            }
        }

        "pane.close" => {
            let Some(id) = target_pane(session, &arg(0)) else {
                return Some(Reply::err("no such pane"));
            };
            match session.close_pane(id) {
                true => Reply::ok(serde_json::json!({ "closed": arg(0) })),
                false => Reply::err("no such pane"),
            }
        }

        "layout.open" => match session.layouts.iter().position(|l| l.def.name == arg(0)) {
            Some(i) => {
                session.open_layout(i, area);
                Reply::ok(serde_json::json!({ "opened": arg(0) }))
            }
            None => Reply::err("no such layout"),
        },

        "worktree.list" => {
            let Some(dir) = session.focused_dir() else {
                return Some(Reply::err("no project focused"));
            };
            let here = session.focused_dir();
            Reply::ok(serde_json::json!({
                "worktrees": crate::git::worktrees(&dir)
                    .into_iter()
                    .map(|t| serde_json::json!({
                        "path": t.path,
                        "branch": t.branch,
                        "main": t.main,
                        // Which one you are in, since the answer is a list
                        // of directories that all look alike.
                        "open": here.as_deref() == Some(t.path.as_path()),
                    }))
                    .collect::<Vec<_>>(),
            }))
        }

        "worktree.remove" => {
            let Some(dir) = session.focused_dir() else {
                return Some(Reply::err("no project focused"));
            };
            let want = arg(0);
            let force = args.iter().any(|a| a == "--force");
            // By branch or by path, because a branch is what you remember
            // and a path is what `worktree list` gave you.
            let Some(tree) = crate::git::worktrees(&dir)
                .into_iter()
                .find(|t| t.branch == want || t.path.to_string_lossy() == want)
            else {
                return Some(Reply::err(format!("no worktree for {want}")));
            };
            if tree.main {
                return Some(Reply::err("that is the repository, not a worktree of it"));
            }
            match crate::git::remove(&dir, &tree.path, force) {
                Ok(()) => {
                    session.close_checkout(&tree.path);
                    Reply::ok(serde_json::json!({ "removed": tree.path }))
                }
                Err(why) => Reply::err(why),
            }
        }

        // Rank one: the agent saying what it is doing, which is a fact
        // where everything else dirk has is a guess.
        "agent.state" => {
            let Some(state) = crate::agent::State::named(&arg(0)) else {
                return Some(Reply::err("no such state"));
            };
            // No target means the workspace you are looking at. A target
            // that was given and did not resolve is an error, not an
            // invitation to pick one: `--current` outside a pane arrives
            // here as the literal string, and a hook firing just after its
            // workspace closed would otherwise land its state -- and its
            // notification, and its noise -- on whatever you happen to be
            // looking at instead.
            let target = match arg(1).is_empty() {
                true => match session.focus {
                    Focus::Ws { p, w } => Some((p, w)),
                    Focus::Layout(_) => None,
                },
                false => target_workspace(session, &arg(1)),
            };
            let Some((p, w)) = target else {
                return Some(Reply::err("no such workspace"));
            };
            // The harness's own name for this conversation, when the hook
            // passed one along. Taken before the workspace is borrowed
            // mutably, because reading it needs the kinds.
            let named = args
                .iter()
                .position(|a| a == "--session")
                .and_then(|i| args.get(i + 1))
                .filter(|id| !id.is_empty())
                .cloned();
            let kind = session
                .workspace(p, w)
                .and_then(|ws| ws.active_pane())
                .and_then(|pane| pane.occupant.agent())
                .map(|k| k.name.clone());
            let Some(ws) = session.workspace_mut(p, w) else {
                return Some(Reply::err("no such workspace"));
            };
            ws.reported = Some((state, std::time::Instant::now()));
            // Kept only alongside the harness it came from: a reference is
            // only meaningful to the program that issued it, and resuming
            // codex on claude's id would start a conversation nobody had.
            if let (Some(id), Some(kind)) = (named, kind) {
                ws.agent_session = Some((kind, id));
            }
            Reply::ok(serde_json::json!({
                "workspace": workspace_id(ws.id),
                "state": state.glyph_name(),
                "session": ws.agent_session.as_ref().map(|(_, id)| id.clone()),
            }))
        }

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
    Command {
        name: "session.handoff",
        args: "",
        // Only ever an error. When it works this process becomes the new binary
        // partway through the call and there is nobody left to answer -- the
        // caller learns by its connection ending, which is the honest report.
        answer: "an error if it did not happen; nothing at all if it did",
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

    /// A session with one project open in it, and the area a client would give
    /// it.
    ///
    /// The whole point of `write` taking a session rather than the program: this
    /// is four lines and no terminal. The same answers used to be six hundred
    /// lines inside one method on a ninety-field struct, reachable only by
    /// starting dirk on a pseudo-terminal and typing at it.
    fn a_session() -> (crate::mux::session::Session, ratatui::layout::Rect) {
        let cfg = crate::config::Config::default();
        let (tx, rx) = std::sync::mpsc::channel();
        // Held: the panes post to it, and a dropped receiver makes every send
        // fail, which is not what any of these tests is about.
        std::mem::forget(rx);
        let mut session = crate::mux::session::Session::new(&cfg, tx);
        session.open_project(std::path::Path::new("."));
        (
            session,
            ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 80,
                height: 24,
            },
        )
    }

    /// A workspace of its own, and the id of the pane in it.
    fn a_workspace_with_a_pane(
        session: &mut crate::mux::session::Session,
        area: ratatui::layout::Rect,
    ) -> String {
        let made = say(session, area, "workspace.create", &[]);
        assert!(made.ok, "workspace.create failed: {made:?}");
        let ws = made.result["workspace"]
            .as_str()
            .expect("an id")
            .to_string();
        let (p, w) = target_workspace(session, &ws).expect("it is there");
        let pane = session
            .workspace(p, w)
            .and_then(|it| it.panes().first().map(|pane| pane.id))
            .expect("a workspace arrives with a pane in it");
        pane_id_of(session, pane).expect("the pane has an id")
    }

    fn say(
        session: &mut crate::mux::session::Session,
        area: ratatui::layout::Rect,
        cmd: &str,
        args: &[&str],
    ) -> crate::wire::Reply {
        let args: Vec<String> = args.iter().map(|a| (*a).to_string()).collect();
        write(session, area, cmd, &args).unwrap_or_else(|| panic!("{cmd} is not a writing command"))
    }

    #[test]
    fn a_command_that_writes_nothing_is_not_this_half_s() {
        // `None` is how the two halves divide the surface between them: a read
        // falls through to whatever is left, and so does a command that does
        // not exist. Answering either here would take it away from the code
        // that actually handles it.
        let (mut session, area) = a_session();
        assert!(write(&mut session, area, "pane.list", &[]).is_none());
        assert!(write(&mut session, area, "nonsense.thing", &[]).is_none());
    }

    #[test]
    fn a_workspace_is_made_and_then_named_and_then_closed() {
        let (mut session, area) = a_session();

        let made = say(&mut session, area, "workspace.create", &[]);
        assert!(made.ok, "workspace.create failed: {made:?}");
        let id = made.result["workspace"]
            .as_str()
            .expect("the new workspace's id")
            .to_string();
        assert!(
            target_workspace(&session, &id).is_some(),
            "it answered with an id that names nothing"
        );

        let named = say(&mut session, area, "workspace.rename", &[&id, "a", "name"]);
        assert!(named.ok, "workspace.rename failed: {named:?}");
        let (p, w) = target_workspace(&session, &id).expect("it is still there");
        assert_eq!(
            session.workspace(p, w).map(|ws| ws.label.as_str()),
            Some("a name")
        );

        // Closing ends the panes and stops there. The workspace goes when their
        // exits reach the event loop, which is the half of this that is not
        // here -- and is why the answer is about what was asked rather than
        // about what is left.
        let closed = say(&mut session, area, "workspace.close", &[&id]);
        assert!(closed.ok, "workspace.close failed: {closed:?}");
        let (p, w) = target_workspace(&session, &id).expect("still there, for now");
        let alive = session
            .workspace(p, w)
            .map(|ws| ws.panes().iter().filter(|pane| !pane.dead).count())
            .unwrap_or_default();
        assert_eq!(
            alive, 0,
            "it said it closed {id} and something is still running"
        );
    }

    #[test]
    fn a_name_of_nothing_but_spaces_is_refused() {
        // Handing a name back is `rename` with no name at all. A name made of
        // spaces is a mistake, and taking it would leave a row that looks
        // unnamed and cannot be renamed by the thing that names rows.
        let (mut session, area) = a_session();
        let made = say(&mut session, area, "workspace.create", &[]);
        let id = made.result["workspace"]
            .as_str()
            .expect("an id")
            .to_string();
        let said = say(&mut session, area, "workspace.rename", &[&id, "   "]);
        assert!(!said.ok, "a name of spaces was taken");
    }

    #[test]
    fn a_workspace_that_is_not_there_is_said_so_rather_than_guessed_at() {
        let (mut session, area) = a_session();
        for (cmd, args) in [
            ("workspace.focus", vec!["w9999"]),
            ("workspace.rename", vec!["w9999", "x"]),
            ("workspace.close", vec!["w9999"]),
            ("pane.focus", vec!["w9999:p1"]),
            ("pane.close", vec!["w9999:p1"]),
        ] {
            let said = say(&mut session, area, cmd, &args);
            assert!(!said.ok, "{cmd} answered for a workspace that is not there");
            assert!(
                said.error
                    .as_deref()
                    .unwrap_or_default()
                    .contains("no such"),
                "{cmd} said {:?} rather than what was wrong",
                said.error
            );
        }
    }

    #[test]
    fn splitting_a_pane_makes_another_and_answers_with_its_id() {
        let (mut session, area) = a_session();
        let pane = a_workspace_with_a_pane(&mut session, area);

        let split = say(&mut session, area, "pane.split", &[&pane, "cols"]);
        assert!(split.ok, "pane.split failed: {split:?}");
        let made = split.result["pane"].as_str().expect("the new pane's id");
        assert_ne!(made, pane, "it answered with the pane it was given");
        assert!(
            target_pane(&session, made).is_some(),
            "the id it answered with does not name a pane"
        );
    }

    #[test]
    fn sending_keys_needs_a_key_dirk_can_read() {
        let (mut session, area) = a_session();
        let pane = a_workspace_with_a_pane(&mut session, area);

        let said = say(&mut session, area, "pane.send-keys", &[&pane]);
        assert!(!said.ok, "send-keys with no key was answered");

        let said = say(
            &mut session,
            area,
            "pane.send-keys",
            &[&pane, "ctrl+zarquon"],
        );
        assert!(!said.ok, "a key nobody can read was accepted");
        assert!(
            said.error
                .as_deref()
                .unwrap_or_default()
                .contains("no such key"),
            "it said {:?} rather than which key",
            said.error
        );
    }
}
