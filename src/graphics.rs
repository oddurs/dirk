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

//! Images in a pane.
//!
//! dirk emulates the terminal in each pane, so a program that draws an image
//! draws it to dirk and it stops there. A plot, a diagram, a screenshot an
//! agent has just taken: all of them work in the terminal and stop working the
//! moment they are inside dirk. It is the largest single thing that makes a
//! dirk pane not a real terminal.
//!
//! The protocol is kitty's, which is an APC: `ESC _ G <keys> ; <payload> ESC \`.
//! vt100 discards those without a word — there is no callback for them and
//! nothing is left in the grid — so the bytes are taken out of the stream here,
//! before the parser sees them, and everything else is passed through.
//!
//! **An image is transmitted once and placed many times.** That split is the
//! whole design: the payload can be a megabyte and the placement is twenty
//! bytes, so a redraw re-places rather than re-sending, and a program that
//! shows one image in four panes sends it once.

use std::collections::HashMap;

/// The escape that opens one of these, and the two that can close it.
const OPEN: &[u8] = b"\x1b_G";

/// What a program asked dirk to do with an image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    /// The keys before the semicolon: `a=T`, `i=31`, `f=100`, and the rest.
    pub keys: HashMap<String, String>,
    /// Everything after it, still base64 as it arrived.
    pub payload: Vec<u8>,
}

impl Command {
    pub fn get(&self, k: &str) -> Option<&str> {
        self.keys.get(k).map(String::as_str)
    }

    fn number(&self, k: &str) -> Option<u32> {
        self.get(k)?.parse().ok()
    }

    /// `a=`, defaulting to transmit as the protocol does.
    pub fn action(&self) -> char {
        self.get("a").and_then(|a| a.chars().next()).unwrap_or('t')
    }

    /// The image this is about. Zero is "the one being built".
    pub fn id(&self) -> u32 {
        self.number("i").unwrap_or(0)
    }

    /// Whether more chunks of the payload are coming.
    pub fn more(&self) -> bool {
        self.get("m") == Some("1")
    }

    /// How many cells it should cover, when the program said.
    pub fn cells(&self) -> Option<(u16, u16)> {
        Some((self.number("c")? as u16, self.number("r")? as u16))
    }
}

/// Pull the graphics commands out of a stream, leaving everything else.
///
/// Stateful because an APC can arrive across two reads: whatever is left of a
/// partial one stays in `carry` and is finished by the bytes that follow. A
/// megabyte image arrives in chunks by design, so this is the common case
/// rather than an edge.
#[derive(Debug, Default)]
pub struct Reader {
    carry: Vec<u8>,
}

impl Reader {
    /// Split `bytes` into what the terminal parser should see and the commands
    /// that were in it.
    pub fn take(&mut self, bytes: &[u8]) -> (Vec<u8>, Vec<Command>) {
        self.carry.extend_from_slice(bytes);
        let mut text = Vec::with_capacity(self.carry.len());
        let mut found = Vec::new();
        loop {
            let Some(start) = find(&self.carry, OPEN) else {
                // Everything except a tail that could be the beginning of an
                // opener. Half an escape reaching the terminal downstream is an
                // escape it waits for the end of, and the screen stops.
                let cut = (self.carry.len().saturating_sub(OPEN.len() - 1)..self.carry.len())
                    .find(|&k| OPEN.starts_with(&self.carry[k..]))
                    .unwrap_or(self.carry.len());
                text.extend_from_slice(&self.carry[..cut]);
                self.carry.drain(..cut);
                return (text, found);
            };
            text.extend_from_slice(&self.carry[..start]);
            let body = start + OPEN.len();
            let Some((end, len)) = terminator(&self.carry[body..]) else {
                // The rest is an unfinished command, held until it finishes.
                self.carry.drain(..start);
                return (text, found);
            };
            if let Some(cmd) = parse(&self.carry[body..body + end]) {
                found.push(cmd);
            }
            self.carry.drain(..body + end + len);
        }
    }
}

/// Where the APC ends, and how many bytes the terminator takes.
///
/// `ESC \` is the one the protocol specifies; a bare BEL is what some programs
/// send and every terminal accepts.
fn terminator(bytes: &[u8]) -> Option<(usize, usize)> {
    let esc = find(bytes, b"\x1b\\").map(|at| (at, 2));
    let bel = bytes.iter().position(|b| *b == 0x07).map(|at| (at, 1));
    match (esc, bel) {
        (Some(a), Some(b)) if b.0 < a.0 => Some(b),
        (Some(a), _) => Some(a),
        (None, b) => b,
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .filter(|_| !needle.is_empty())
}

/// `a=T,f=100,i=3;<base64>` into keys and a payload.
fn parse(body: &[u8]) -> Option<Command> {
    let split = body.iter().position(|b| *b == b';');
    let (head, payload) = match split {
        Some(at) => (&body[..at], body[at + 1..].to_vec()),
        None => (body, Vec::new()),
    };
    let head = std::str::from_utf8(head).ok()?;
    let mut keys = HashMap::new();
    for pair in head.split(',') {
        let Some((k, v)) = pair.split_once('=') else {
            continue;
        };
        if !k.is_empty() {
            keys.insert(k.to_string(), v.to_string());
        }
    }
    Some(Command { keys, payload })
}

/// The biggest image dirk will hold, in bytes of payload.
///
/// A bound rather than a preference. Without one a program that transmits
/// without ever displaying — or one that is simply wrong — grows the session's
/// memory until something else fails, and the something else is never the pane
/// that did it.
pub const MOST: usize = 8 * 1024 * 1024;

/// The most placements one pane will carry.
///
/// Old ones go first: a plot redrawn every second is a placement every second,
/// and the one that matters is the newest.
pub const PLACEMENTS: usize = 64;

/// One image, as the bytes the program sent.
#[derive(Debug, Clone)]
pub struct Image {
    /// Everything before the payload, so it can be sent on unchanged. dirk does
    /// not decode the image: what it is is between the program and the terminal.
    pub keys: HashMap<String, String>,
    pub payload: Vec<u8>,
    /// Whether it has been given to the terminal downstream yet, and under
    /// which id. dirk renumbers: two panes each using image 1 are two images.
    pub sent: Option<u32>,
}

/// Where an image is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Placement {
    pub image: u32,
    /// Which line of the pane's whole history it sits on, counting the
    /// scrollback. Anchored there rather than to a screen row so that it moves
    /// with its text: the screen row is a view onto this and changes whenever
    /// anything scrolls.
    pub line: usize,
    pub col: u16,
    /// How many cells it covers, when the program said. Nothing means the
    /// terminal downstream decides, which is what it does when a program does
    /// not say either.
    pub cells: Option<(u16, u16)>,
}

/// Every image and placement one pane has.
#[derive(Debug, Default)]
pub struct Store {
    images: HashMap<u32, Image>,
    /// The image being assembled from chunks, if one is.
    building: Option<(u32, Image)>,
    places: Vec<Placement>,
    /// Set when anything changed, so a frame that has nothing new to say says
    /// nothing.
    pub dirty: bool,
}

impl Store {
    #[cfg(test)]
    pub fn images(&self) -> &HashMap<u32, Image> {
        &self.images
    }

    pub fn places(&self) -> &[Placement] {
        &self.places
    }

    pub fn image_mut(&mut self, id: u32) -> Option<&mut Image> {
        self.images.get_mut(&id)
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.places.is_empty()
    }

    /// Act on one command, at the cursor it arrived at.
    ///
    /// `line` is where the pane's cursor is in its whole history, which is what
    /// a placement is anchored to.
    pub fn apply(&mut self, cmd: &Command, line: usize, col: u16) {
        match cmd.action() {
            // Transmit, and transmit-and-display.
            't' | 'T' => {
                let id = self.absorb(cmd);
                if cmd.action() == 'T' && !cmd.more() {
                    self.place(id, line, col, cmd.cells());
                }
            }
            'p' => self.place(cmd.id(), line, col, cmd.cells()),
            'd' => self.delete(cmd),
            // `q` is a query, which only the terminal downstream can answer,
            // and anything else is from a newer protocol than this. Ignored
            // rather than guessed at.
            _ => {}
        }
    }

    /// Add a chunk to the image being built, and answer which image it is.
    fn absorb(&mut self, cmd: &Command) -> u32 {
        let id = match cmd.id() {
            0 => self.building.as_ref().map_or(1, |(id, _)| *id),
            id => id,
        };
        let (_, image) = self.building.get_or_insert_with(|| {
            (
                id,
                Image {
                    keys: cmd.keys.clone(),
                    payload: Vec::new(),
                    sent: None,
                },
            )
        });
        // Dropped at the cap rather than truncated: half an image is not a
        // smaller image, it is one the terminal downstream cannot decode.
        if image.payload.len() + cmd.payload.len() <= MOST {
            image.payload.extend_from_slice(&cmd.payload);
        } else {
            image.payload.clear();
        }
        if !cmd.more() {
            let (id, image) = self.building.take().unwrap_or((
                id,
                Image {
                    keys: cmd.keys.clone(),
                    payload: Vec::new(),
                    sent: None,
                },
            ));
            if !image.payload.is_empty() {
                self.images.insert(id, image);
                self.dirty = true;
            }
        }
        id
    }

    fn place(&mut self, image: u32, line: usize, col: u16, cells: Option<(u16, u16)>) {
        // An image nobody transmitted cannot be shown, and saying so quietly is
        // right: the program may be about to send it.
        if !self.images.contains_key(&image) {
            return;
        }
        self.places.push(Placement {
            image,
            line,
            col,
            cells,
        });
        if self.places.len() > PLACEMENTS {
            let over = self.places.len() - PLACEMENTS;
            self.places.drain(..over);
        }
        self.dirty = true;
    }

    fn delete(&mut self, cmd: &Command) {
        let what = cmd.get("d").and_then(|d| d.chars().next()).unwrap_or('a');
        // Uppercase means the image goes too; lowercase leaves it to be placed
        // again, which is what makes re-placing cheap.
        let free = what.is_uppercase();
        match what.to_ascii_lowercase() {
            'i' => {
                let id = cmd.id();
                self.places.retain(|p| p.image != id);
                if free {
                    self.images.remove(&id);
                }
            }
            _ => {
                self.places.clear();
                if free {
                    self.images.clear();
                }
            }
        }
        self.dirty = true;
    }

    /// Forget placements that have scrolled out of the history entirely.
    ///
    /// A pane keeps a bounded scrollback, so a line old enough is gone and the
    /// placement anchored to it is a picture of nothing.
    pub fn forget_before(&mut self, line: usize) {
        let before = self.places.len();
        self.places.retain(|p| p.line >= line);
        if self.places.len() != before {
            self.dirty = true;
        }
        // An image nothing points at any more is bytes nobody will ask for.
        let wanted: std::collections::HashSet<u32> = self.places.iter().map(|p| p.image).collect();
        self.images.retain(|id, _| wanted.contains(id));
    }
}

/// One image on the screen, as the frame is about to leave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    /// dirk's own id for it downstream. Two panes each using image 1 are two
    /// images, and the terminal below is told so.
    pub id: u32,
    pub row: u16,
    pub col: u16,
    pub cells: Option<(u16, u16)>,
}

/// Give an image to the terminal below, under dirk's id for it.
///
/// The keys the program sent are passed on unchanged apart from the ones dirk
/// owns: the id, the action, and quiet. dirk does not decode the image — what
/// it is is between the program and the terminal — so anything it does not
/// understand is exactly what it should forward.
pub fn transmit(image: &Image, id: u32) -> Vec<u8> {
    let mut keys: Vec<String> = image
        .keys
        .iter()
        .filter(|(k, _)| !matches!(k.as_str(), "a" | "i" | "q" | "I" | "p"))
        .map(|(k, v)| format!("{k}={v}"))
        .collect();
    // Sorted, so the same image is the same bytes every time and a diff of a
    // recording is readable.
    keys.sort();
    // `q=2` suppresses the terminal's replies. dirk is not reading them, and a
    // reply it did not read would arrive in the client's *input*, which is a
    // burst of `\x1b_G...` typed into whatever pane has the keyboard.
    let head = format!("a=t,i={id},q=2,{}", keys.join(","));
    wrap(&head, &image.payload)
}

/// Show an image that has already been given to the terminal below.
pub fn put(shown: &Shown) -> Vec<u8> {
    let mut head = format!("a=p,i={},q=2,C=1", shown.id);
    if let Some((c, r)) = shown.cells {
        head.push_str(&format!(",c={c},r={r}"));
    }
    wrap(&head, &[])
}

/// Take every placement off the screen, leaving the images.
///
/// Sent before a frame's placements rather than after the last one: dirk redraws
/// by repainting, and a placement nobody removed stays where it was while the
/// text under it has moved.
pub fn clear() -> Vec<u8> {
    wrap("a=d,d=a,q=2", &[])
}

fn wrap(head: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(head.len() + payload.len() + 8);
    out.extend_from_slice(OPEN);
    out.extend_from_slice(head.as_bytes());
    if !payload.is_empty() {
        out.push(b';');
        out.extend_from_slice(payload);
    }
    out.extend_from_slice(b"\x1b\\");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(bytes: &[u8]) -> (Vec<u8>, Vec<Command>) {
        Reader::default().take(bytes)
    }

    fn cmd(text: &str) -> Command {
        let (_, mut cmds) = one(format!("\x1b_G{text}\x1b\\").as_bytes());
        cmds.pop().expect("a command")
    }

    #[test]
    fn an_image_is_sent_once_and_shown_many_times() {
        // The whole design: a payload can be a megabyte and a placement is
        // twenty bytes, so a redraw re-places rather than re-sending.
        let mut s = Store::default();
        s.apply(&cmd("a=t,i=1;AAAA"), 0, 0);
        assert_eq!(s.images().len(), 1);
        assert!(s.is_empty(), "transmitting is not showing");

        s.apply(&cmd("a=p,i=1"), 10, 4);
        s.apply(&cmd("a=p,i=1"), 20, 0);
        assert_eq!(s.places().len(), 2);
        assert_eq!(s.places()[0].line, 10);
        assert_eq!(s.places()[0].col, 4);
    }

    #[test]
    fn an_image_arriving_in_chunks_is_one_image() {
        let mut s = Store::default();
        s.apply(&cmd("a=T,i=2,m=1;AAAA"), 0, 0);
        assert!(s.images().is_empty(), "an unfinished image was stored");
        assert!(s.is_empty(), "an unfinished image was shown");
        s.apply(&cmd("a=T,i=2,m=0;BBBB"), 3, 0);
        assert_eq!(
            s.images().get(&2).map(|i| i.payload.clone()),
            Some(b"AAAABBBB".to_vec())
        );
        assert_eq!(s.places().len(), 1, "transmit-and-display did not display");
    }

    #[test]
    fn an_image_nobody_sent_is_not_shown() {
        // Quietly: the program may be about to send it, and a complaint about
        // an order dirk cannot see the whole of is a complaint about nothing.
        let mut s = Store::default();
        s.apply(&cmd("a=p,i=9"), 0, 0);
        assert!(s.is_empty());
    }

    #[test]
    fn an_oversized_image_is_dropped_and_the_others_are_not() {
        // One program writing rubbish must not cost the session its memory, and
        // whatever fails first is never the pane that did it.
        let mut s = Store::default();
        s.apply(&cmd("a=t,i=1;AAAA"), 0, 0);
        let huge = "B".repeat(MOST + 1);
        s.apply(&cmd(&format!("a=T,i=2;{huge}")), 0, 0);
        assert!(s.images().contains_key(&1), "the good one went too");
        assert!(!s.images().contains_key(&2), "the oversized one was kept");
    }

    #[test]
    fn deleting_says_whether_the_image_goes_with_the_placement() {
        let mut s = Store::default();
        s.apply(&cmd("a=T,i=1;AAAA"), 0, 0);
        // Lowercase takes the placement and leaves the image, which is what
        // makes showing it again cheap.
        s.apply(&cmd("a=d,d=i,i=1"), 0, 0);
        assert!(s.is_empty());
        assert!(s.images().contains_key(&1));
        s.apply(&cmd("a=p,i=1"), 5, 0);
        assert_eq!(s.places().len(), 1);
        // Uppercase takes both.
        s.apply(&cmd("a=d,d=I,i=1"), 0, 0);
        assert!(s.is_empty());
        assert!(s.images().is_empty());
    }

    #[test]
    fn a_placement_that_has_scrolled_out_of_history_is_forgotten() {
        // A pane keeps a bounded scrollback, so a line old enough is gone and a
        // placement anchored to it is a picture of nothing.
        let mut s = Store::default();
        s.apply(&cmd("a=t,i=1;AAAA"), 0, 0);
        s.apply(&cmd("a=p,i=1"), 5, 0);
        s.apply(&cmd("a=p,i=1"), 500, 0);
        s.forget_before(100);
        assert_eq!(s.places().len(), 1);
        assert_eq!(s.places()[0].line, 500);
        // And the image is kept, because something still points at it.
        assert!(s.images().contains_key(&1));
        s.forget_before(1000);
        assert!(s.images().is_empty(), "bytes nobody will ask for were kept");
    }

    #[test]
    fn what_is_sent_on_keeps_the_keys_dirk_does_not_own() {
        // dirk does not decode the image: what it is is between the program and
        // the terminal, so anything dirk does not understand is exactly what it
        // should forward.
        let mut s = Store::default();
        s.apply(&cmd("a=t,i=1,f=100,s=64,v=64,X=3;AAAA"), 0, 0);
        let image = s.images().get(&1).expect("the image");
        let bytes = String::from_utf8(transmit(image, 77)).expect("utf8");
        assert!(bytes.starts_with("\x1b_Ga=t,i=77,q=2,"), "{bytes}");
        for kept in ["f=100", "s=64", "v=64", "X=3"] {
            assert!(bytes.contains(kept), "{kept} was dropped: {bytes}");
        }
        assert!(bytes.ends_with(";AAAA\x1b\\"), "{bytes}");
        // The program's own id is replaced, not passed on: two panes each using
        // image 1 are two images.
        assert!(
            !bytes.contains("i=1,"),
            "the program's id survived: {bytes}"
        );
    }

    #[test]
    fn what_dirk_sends_never_asks_the_terminal_to_reply() {
        // A reply dirk did not read arrives in the client's *input*, which is a
        // burst of escape bytes typed into whatever pane has the keyboard.
        let shown = Shown {
            id: 4,
            row: 0,
            col: 0,
            cells: Some((10, 5)),
        };
        for bytes in [put(&shown), clear()] {
            let text = String::from_utf8(bytes).expect("utf8");
            assert!(text.contains("q=2"), "something could be answered: {text}");
        }
        let text = String::from_utf8(put(&shown)).expect("utf8");
        assert!(text.contains("c=10,r=5"), "the size was lost: {text}");
        // `C=1` keeps the terminal from moving the cursor, which dirk has
        // already placed for the frame.
        assert!(text.contains("C=1"), "it would move the cursor: {text}");
    }

    #[test]
    fn a_command_is_taken_out_and_the_text_is_left() {
        let (text, cmds) = one(b"before\x1b_Ga=T,f=100,i=3;AAAA\x1b\\after");
        assert_eq!(text, b"beforeafter");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].action(), 'T');
        assert_eq!(cmds[0].id(), 3);
        assert_eq!(cmds[0].payload, b"AAAA");
    }

    #[test]
    fn a_bare_bell_ends_one_too() {
        // The protocol says `ESC \`, and programs send a BEL. Every terminal
        // takes both, so refusing one would make dirk the only thing that does
        // not.
        let (text, cmds) = one(b"\x1b_Ga=p,i=1\x07tail");
        assert_eq!(text, b"tail");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].action(), 'p');
    }

    #[test]
    fn a_command_split_across_two_reads_is_still_one_command() {
        // A megabyte image arrives in chunks by design, so this is the common
        // case and not an edge.
        let mut r = Reader::default();
        let (text, cmds) = r.take(b"one\x1b_Ga=T,i=7;AAA");
        assert_eq!(text, b"one");
        assert!(cmds.is_empty(), "an unfinished command was acted on");
        let (text, cmds) = r.take(b"BBB\x1b\\two");
        assert_eq!(text, b"two");
        assert_eq!(cmds.len(), 1);
        assert_eq!(cmds[0].payload, b"AAABBB");
    }

    #[test]
    fn an_opener_split_across_two_reads_is_not_passed_on() {
        // Half an escape reaching the terminal downstream is an escape it waits
        // for the end of, and the screen stops updating.
        let mut r = Reader::default();
        let (text, _) = r.take(b"x\x1b_");
        assert_eq!(text, b"x", "half an opener was passed through");
        let (text, cmds) = r.take(b"Ga=p,i=1\x1b\\y");
        assert_eq!(text, b"y");
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn several_in_one_read_are_all_found() {
        let (text, cmds) = one(b"\x1b_Ga=t,i=1;AA\x1b\\mid\x1b_Ga=p,i=1\x1b\\end");
        assert_eq!(text, b"midend");
        assert_eq!(cmds.len(), 2);
        assert_eq!(cmds[0].action(), 't');
        assert_eq!(cmds[1].action(), 'p');
    }

    #[test]
    fn nonsense_in_the_keys_does_not_take_the_stream_with_it() {
        // A malformed command is dropped and the text around it survives: one
        // program writing rubbish must not stop every pane from drawing.
        let (text, cmds) = one(b"a\x1b_G=,,=,x\x1b\\b");
        assert_eq!(text, b"ab");
        assert_eq!(
            cmds.len(),
            1,
            "it should still be a command, just an empty one"
        );
        assert_eq!(cmds[0].action(), 't', "with the protocol's default action");
    }
}
