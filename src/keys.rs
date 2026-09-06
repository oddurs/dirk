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
