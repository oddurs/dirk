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

//! Putting text somewhere you can paste it from.
//!
//! Two ways, and both are tried, because neither works everywhere.
//!
//! A **command** — `pbcopy`, `wl-copy`, `xclip` — reaches the clipboard of the
//! machine it runs on. That is the right one when dirk and your keyboard are on
//! the same computer, and useless when they are not.
//!
//! **OSC 52** asks the *terminal* to do it, so it crosses ssh for free: the
//! bytes go up the same stream the frames come down, and the terminal at the
//! other end is the one holding the clipboard. Not every terminal answers it —
//! Terminal.app does not, most others do — which is why it is not the only way.
//!
//! Both together cost one subprocess and about eighty bytes, and between them
//! something almost always works.

use std::io::Write;
use std::process::{Command, Stdio};

/// Run the configured command, if there is one that exists.
///
/// Never reports failure: there is nothing useful to say to somebody who has no
/// clipboard tool installed, and the escape sequence may well have worked
/// anyway.
pub fn put(command: &[String], text: &str) {
    let argv = pick(command);
    let Some((program, args)) = argv.split_first() else {
        return;
    };
    let (program, args) = (program.clone(), args.to_vec());
    let text = text.to_string();
    std::thread::spawn(move || {
        let Ok(mut child) = Command::new(&program)
            .args(&args)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        let _ = child.wait();
    });
}

/// The configured command, or the one this platform is likely to have.
fn pick(command: &[String]) -> Vec<String> {
    if !command.is_empty() {
        return command.to_vec();
    }
    shipped()
}

#[cfg(target_os = "macos")]
fn shipped() -> Vec<String> {
    vec!["pbcopy".into()]
}

/// Wayland first, then X. Neither is guaranteed and a missing one costs a
/// failed spawn, which is already the quiet case.
#[cfg(not(target_os = "macos"))]
fn shipped() -> Vec<String> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return vec!["wl-copy".into()];
    }
    vec!["xclip".into(), "-selection".into(), "clipboard".into()]
}

/// The escape sequence that asks the terminal to hold something.
///
/// Base64 because the sequence is text and a selection is not; `c` because that
/// is the clipboard rather than the primary selection, which is the one people
/// mean by "copy".
pub fn osc52(text: &str) -> Vec<u8> {
    let mut out = b"\x1b]52;c;".to_vec();
    out.extend_from_slice(base64(text.as_bytes()).as_bytes());
    out.extend_from_slice(b"\x07");
    out
}

/// Base64, written out rather than depended on.
///
/// Forty lines against a crate and a transitive tree, for the places dirk
/// needs it: here, and the handoff note, which carries a pane's screen through
/// a channel of JSON.
pub fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            // A chunk of one byte carries two characters of information and a
            // chunk of two carries three; the rest is padding.
            match i > chunk.len() {
                true => out.push('='),
                false => out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char),
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_matches_the_examples_everybody_uses() {
        // RFC 4648's own, which is what every other implementation is checked
        // against, padding included.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64_survives_bytes_that_are_not_text() {
        // A selection can hold anything a program drew, and a terminal will
        // hand back whatever it was given.
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
    }

    #[test]
    fn the_sequence_asks_for_the_clipboard_and_ends() {
        // `c` rather than `p`: the primary selection is the middle-click one,
        // and "copy" means the other.
        let seq = osc52("hi");
        assert!(seq.starts_with(b"\x1b]52;c;"));
        assert!(
            seq.ends_with(b"\x07"),
            "an unterminated sequence eats the frame after it"
        );
        assert_eq!(&seq[7..seq.len() - 1], b"aGk=");
    }
}
