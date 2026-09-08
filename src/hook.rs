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

//! Putting the hook in the file, rather than printing it and hoping.
//!
//! Everything below rank one is dirk guessing at something the agent already
//! knows, and the README has always said so — and then asked you to install it
//! by hand. Most people therefore run on rank two for ever, which means dirk
//! spends its life inferring a state its agent could simply have told it.
//!
//! The care is all in not owning a file dirk did not write. The settings file
//! belongs to somebody else, holds their other settings, and is edited by hand;
//! dirk adds two entries to it and must be able to take exactly those two back
//! out. It recognises its own by shape — a command that reports a state to dirk
//! from inside a pane — because the file is JSON and JSON has no comments to
//! leave a marker in.
//!
//! One that is dirk-shaped and not dirk's current text is somebody's edit. It
//! is reported and left alone: a silent overwrite would lose work that was done
//! deliberately, and the person who did it is exactly the person who would
//! never trust this again.

use crate::agent;
use serde_json::{Value, json};

/// What is in a settings file, for one harness.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// dirk's hooks are there and say what dirk would write now.
    Installed,
    /// Nothing of dirk's is in the file.
    Absent,
    /// Something dirk-shaped is there and is not what dirk writes.
    Edited,
}

impl State {
    pub fn name(self) -> &'static str {
        match self {
            State::Installed => "installed",
            State::Absent => "absent",
            State::Edited => "edited",
        }
    }
}

/// The settings file for a harness, under the user's home directory.
pub fn path(kind: &str) -> Option<std::path::PathBuf> {
    agent::hookable(kind).map(|h| crate::config::home().join(h.path))
}

/// What the file says about this harness.
///
/// A file that does not exist, or does not parse, is `Absent` rather than an
/// error: neither is a reason to refuse to say what dirk would do about it, and
/// installing will say plainly if it cannot read it.
pub fn state(kind: &str) -> State {
    let Some(hooks) = agent::hookable(kind) else {
        return State::Absent;
    };
    let Some(doc) = read(kind) else {
        return State::Absent;
    };
    let mut found = 0;
    let mut current = 0;
    for (event, state) in [(hooks.done, "done"), (hooks.blocked, "blocked")] {
        let want = agent::command(state);
        for command in commands(&doc, event) {
            if !agent::ours(&command) {
                continue;
            }
            found += 1;
            if command == want {
                current += 1;
            }
        }
    }
    match (found, current) {
        (0, _) => State::Absent,
        (f, c) if f == c && c == 2 => State::Installed,
        _ => State::Edited,
    }
}

/// Put dirk's two hooks in, or say why not.
///
/// Installing twice is a no-op, because the entries are matched by their text
/// and the text is what would be written.
pub fn install(kind: &str) -> Result<String, String> {
    let Some(hooks) = agent::hookable(kind) else {
        return Err(format!(
            "there is no snippet for {kind} yet; `dirk agent hooks {kind}` says what to write"
        ));
    };
    let path = path(kind).ok_or("no settings file for that harness")?;
    match state(kind) {
        State::Installed => Ok(format!("already installed in {}", path.display())),
        // Not overwritten. Somebody changed this on purpose, and a silent
        // replacement would lose that and their trust with it.
        State::Edited => Err(format!(
            "{} holds a dirk hook that is not the one dirk writes; \
             remove it or run `dirk agent hooks uninstall {kind}` first",
            path.display()
        )),
        State::Absent => {
            let mut doc = read(kind).unwrap_or_else(|| json!({}));
            for (event, state) in [(hooks.done, "done"), (hooks.blocked, "blocked")] {
                add(&mut doc, event, &agent::command(state));
            }
            write(&path, &doc)?;
            Ok(format!("installed in {}", path.display()))
        }
    }
}

/// Take dirk's two hooks out and leave everything else alone.
pub fn uninstall(kind: &str) -> Result<String, String> {
    let Some(hooks) = agent::hookable(kind) else {
        return Err(format!("dirk installs no hooks for {kind}"));
    };
    let path = path(kind).ok_or("no settings file for that harness")?;
    let Some(mut doc) = read(kind) else {
        return Ok(format!("nothing of dirk's in {}", path.display()));
    };
    let mut gone = 0;
    for event in [hooks.done, hooks.blocked] {
        gone += remove(&mut doc, event);
    }
    if gone == 0 {
        return Ok(format!("nothing of dirk's in {}", path.display()));
    }
    tidy(&mut doc, &[hooks.done, hooks.blocked]);
    write(&path, &doc)?;
    Ok(format!("removed {gone} from {}", path.display()))
}

fn read(kind: &str) -> Option<Value> {
    let path = path(kind)?;
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Written whole, and atomically.
///
/// serde_json is built here with `preserve_order`, so the keys somebody else
/// wrote come back in the order they wrote them. The indentation becomes dirk's
/// rather than theirs, which is the one thing this cannot preserve and is said
/// so in the manual.
fn write(path: &std::path::Path, doc: &Value) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(doc).map_err(|e| e.to_string())?;
    // Beside the file and then renamed: a settings file truncated by a crash
    // halfway through a write is a harness that will not start.
    let temp = path.with_extension("dirk-tmp");
    std::fs::write(&temp, format!("{text}\n")).map_err(|e| format!("{}: {e}", temp.display()))?;
    std::fs::rename(&temp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Every command string under `hooks.<event>`, however deeply the harness nests
/// them.
fn commands(doc: &Value, event: &str) -> Vec<String> {
    let mut out = Vec::new();
    collect(doc.get("hooks").and_then(|h| h.get(event)), &mut out);
    out
}

fn collect(value: Option<&Value>, out: &mut Vec<String>) {
    match value {
        Some(Value::Array(items)) => items.iter().for_each(|v| collect(Some(v), out)),
        Some(Value::Object(map)) => {
            if let Some(Value::String(c)) = map.get("command") {
                out.push(c.clone());
            }
            map.values().for_each(|v| collect(Some(v), out));
        }
        _ => {}
    }
}

/// Add one matcher group carrying dirk's command.
fn add(doc: &mut Value, event: &str, command: &str) {
    let entry = json!({ "hooks": [{ "type": "command", "command": command }] });
    let root = doc.as_object_mut();
    let Some(root) = root else { return };
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    let Some(hooks) = hooks else { return };
    // Something else under that key is left exactly as it is: this is
    // somebody's settings file and dirk is a guest in it.
    if let Value::Array(list) = hooks.entry(event).or_insert_with(|| json!([])) {
        list.push(entry);
    }
}

/// Take out every group whose only command is one of dirk's.
///
/// A group that also holds somebody else's command is left alone: removing
/// their hook because it shares a matcher with ours is the kind of damage that
/// is noticed weeks later.
fn remove(doc: &mut Value, event: &str) -> usize {
    let Some(list) = doc
        .get_mut("hooks")
        .and_then(|h| h.get_mut(event))
        .and_then(|e| e.as_array_mut())
    else {
        return 0;
    };
    let before = list.len();
    list.retain(|group| {
        let mut found = Vec::new();
        collect(Some(group), &mut found);
        !(!found.is_empty() && found.iter().all(|c| agent::ours(c)))
    });
    before - list.len()
}

/// Drop the keys that are empty because dirk emptied them.
///
/// Uninstalling should leave the file as close to how it was found as it can.
/// An empty `"Stop": []` where there was nothing before is not damage, but it
/// is litter, and litter in somebody's settings file is how a tool gets a
/// reputation.
fn tidy(doc: &mut Value, events: &[&str]) {
    let Some(hooks) = doc.get_mut("hooks").and_then(|h| h.as_object_mut()) else {
        return;
    };
    for event in events {
        if hooks
            .get(*event)
            .and_then(|v| v.as_array())
            .is_some_and(Vec::is_empty)
        {
            hooks.remove(*event);
        }
    }
    if hooks.is_empty()
        && let Some(root) = doc.as_object_mut()
    {
        root.remove("hooks");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirks_own_commands_are_recognised_by_shape() {
        assert!(agent::ours(&agent::command("done")));
        assert!(agent::ours(&agent::command("blocked")));
        assert!(!agent::ours("npm test"));
        // Somebody else reporting to dirk from outside a pane is not ours: the
        // guard is the shape.
        assert!(!agent::ours("dirk agent state done w1"));
    }

    #[test]
    fn adding_and_removing_leaves_the_rest_of_the_file_alone() {
        let mut doc = json!({
            "model": "opus",
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": "npm test" }] }] },
        });
        add(&mut doc, "Stop", &agent::command("done"));
        assert_eq!(commands(&doc, "Stop").len(), 2);

        assert_eq!(remove(&mut doc, "Stop"), 1);
        assert_eq!(commands(&doc, "Stop"), vec!["npm test".to_string()]);
        // Everything that was not ours is untouched, including the key order.
        assert_eq!(doc.get("model"), Some(&json!("opus")));
        assert_eq!(
            doc.as_object().unwrap().keys().collect::<Vec<_>>(),
            vec!["model", "hooks"]
        );
    }

    #[test]
    fn uninstalling_leaves_no_litter_where_there_was_none() {
        let mut doc = json!({
            "model": "opus",
            "hooks": { "Stop": [{ "hooks": [{ "type": "command", "command": agent::command("done") }] }] },
        });
        assert_eq!(remove(&mut doc, "Stop"), 1);
        tidy(&mut doc, &["Stop", "Notification"]);
        assert_eq!(doc, json!({ "model": "opus" }));
    }

    #[test]
    fn a_group_shared_with_somebody_elses_hook_is_not_removed() {
        // Removing their hook because it shares a matcher with ours is the kind
        // of damage that is noticed weeks later.
        let mut doc = json!({
            "hooks": { "Stop": [{ "hooks": [
                { "type": "command", "command": agent::command("done") },
                { "type": "command", "command": "npm test" },
            ] }] },
        });
        assert_eq!(remove(&mut doc, "Stop"), 0);
        assert_eq!(commands(&doc, "Stop").len(), 2);
    }
}
