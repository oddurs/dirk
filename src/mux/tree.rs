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

//! The split tree: how the panes of one workspace divide the screen.
//!
//! The tree refers to panes by id rather than owning them. Panes live in a flat
//! arena on the workspace, which keeps this file about geometry alone and makes
//! moving a pane between workspaces a tree edit rather than a question about
//! ownership.
//!
//! Division is `ratatui::layout::Layout`, not arithmetic. Rounding, minimum
//! sizes and where the leftover column goes are all solved there, and getting
//! them subtly wrong is the kind of bug that appears as a one-column gap at some
//! terminal widths and nowhere else.
//!
//! Two invariants the operations maintain, because everything else assumes
//! them:
//!
//! - **No single-child splits.** Removing a leaf collapses any split left
//!   holding one child into that child. A tree that accumulates them still
//!   renders correctly and is much harder to reason about.
//! - **No empty splits.** A split that loses its last child tells its parent to
//!   drop it, recursively, so the only way to have nothing is an empty
//!   workspace.

use crate::mux::PaneId;
use ratatui::layout::{Constraint, Direction, Layout, Rect};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Children side by side.
    Cols,
    /// Children stacked.
    Rows,
}

impl From<Dir> for Direction {
    fn from(d: Dir) -> Self {
        match d {
            Dir::Cols => Direction::Horizontal,
            Dir::Rows => Direction::Vertical,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Node {
    Leaf(PaneId),
    Split {
        dir: Dir,
        children: Vec<(Constraint, Node)>,
    },
}

impl Node {
    /// Where each pane goes, in tree order.
    pub fn rects(&self, area: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.collect_rects(area, &mut out);
        out
    }

    fn collect_rects(&self, area: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Node::Leaf(id) => out.push((*id, area)),
            Node::Split { dir, children } => {
                let constraints: Vec<Constraint> = children.iter().map(|(c, _)| *c).collect();
                let areas = Layout::new(Direction::from(*dir), constraints).split(area);
                for ((_, node), a) in children.iter().zip(areas.iter()) {
                    node.collect_rects(*a, out);
                }
            }
        }
    }

    /// Every pane, in the order it is drawn. That order is also the order focus
    /// cycles in, which is why it is tree order and not insertion order.
    pub fn leaves(&self) -> Vec<PaneId> {
        let mut out = Vec::new();
        self.collect_leaves(&mut out);
        out
    }

    fn collect_leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            Node::Leaf(id) => out.push(*id),
            Node::Split { children, .. } => {
                children.iter().for_each(|(_, n)| n.collect_leaves(out))
            }
        }
    }

    /// Put `new` beside `target`, splitting in `dir`.
    ///
    /// When the split that already holds `target` runs in the same direction,
    /// `new` joins it as a sibling instead of nesting. Without that, three
    /// successive splits to the right give 1/2, 1/4, 1/4 rather than thirds —
    /// each split halving only the pane it was invoked on.
    pub fn split(&mut self, target: PaneId, dir: Dir, new: PaneId) -> bool {
        if let Node::Split { dir: d, children } = self
            && *d == dir
            && let Some(i) = children
                .iter()
                .position(|(_, n)| matches!(n, Node::Leaf(x) if *x == target))
        {
            children.insert(i + 1, (Constraint::Fill(1), Node::Leaf(new)));
            return true;
        }

        match self {
            Node::Leaf(id) => {
                if *id != target {
                    return false;
                }
                let existing = Node::Leaf(*id);
                *self = Node::Split {
                    dir,
                    children: vec![
                        (Constraint::Fill(1), existing),
                        (Constraint::Fill(1), Node::Leaf(new)),
                    ],
                };
                true
            }
            // `any` short-circuits, which is what is wanted: exactly one leaf
            // can match, and stopping there avoids walking the rest.
            Node::Split { children, .. } => {
                children.iter_mut().any(|(_, n)| n.split(target, dir, new))
            }
        }
    }

    /// Take `id` out of the tree.
    ///
    /// Returns true when this node is now empty and its parent should drop it —
    /// which for a leaf means it was the one being removed. A caller holding the
    /// root should read true as "the workspace has no panes left".
    pub fn remove(&mut self, id: PaneId) -> bool {
        // The collapse cannot happen inside the match: `children` is borrowed
        // from `self` there, and replacing `self` with one of its own children
        // needs that borrow to have ended first.
        let mut collapse_to = None;

        let empty = match self {
            Node::Leaf(x) => return *x == id,
            Node::Split { children, .. } => {
                let mut hit = None;
                for (i, (_, child)) in children.iter_mut().enumerate() {
                    if child.remove(id) {
                        hit = Some(i);
                        break;
                    }
                }
                // Either the id is not in this subtree, or a descendant removed
                // it and is still holding panes of its own. Neither is our
                // business.
                let Some(i) = hit else { return false };
                children.remove(i);

                match children.len() {
                    0 => true,
                    1 => {
                        collapse_to = Some(children.remove(0).1);
                        false
                    }
                    _ => false,
                }
            }
        };

        // A split holding one child is that child.
        if let Some(only) = collapse_to {
            *self = only;
        }
        empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: PaneId) -> Node {
        Node::Leaf(id)
    }

    fn area() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 24,
        }
    }

    #[test]
    fn a_lone_pane_gets_the_whole_area() {
        assert_eq!(leaf(1).rects(area()), vec![(1, area())]);
    }

    #[test]
    fn splitting_in_the_same_direction_gives_equal_shares() {
        // The bug this exists to prevent: nesting every split would make these
        // 1/2, 1/4, 1/4 rather than thirds.
        let mut t = leaf(1);
        assert!(t.split(1, Dir::Cols, 2));
        assert!(t.split(2, Dir::Cols, 3));

        let rects = t.rects(area());
        assert_eq!(rects.len(), 3);

        // Assert the properties, not where ratatui happens to put the leftover
        // column. 80 does not divide by three, and which pane absorbs the
        // remainder is its business.
        let widths: Vec<u16> = rects.iter().map(|(_, r)| r.width).collect();
        assert_eq!(widths.iter().sum::<u16>(), 80, "the area is fully used");
        assert!(
            widths.iter().all(|w| w.abs_diff(80 / 3) <= 1),
            "each within a column of a third: {widths:?}"
        );
        assert_contiguous_cols(&rects, area());
    }

    /// No gaps and no overlaps: each pane starts where the previous one ended,
    /// the first at the left edge and the last at the right.
    fn assert_contiguous_cols(rects: &[(PaneId, Rect)], area: Rect) {
        let mut x = area.x;
        for (id, r) in rects {
            assert_eq!(r.x, x, "pane {id} starts at {}, expected {x}", r.x);
            x = r.right();
        }
        assert_eq!(x, area.right(), "the last pane ends at the right edge");
    }

    #[test]
    fn splitting_the_other_way_nests() {
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        t.split(2, Dir::Rows, 3);

        let rects = t.rects(area());
        assert_eq!(rects.len(), 3);
        // 1 keeps its full-height column; 2 and 3 share the other one.
        assert_eq!(rects[0].1.height, 24);
        assert_eq!(rects[1].1.height, 12);
        assert_eq!(rects[2].1.height, 12);
        assert_eq!(rects[1].1.x, rects[2].1.x);
    }

    #[test]
    fn tree_order_is_draw_order_and_focus_order() {
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        t.split(1, Dir::Rows, 3);
        assert_eq!(t.leaves(), vec![1, 3, 2]);
    }

    #[test]
    fn removing_a_leaf_collapses_the_split_it_leaves_behind() {
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        assert!(!t.remove(2));
        // Not a split holding one child — the child itself.
        assert!(matches!(t, Node::Leaf(1)));
        assert_eq!(t.rects(area()), vec![(1, area())]);
    }

    #[test]
    fn collapsing_is_recursive() {
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        t.split(2, Dir::Rows, 3);
        t.split(3, Dir::Cols, 4);
        // Unwind the nesting; each removal should leave no empty scaffolding.
        t.remove(4);
        t.remove(3);
        assert!(matches!(t, Node::Split { .. }));
        t.remove(2);
        assert!(matches!(t, Node::Leaf(1)), "got {t:?}");
    }

    #[test]
    fn removing_the_last_pane_reports_the_tree_is_empty() {
        let mut t = leaf(1);
        assert!(t.remove(1));
    }

    #[test]
    fn removing_something_that_is_not_there_changes_nothing() {
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        let before = t.leaves();
        assert!(!t.remove(99));
        assert_eq!(t.leaves(), before);
    }

    #[test]
    fn splitting_a_pane_that_is_not_there_is_refused() {
        let mut t = leaf(1);
        assert!(!t.split(99, Dir::Cols, 2));
        assert_eq!(t.leaves(), vec![1]);
    }

    #[test]
    fn every_pane_gets_a_rect_inside_the_area() {
        let mut t = leaf(1);
        for i in 2..=6 {
            let dir = if i % 2 == 0 { Dir::Cols } else { Dir::Rows };
            assert!(t.split(i - 1, dir, i));
        }
        let rects = t.rects(area());
        assert_eq!(rects.len(), 6);
        for (_, r) in rects {
            assert!(
                r.right() <= 80 && r.bottom() <= 24,
                "{r:?} escaped the area"
            );
        }
    }
}
