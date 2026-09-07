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

//! What is under the pointer.
//!
//! dirk is clickable first, and the cheapest way to mean that is to have the
//! renderer say what it just drew. Every clickable thing registers its rect as
//! it is painted, so there is no second layout pass that can disagree with the
//! first — the map is a by-product of drawing, not a model of it.
//!
//! Later pushes win. Things drawn on top of other things are hit first, which
//! is what makes the picker overlay swallow clicks meant for the tree beneath.

use ratatui::layout::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Layout(usize),
    /// The disclosure triangle and the project name both fold the project.
    ProjectFold(usize),
    Workspace {
        p: usize,
        w: usize,
    },
    /// One pane inside the focused workspace, by draw order.
    Pane {
        index: usize,
    },
    /// A pane reached from the nav, which may be in a workspace that is not
    /// focused yet.
    NavPane {
        p: usize,
        w: usize,
        index: usize,
    },
    NewWorkspace(usize),
    OpenProject,
    PickerRow(usize),
    /// The agents list is the only one whose order is a choice.
    SortAgents,
}

#[derive(Default)]
pub struct HitMap {
    spots: Vec<(Rect, Target)>,
}

impl HitMap {
    pub fn clear(&mut self) {
        self.spots.clear();
    }

    pub fn push(&mut self, area: Rect, target: Target) {
        self.spots.push((area, target));
    }

    pub fn at(&self, x: u16, y: u16) -> Option<Target> {
        self.spots
            .iter()
            .rev()
            .find(|(r, _)| x >= r.x && x < r.x + r.width && y >= r.y && y < r.y + r.height)
            .map(|(_, t)| *t)
    }
}
