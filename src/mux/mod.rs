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

//! The multiplexer core: panes, and the tree they hang in.

pub mod pty;
pub mod session;
pub mod tree;

pub use pty::{Pane, PaneId};
pub use session::{Focus, Session, Workspace};
pub use tree::Dir;

/// Everything the event loop can be woken by. One channel, three producers:
/// the terminal reader, one thread per pane, and the ticker. The loop blocks on
/// a receive and never polls, so an idle dirk costs nothing.
#[derive(Debug)]
pub enum Ev {
    Term(crossterm::event::Event),
    /// The id is unused while dirk redraws whole frames; it is what a
    /// damage-tracked renderer would key on.
    Output(#[allow(dead_code)] PaneId),
    Exited(PaneId),
    Tick,
}
