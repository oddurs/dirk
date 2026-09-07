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
        "path": proj.path,
        "state": ws.state.glyph_name(),
        "seen": ws.seen,
        "held": ws.naming.held,
        "focused": session.focus == Focus::Ws { p, w },
        "panes": ws.tree().leaves().len(),
        "branch": tokens.get("branch"),
        "worktree": tokens.get("worktree").is_some(),
        "agent": tokens.get("agent"),
        "since": tokens.get("since"),
        "age": tokens.get("age"),
    })
}

fn pane_json(ws_id: u64, pane: &crate::mux::Pane, focused: bool) -> Value {
    json!({
        "id": pane_id(ws_id, pane.id),
        "workspace": workspace_id(ws_id),
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

/// What a caller may ask, and what it gets back.
///
/// Split from the mutating half so a read is obviously a read — including to
/// the seen rule, which must not be touched here.
pub fn read(session: &Session, cmd: &str, args: &[String]) -> Option<crate::wire::Reply> {
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
            let lines: u16 = args.get(1).and_then(|n| n.parse().ok()).unwrap_or(50);
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

        "agent.list" => {
            let mut list = Vec::new();
            for (p, w) in session.flat() {
                let Some(ws) = session.workspace(p, w) else {
                    continue;
                };
                let Some(pane) = ws.active_pane() else {
                    continue;
                };
                // A harness dirk does not recognise but which reports its own
                // state is still an agent -- being told is the best signal
                // there is, and a list that ignored it would make rank one
                // worth less than the guessing it replaced.
                let kind = pane.occupant.agent();
                if kind.is_none() && ws.reported.is_none() {
                    continue;
                }
                list.push(json!({
                    "name": pane.agent_name,
                    "kind": kind.map(|k| k.name.clone()),
                    "pane": pane_id(ws.id, pane.id),
                    "workspace": workspace_id(ws.id),
                    "state": ws.state.glyph_name(),
                    // Which signal decided it. A badge you cannot explain is a
                    // badge you stop believing, and when one is wrong this is
                    // what says which source was wrong.
                    "why": ws.source.name(),
                    "available": pane.occupant.available(),
                    "intent": ws.intent,
                }));
            }
            Reply::ok(json!({ "agents": list }))
        }

        // The surface, from the surface. A caller that has to read the source
        // to find out what it may ask is one that will guess.
        "session.commands" => Reply::ok(json!({ "commands": COMMANDS })),

        _ => return None,
    })
}

/// The nouns a session answers to.
///
/// Here rather than beside the argument parser so the table below and the
/// routing cannot disagree -- which they did, silently, the moment a noun was
/// added to one of them.
pub const NOUNS: &[&str] = &[
    "workspace",
    "pane",
    "layout",
    "agent",
    "session",
    "worktree",
    "tab",
];

/// Every command, with what it takes. Kept beside the handlers so the two
/// cannot drift without somebody noticing.
pub const COMMANDS: &[(&str, &str)] = &[
    ("workspace.list", ""),
    ("workspace.focus", "<workspace|pane>"),
    ("workspace.create", "[path]"),
    ("workspace.rename", "<workspace> <name...>"),
    ("workspace.close", "<workspace>"),
    ("pane.list", "[workspace]"),
    ("pane.focus", "<pane>"),
    ("pane.split", "<pane> [cols|rows]"),
    ("pane.read", "<pane> [lines]"),
    ("pane.send-keys", "<pane> <text...>"),
    ("pane.close", "<pane>"),
    ("layout.list", ""),
    ("layout.open", "<name>"),
    ("tab.list", "[workspace]"),
    ("tab.new", "[workspace]"),
    ("tab.focus", "<tab>"),
    ("tab.rename", "<tab> <name...>"),
    ("tab.close", "<tab>"),
    ("worktree.list", ""),
    ("worktree.add", "<branch>"),
    ("worktree.remove", "<branch|path> [--force]"),
    ("agent.list", ""),
    (
        "agent.state",
        "<blocked|working|done|idle|starting> [workspace|pane]",
    ),
    ("agent.start", "<kind> [pane]"),
    ("agent.hooks", "<kind>"),
    ("session.info", ""),
    ("session.commands", ""),
    ("session.reload", ""),
    ("session.quit", ""),
];

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
    fn the_listed_commands_are_the_ones_that_exist() {
        // A surface that says it accepts something it does not is worse than
        // one that says nothing.
        let session = None::<()>;
        let _ = session;
        for (name, _) in COMMANDS {
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
