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

pub mod layout;
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
    /// A client connected, with the screen it is to be shown.
    Attach(Box<crate::server::View>),
    /// A client went away, named so that one being taken over cannot clear the
    /// view of the one that replaced it. The session does not go anywhere.
    Detach(u64),
    /// Something asked of the session from outside, and where to answer.
    ///
    /// Answered on the loop that owns the state rather than from the socket
    /// thread, so a command sees the session between frames rather than halfway
    /// through one.
    Command(
        crate::wire::Request,
        std::sync::mpsc::SyncSender<crate::wire::Reply>,
    ),
    /// The id is unused while dirk redraws whole frames; it is what a
    /// damage-tracked renderer would key on.
    Output(#[allow(dead_code)] PaneId),
    Exited(PaneId),
    /// A sample of what is running in each pane finished.
    Agents(crate::agent::Reading),
    /// A second-source intent arrived for a pane whose title said nothing.
    Suggested {
        pane: PaneId,
        intent: String,
    },
    /// A git read finished. Answered by path rather than by index, because a
    /// project can be closed while its answer is still in flight.
    Git(crate::git::Answer),
    /// A round of board status commands finished. Answered by name rather than
    /// by index, because the list can be reloaded while one is in flight.
    Badges(Vec<(String, Option<String>)>),
    Tick,
}
