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

//! Turning a crossterm key back into the bytes a terminal would have sent.
//!
//! dirk reads keys through crossterm, which has already parsed the escape
//! sequences the terminal sent it. The program inside a pane expects those
//! sequences, so they have to be reassembled. This is the inverse of a parser
//! and it is as dull as that sounds — but it is where a multiplexer feels
//! broken if it is wrong, because it is every keystroke.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// `app_cursor` is DECCKM: in application cursor mode the arrows are `ESC O A`
/// rather than `ESC [ A`, which is what makes readline and vim behave.
pub fn encode(k: KeyEvent, app_cursor: bool) -> Option<Vec<u8>> {
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    let alt = k.modifiers.contains(KeyModifiers::ALT);

    let mut out: Vec<u8> = match k.code {
        KeyCode::Char(c) if ctrl => vec![control(c)?],
        KeyCode::Char(c) => c.to_string().into_bytes(),
        KeyCode::Enter => vec![b'\r'],
        KeyCode::Tab => vec![b'\t'],
        KeyCode::BackTab => b"\x1b[Z".to_vec(),
        KeyCode::Backspace => vec![0x7f],
        KeyCode::Esc => vec![0x1b],
        KeyCode::Up => cursor(b'A', app_cursor),
        KeyCode::Down => cursor(b'B', app_cursor),
        KeyCode::Right => cursor(b'C', app_cursor),
        KeyCode::Left => cursor(b'D', app_cursor),
        KeyCode::Home => cursor(b'H', app_cursor),
        KeyCode::End => cursor(b'F', app_cursor),
        KeyCode::Insert => b"\x1b[2~".to_vec(),
        KeyCode::Delete => b"\x1b[3~".to_vec(),
        KeyCode::PageUp => b"\x1b[5~".to_vec(),
        KeyCode::PageDown => b"\x1b[6~".to_vec(),
        KeyCode::F(n) => function(n)?,
        _ => return None,
    };

    // Alt is a leading escape. Terminals have done this since the VT220 and
    // every shell still reads it that way.
    if alt {
        out.insert(0, 0x1b);
    }
    Some(out)
}

fn control(c: char) -> Option<u8> {
    Some(match c.to_ascii_lowercase() {
        c @ 'a'..='z' => c as u8 - b'a' + 1,
        ' ' | '@' => 0,
        '[' => 27,
        '\\' => 28,
        ']' => 29,
        '^' => 30,
        '_' | '/' => 31,
        '?' => 127,
        _ => return None,
    })
}

fn cursor(final_byte: u8, app: bool) -> Vec<u8> {
    vec![0x1b, if app { b'O' } else { b'[' }, final_byte]
}

fn function(n: u8) -> Option<Vec<u8>> {
    Some(match n {
        1..=4 => vec![0x1b, b'O', b'P' + (n - 1)],
        5 => b"\x1b[15~".to_vec(),
        6..=9 => format!("\x1b[{}~", n + 11).into_bytes(),
        10 => b"\x1b[21~".to_vec(),
        11 => b"\x1b[23~".to_vec(),
        12 => b"\x1b[24~".to_vec(),
        _ => return None,
    })
}

/// Read a key by the name a person would write: `esc`, `up`, `ctrl+c`.
///
/// The inverse of what a keymap does, for callers who have no keyboard. A
/// program driving an agent's interface has to be able to say *escape* and
/// *ctrl-c* — and it cannot say them as text, because the whole difference
/// between text and a key is that one of them is not characters.
///
/// Names are lowercase and modifiers are `+`-joined, in any order. `escape` is
/// accepted for `esc` because half the world writes it that way and being right
/// about which half is not worth an error message.
pub fn named(text: &str) -> Option<KeyEvent> {
    let mut mods = KeyModifiers::NONE;
    let lower = text.to_ascii_lowercase();
    // Split at the *last* separator, and read a trailing one as the key
    // itself: `+` is a plus sign and `ctrl++` is control and a plus sign.
    // Splitting left to right makes both of those an empty key name.
    let (head, key) = match lower.rsplit_once('+') {
        Some((head, "")) => (head, "+"),
        Some((head, tail)) => (head, tail),
        None => ("", lower.as_str()),
    };
    for m in head.split('+').filter(|m| !m.is_empty()) {
        mods |= match m {
            "ctrl" | "control" => KeyModifiers::CONTROL,
            "alt" | "meta" | "option" => KeyModifiers::ALT,
            "shift" => KeyModifiers::SHIFT,
            _ => return None,
        };
    }

    let code = match key {
        "enter" | "return" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "backspace" | "bs" => KeyCode::Backspace,
        "delete" | "del" => KeyCode::Delete,
        "insert" | "ins" => KeyCode::Insert,
        "space" => KeyCode::Char(' '),
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" | "pgup" => KeyCode::PageUp,
        "pagedown" | "pgdn" => KeyCode::PageDown,
        f if f.len() >= 2 && f.starts_with('f') && f[1..].chars().all(|c| c.is_ascii_digit()) => {
            KeyCode::F(f[1..].parse().ok()?)
        }
        // One character is that character. Anything longer is a name dirk does
        // not know, and guessing that it was meant literally is how a caller
        // that typed `escpae` ends up with those six letters in its agent.
        other => {
            let mut chars = other.chars();
            let c = chars.next()?;
            chars.next().is_none().then_some(KeyCode::Char(c))?
        }
    };
    Some(KeyEvent::new(code, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_is_read_by_the_name_a_person_would_write() {
        assert_eq!(named("esc"), Some(KeyEvent::from(KeyCode::Esc)));
        assert_eq!(named("escape"), Some(KeyEvent::from(KeyCode::Esc)));
        assert_eq!(named("ENTER"), Some(KeyEvent::from(KeyCode::Enter)));
        assert_eq!(named("f5"), Some(KeyEvent::from(KeyCode::F(5))));
        assert_eq!(named("space"), Some(KeyEvent::from(KeyCode::Char(' '))));
        assert_eq!(
            named("ctrl+c"),
            Some(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL))
        );
        // Order is not significant: nobody remembers which one goes first.
        assert_eq!(named("shift+alt+up"), named("alt+shift+up"));
    }

    #[test]
    fn a_name_dirk_does_not_know_is_refused_rather_than_typed() {
        // The failure this exists to prevent: a caller that meant Escape and
        // wrote it wrong should get an error, not those six letters typed into
        // whatever the pane is running.
        assert_eq!(named("escpae"), None);
        assert_eq!(named("hyper+x"), None);
        assert_eq!(named(""), None);
        // One character is that character, which is how `ctrl+c` has a key at
        // all -- but two are a name, and there is no such name.
        assert_eq!(named("x"), Some(KeyEvent::from(KeyCode::Char('x'))));
        assert_eq!(named("xy"), None);
    }

    #[test]
    fn the_plus_that_joins_modifiers_can_also_be_the_key() {
        assert_eq!(named("+"), Some(KeyEvent::from(KeyCode::Char('+'))));
        assert_eq!(
            named("ctrl++"),
            Some(KeyEvent::new(KeyCode::Char('+'), KeyModifiers::CONTROL))
        );
    }

    #[test]
    fn an_arrow_comes_out_as_whatever_the_program_asked_for() {
        // DECCKM. In application cursor mode the arrows are ESC O A rather than
        // ESC [ A, and a caller sending the wrong one moves a menu selection in
        // some programs and not in others -- which is exactly the kind of bug
        // that gets blamed on the agent.
        let up = named("up").expect("up");
        assert_eq!(encode(up, false), Some(b"\x1b[A".to_vec()));
        assert_eq!(encode(up, true), Some(b"\x1bOA".to_vec()));
    }
}
