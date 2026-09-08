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

//! What an agent needs to know to drive dirk.
//!
//! Generated from the command table rather than written beside it, so it cannot
//! describe a surface that no longer exists. A README is the wrong shape for
//! this: it is written for someone deciding whether to use dirk, and this is
//! for something that has already been told to.
//!
//! The traps are most of the value. Everything here that is not a command is
//! something that would otherwise be discovered by getting it wrong.

/// The whole skill, as text.
pub fn text() -> String {
    let mut out = String::new();
    out.push_str(HEAD);

    out.push_str("\n## Commands\n\n```\n");
    for (name, args) in crate::api::COMMANDS {
        let (noun, verb) = name.split_once('.').unwrap_or((name, ""));
        // Trimmed: most commands take nothing, and a line ending in a space is
        // a line somebody will copy with the space in it.
        out.push_str(
            format!("dirk {noun} {verb} {args}\n")
                .trim_start()
                .trim_end(),
        );
        out.push('\n');
    }
    out.push_str("```\n");

    out.push_str(TAIL);
    out
}

const HEAD: &str = r#"---
name: dirk
description: "Control dirk, a terminal multiplexer for coding agents. Use only when the task is about inspecting or controlling panes, workspaces or other agents in a dirk session. Requires DIRK=1."
---

# dirk

dirk organises terminals into projects, workspaces and panes, recognises coding
agents running inside them, and answers for itself over a socket.

Before issuing any command, check that you are inside a dirk pane:

```bash
test "${DIRK:-}" = 1
```

If that fails, say you are not running inside dirk and stop. Do not drive
somebody else's session from outside it.

Every answer is JSON, including the failures — the exit status is 0 or 1 and the
body carries `ok` and `error`. Read ids out of answers rather than guessing
them.
"#;

const TAIL: &str = r#"
## Ids

`w7` is a workspace. `w7:p12` is a pane in it. A pane id is unique on its own,
so `p12` works too — the workspace half is there to be read.

The number the nav shows beside a workspace is **not** an id. It is positional
and changes when spaces are reordered; `n` is for a human glancing at a column.

`--current` is the pane you are in, resolved from `DIRK_PANE_ID`. Prefer it to
looking your own id up.

## Things that are true and would otherwise be found out the hard way

**Reading does not mark an agent seen.** dirk separates `done` — finished work
nobody has looked at — from `idle` by whether a human has focused the workspace.
Asking about a workspace is not looking at one, so `workspace list` will not
clear a `done`. That is deliberate: a status line that polled the session would
otherwise erase the very notifications it exists to show.

**A workspace holding more than one pane is not renamed.** Two panes have no
single intent. If you split a workspace, its name stops tracking the work.

**`state` is about the agent, not the pane.** `blocked` means dirk recognised an
approval prompt on screen and a human is being waited for. `working` means
output recently. `done` means stopped and unseen; `idle` means stopped and seen.

**A pane you did not create may be a human's.** `pane run` types into it.
Check `available` before starting anything in a pane: it is true only for a
shell sitting at its prompt.

**`pane read` returns the screen, not the scrollback**, and it is anchored to
the cursor rather than to the bottom of the grid.

## Starting work beside yourself

Split your own pane and work there, rather than creating a workspace — a
workspace is a unit of work a human organised, and taking one over is rude.

```bash
dirk pane split --current rows
```

The answer names the new pane. It holds a shell at a prompt.
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_skill_describes_the_surface_that_exists() {
        // Generated rather than written beside the code, so it cannot drift.
        let text = text();
        for (name, _) in crate::api::COMMANDS {
            let (noun, verb) = name.split_once('.').unwrap();
            assert!(
                text.contains(&format!("dirk {noun} {verb}")),
                "{name} is not in the skill"
            );
        }
    }

    #[test]
    fn it_says_how_to_check_it_is_being_used_in_the_right_place() {
        let text = text();
        assert!(text.contains("DIRK:-"), "no environment check");
        assert!(text.contains("DIRK_PANE_ID"), "nothing about --current");
    }

    #[test]
    fn it_carries_the_rules_that_are_not_guessable() {
        // The traps are most of the value: each of these is something that
        // would otherwise be discovered by getting it wrong.
        let text = text();
        for rule in [
            "not mark an agent seen",
            "more than one pane is not renamed",
            "available",
        ] {
            assert!(text.contains(rule), "the skill does not mention: {rule}");
        }
    }
}
