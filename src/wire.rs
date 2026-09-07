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

//! What a client and a server say to each other.
//!
//! A message is a length, a kind, and a payload. Control messages are JSON
//! because they are rare and worth reading in a log; frames are raw bytes
//! because they are escape sequences and a line-delimited format cannot carry
//! them without escaping everything twice, or base64 without inflating every
//! frame by a third for nothing.
//!
//! The client parses terminal input, because that is where the terminal is, and
//! sends events across as [`Input`]. They are turned back into crossterm events
//! on the far side, so the event loop is unaware any of this is happening.

use crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

/// The largest message anyone may send.
///
/// A frame of a very large terminal is tens of kilobytes; anything past this is
/// a bug or something that is not dirk on the other end, and reading it into
/// memory is how that becomes a crash.
const MAX_MESSAGE: u32 = 8 << 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// Client says hello, with the size of its terminal.
    Hello = 1,
    /// Client sends one terminal event.
    Input = 2,
    /// Server sends escape sequences to paint.
    Frame = 3,
    /// Server says why it is ending the connection.
    Bye = 4,
    /// A caller asks the session to do or report something.
    Command = 5,
    /// The answer to one command.
    Reply = 6,
}

impl Kind {
    fn from(b: u8) -> Option<Self> {
        Some(match b {
            1 => Kind::Hello,
            2 => Kind::Input,
            3 => Kind::Frame,
            4 => Kind::Bye,
            5 => Kind::Command,
            6 => Kind::Reply,
            _ => return None,
        })
    }
}

/// One thing asked of a session.
///
/// A name and a list of words rather than a type per command: the surface is
/// meant to grow, and every caller on the other side is a shell or an agent
/// composing strings. A typed tree would be a nicer thing to hold and a worse
/// thing to call.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub cmd: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// The answer.
///
/// Structured, including the failure. A caller is a program, and prose on
/// stderr is not something a program can branch on.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub result: serde_json::Value,
}

impl Reply {
    pub fn ok(result: serde_json::Value) -> Self {
        Self {
            ok: true,
            error: None,
            result,
        }
    }
    pub fn err(why: impl Into<String>) -> Self {
        Self {
            ok: false,
            error: Some(why.into()),
            result: serde_json::Value::Null,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Hello {
    pub cols: u16,
    pub rows: u16,
}

/// One terminal event, as the wire sees it.
///
/// Its own type rather than crossterm's: that one is not serialisable, and
/// pinning the shape here means a client and a server of different versions
/// disagree loudly rather than by misreading a field.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Input {
    Key {
        code: String,
        mods: u8,
        release: bool,
    },
    Mouse {
        kind: String,
        button: u8,
        col: u16,
        row: u16,
        mods: u8,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Paste(String),
}

impl Input {
    pub fn from_event(ev: &Event) -> Option<Self> {
        Some(match ev {
            Event::Key(k) => Input::Key {
                code: key_name(k.code)?,
                mods: k.modifiers.bits(),
                release: k.kind == KeyEventKind::Release,
            },
            Event::Mouse(m) => {
                let (kind, button) = mouse_name(m.kind);
                Input::Mouse {
                    kind,
                    button,
                    col: m.column,
                    row: m.row,
                    mods: m.modifiers.bits(),
                }
            }
            Event::Resize(cols, rows) => Input::Resize {
                cols: *cols,
                rows: *rows,
            },
            Event::Paste(text) => Input::Paste(text.clone()),
            _ => return None,
        })
    }

    pub fn into_event(self) -> Option<Event> {
        Some(match self {
            Input::Key {
                code,
                mods,
                release,
            } => Event::Key(KeyEvent {
                code: key_code(&code)?,
                modifiers: KeyModifiers::from_bits_truncate(mods),
                kind: if release {
                    KeyEventKind::Release
                } else {
                    KeyEventKind::Press
                },
                state: KeyEventState::NONE,
            }),
            Input::Mouse {
                kind,
                button,
                col,
                row,
                mods,
            } => Event::Mouse(MouseEvent {
                kind: mouse_kind(&kind, button)?,
                column: col,
                row,
                modifiers: KeyModifiers::from_bits_truncate(mods),
            }),
            Input::Resize { cols, rows } => Event::Resize(cols, rows),
            Input::Paste(text) => Event::Paste(text),
        })
    }
}

/// Key codes as names. Spelled out rather than numbered so that a mismatch
/// between two versions is a key that does nothing rather than a key that does
/// something else.
fn key_name(code: KeyCode) -> Option<String> {
    Some(match code {
        KeyCode::Char(c) => format!("c:{c}"),
        KeyCode::F(n) => format!("f:{n}"),
        KeyCode::Backspace => "backspace".into(),
        KeyCode::Enter => "enter".into(),
        KeyCode::Left => "left".into(),
        KeyCode::Right => "right".into(),
        KeyCode::Up => "up".into(),
        KeyCode::Down => "down".into(),
        KeyCode::Home => "home".into(),
        KeyCode::End => "end".into(),
        KeyCode::PageUp => "pageup".into(),
        KeyCode::PageDown => "pagedown".into(),
        KeyCode::Tab => "tab".into(),
        KeyCode::BackTab => "backtab".into(),
        KeyCode::Delete => "delete".into(),
        KeyCode::Insert => "insert".into(),
        KeyCode::Esc => "esc".into(),
        _ => return None,
    })
}

fn key_code(name: &str) -> Option<KeyCode> {
    if let Some(c) = name.strip_prefix("c:") {
        return c.chars().next().map(KeyCode::Char);
    }
    if let Some(n) = name.strip_prefix("f:") {
        return n.parse().ok().map(KeyCode::F);
    }
    Some(match name {
        "backspace" => KeyCode::Backspace,
        "enter" => KeyCode::Enter,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "tab" => KeyCode::Tab,
        "backtab" => KeyCode::BackTab,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        "esc" => KeyCode::Esc,
        _ => return None,
    })
}

fn mouse_name(kind: MouseEventKind) -> (String, u8) {
    let b = |b: MouseButton| match b {
        MouseButton::Left => 0,
        MouseButton::Middle => 1,
        MouseButton::Right => 2,
    };
    match kind {
        MouseEventKind::Down(x) => ("down".into(), b(x)),
        MouseEventKind::Up(x) => ("up".into(), b(x)),
        MouseEventKind::Drag(x) => ("drag".into(), b(x)),
        MouseEventKind::Moved => ("moved".into(), 0),
        MouseEventKind::ScrollUp => ("scrollup".into(), 0),
        MouseEventKind::ScrollDown => ("scrolldown".into(), 0),
        MouseEventKind::ScrollLeft => ("scrollleft".into(), 0),
        MouseEventKind::ScrollRight => ("scrollright".into(), 0),
    }
}

fn mouse_kind(name: &str, button: u8) -> Option<MouseEventKind> {
    let b = match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        _ => MouseButton::Right,
    };
    Some(match name {
        "down" => MouseEventKind::Down(b),
        "up" => MouseEventKind::Up(b),
        "drag" => MouseEventKind::Drag(b),
        "moved" => MouseEventKind::Moved,
        "scrollup" => MouseEventKind::ScrollUp,
        "scrolldown" => MouseEventKind::ScrollDown,
        "scrollleft" => MouseEventKind::ScrollLeft,
        "scrollright" => MouseEventKind::ScrollRight,
        _ => return None,
    })
}

// ── Framing ─────────────────────────────────────────────────────────────

pub fn send(w: &mut impl Write, kind: Kind, payload: &[u8]) -> io::Result<()> {
    let len = u32::try_from(payload.len())
        .ok()
        .filter(|n| *n <= MAX_MESSAGE)
        .ok_or_else(|| io::Error::other("message too large to send"))?;
    w.write_all(&len.to_be_bytes())?;
    w.write_all(&[kind as u8])?;
    w.write_all(payload)?;
    w.flush()
}

pub fn send_json<T: Serialize>(w: &mut impl Write, kind: Kind, value: &T) -> io::Result<()> {
    let body = serde_json::to_vec(value).map_err(io::Error::other)?;
    send(w, kind, &body)
}

/// Read one message, or `None` at a clean end of stream.
pub fn recv(r: &mut impl Read) -> io::Result<Option<(Kind, Vec<u8>)>> {
    let mut header = [0u8; 5];
    match r.read_exact(&mut header) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let len = u32::from_be_bytes([header[0], header[1], header[2], header[3]]);
    if len > MAX_MESSAGE {
        // Refused rather than allocated: the length is the first thing anyone
        // on the other end controls.
        return Err(io::Error::other("message too large to read"));
    }
    let kind = Kind::from(header[4]).ok_or_else(|| io::Error::other("unknown message kind"))?;
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    Ok(Some((kind, body)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(ev: Event) -> Event {
        let wire = Input::from_event(&ev).expect("representable");
        let back = wire.clone().into_event().expect("reversible");
        // And it survives the wire itself.
        let json = serde_json::to_vec(&wire).unwrap();
        let parsed: Input = serde_json::from_slice(&json).unwrap();
        assert_eq!(parsed, wire, "the encoding is not stable");
        back
    }

    #[test]
    fn every_key_the_multiplexer_binds_survives_the_wire() {
        // These are the keys the prefix and the pane paths depend on. One that
        // does not round-trip is a key that silently stops working when
        // detached.
        for code in [
            KeyCode::Char(' '),
            KeyCode::Char('q'),
            KeyCode::Enter,
            KeyCode::Esc,
            KeyCode::Tab,
            KeyCode::BackTab,
            KeyCode::Up,
            KeyCode::Down,
            KeyCode::Left,
            KeyCode::Right,
            KeyCode::Backspace,
            KeyCode::Delete,
            KeyCode::F(5),
        ] {
            let ev = Event::Key(KeyEvent::new(code, KeyModifiers::CONTROL));
            assert_eq!(roundtrip(ev.clone()), ev, "{code:?}");
        }
    }

    #[test]
    fn modifiers_survive_because_ctrl_space_is_the_prefix() {
        let ev = Event::Key(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::CONTROL));
        let Event::Key(k) = roundtrip(ev) else {
            panic!("not a key")
        };
        assert!(k.modifiers.contains(KeyModifiers::CONTROL));
    }

    #[test]
    fn a_click_arrives_at_the_cell_it_was_made_in() {
        let ev = Event::Mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 37,
            row: 9,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(roundtrip(ev.clone()), ev);
    }

    #[test]
    fn every_mouse_kind_round_trips() {
        for kind in [
            MouseEventKind::Down(MouseButton::Right),
            MouseEventKind::Up(MouseButton::Middle),
            MouseEventKind::Drag(MouseButton::Left),
            MouseEventKind::Moved,
            MouseEventKind::ScrollUp,
            MouseEventKind::ScrollDown,
        ] {
            let ev = Event::Mouse(MouseEvent {
                kind,
                column: 1,
                row: 2,
                modifiers: KeyModifiers::NONE,
            });
            assert_eq!(roundtrip(ev.clone()), ev, "{kind:?}");
        }
    }

    #[test]
    fn a_message_round_trips_through_the_framing() {
        let mut buf = Vec::new();
        send(&mut buf, Kind::Frame, b"\x1b[2J\x1b[H").unwrap();
        send_json(&mut buf, Kind::Hello, &Hello { cols: 80, rows: 24 }).unwrap();

        let mut cursor = std::io::Cursor::new(buf);
        let (kind, body) = recv(&mut cursor).unwrap().unwrap();
        assert_eq!(kind, Kind::Frame);
        assert_eq!(
            body, b"\x1b[2J\x1b[H",
            "escape sequences pass through untouched"
        );

        let (kind, body) = recv(&mut cursor).unwrap().unwrap();
        assert_eq!(kind, Kind::Hello);
        let hello: Hello = serde_json::from_slice(&body).unwrap();
        assert_eq!((hello.cols, hello.rows), (80, 24));

        assert!(
            recv(&mut cursor).unwrap().is_none(),
            "a clean end is not an error"
        );
    }

    #[test]
    fn an_absurd_length_is_refused_rather_than_allocated() {
        // The length is the first thing the other end controls, and believing
        // it is how a protocol becomes a way to exhaust memory.
        let mut framed = Vec::new();
        framed.extend_from_slice(&u32::MAX.to_be_bytes());
        framed.push(Kind::Frame as u8);
        let err = recv(&mut std::io::Cursor::new(framed)).unwrap_err();
        assert!(err.to_string().contains("too large"));
    }

    #[test]
    fn an_unknown_kind_is_an_error_not_a_guess() {
        let mut framed = Vec::new();
        framed.extend_from_slice(&0u32.to_be_bytes());
        framed.push(200);
        assert!(recv(&mut std::io::Cursor::new(framed)).is_err());
    }

    #[test]
    fn a_truncated_message_is_an_error_rather_than_a_short_read() {
        let mut framed = Vec::new();
        framed.extend_from_slice(&10u32.to_be_bytes());
        framed.push(Kind::Frame as u8);
        framed.extend_from_slice(b"only3");
        assert!(recv(&mut std::io::Cursor::new(framed)).is_err());
    }
}
