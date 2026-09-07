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

//! Telling you about an agent when you are not looking at dirk.
//!
//! The point of running several agents is not watching them, and attention in
//! the nav only helps while dirk is the window in front of you.
//!
//! No dependency for this. Each platform has one command that does it, and
//! shelling out to that is a dozen lines against a crate, a build-time cost and
//! a transitive tree — for something that is allowed to silently do nothing.

use std::process::{Command, Stdio};

/// Send one, on a thread of its own.
///
/// Never blocks and never reports failure. `osascript` can take a noticeable
/// moment, and a notification is the least important thing dirk does — it must
/// not be able to hold up a redraw, and there is nothing useful to say if the
/// platform has no notifier.
pub fn send(title: String, body: String) {
    std::thread::spawn(move || {
        let _ = deliver(&title, &body);
    });
}

#[cfg(target_os = "macos")]
fn deliver(title: &str, body: &str) -> std::io::Result<()> {
    // Through AppleScript, which is the only route that does not need an app
    // bundle of dirk's own. Quotes are escaped because this is a script, not
    // an argument vector.
    let script = format!(
        "display notification {} with title {}",
        quote(body),
        quote(title)
    );
    Command::new("osascript")
        .args(["-e", &script])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

#[cfg(target_os = "macos")]
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

#[cfg(not(target_os = "macos"))]
fn deliver(title: &str, body: &str) -> std::io::Result<()> {
    // Arguments rather than a script, so nothing needs escaping.
    Command::new("notify-send")
        .args(["--app-name=dirk", title, body])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    #[test]
    fn a_body_cannot_escape_its_string_and_become_script() {
        // The body is a workspace label, which is a terminal title, which is
        // whatever a program in a pane decided to print.
        let hostile = "\" & (do shell script \"touch /tmp/pwned\") & \"";
        let quoted = super::quote(hostile);
        assert!(quoted.starts_with('"') && quoted.ends_with('"'));
        // Every inner quote is escaped, so the string is never left.
        let inner = &quoted[1..quoted.len() - 1];
        for (i, c) in inner.char_indices() {
            if c == '"' {
                assert_eq!(&inner[i - 1..i], "\\", "an unescaped quote at {i}");
            }
        }
    }
}
