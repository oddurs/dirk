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

    /// Move the boundary the pane sits against, by cells.
    ///
    /// Positive is right or down, whichever way the split runs — the boundary
    /// moves, rather than this pane growing. That is what `l` means when the
    /// focused pane is on the right of a pair: the edge beside it is on its
    /// left, and moving it right makes that pane narrower. tmux does the same,
    /// and a version that always grew the focused pane would mean `l` and `h`
    /// swapping meaning depending on which pane you were in.
    ///
    /// The innermost split running the right way is the one it sits against: in
    /// a column inside a row, `l` moves the column's edge and not the outer
    /// one. Anything else resizes a boundary somewhere else on the screen,
    /// which is the version of this that people give up on.
    ///
    /// Shares are rewritten in cells rather than nudged as weights, so a press
    /// is a column and not a proportion of one — and the other children keep
    /// the sizes they had, so nothing moves except the boundary asked for.
    pub fn nudge(&mut self, target: PaneId, dir: Dir, cells: i32, area: Rect) -> bool {
        // Narrower than this and there is nothing left to look at, so the
        // boundary stops rather than the pane disappearing.
        const MIN: u16 = 3;
        let Node::Split { dir: d, children } = self else {
            return false;
        };
        let Some(i) = children
            .iter()
            .position(|(_, n)| n.leaves().contains(&target))
        else {
            return false;
        };
        let constraints: Vec<Constraint> = children.iter().map(|(c, _)| *c).collect();
        let areas = Layout::new(Direction::from(*d), constraints).split(area);

        // Deeper first: a split of the right kind further in is the boundary
        // this pane is actually beside.
        if let Some(a) = areas.get(i)
            && children[i].1.nudge(target, dir, cells, *a)
        {
            return true;
        }
        if *d != dir || children.len() < 2 {
            return false;
        }

        let along = |r: &Rect| match dir {
            Dir::Cols => r.width,
            Dir::Rows => r.height,
        };
        let mut sizes: Vec<u16> = areas.iter().map(along).collect();
        // The neighbour on the far side of the boundary being moved: the one
        // after, unless this is the last, in which case the one before and the
        // sign flips with it.
        let (j, cells) = match i + 1 < sizes.len() {
            true => (i + 1, cells),
            false => (i - 1, -cells),
        };
        let want = sizes[i] as i32 + cells;
        let give = sizes[j] as i32 - cells;
        if want < MIN as i32 || give < MIN as i32 {
            return false;
        }
        sizes[i] = want as u16;
        sizes[j] = give as u16;
        for ((c, _), size) in children.iter_mut().zip(sizes) {
            *c = Constraint::Fill(size.max(1));
        }
        true
    }

    /// Every pane, in the order it is drawn. That order is also the order focus
    /// cycles in, which is why it is tree order and not insertion order.
    /// Drop any leaf that is not in the list, and collapse what that empties.
    ///
    /// A handoff can lose a pane -- a descriptor that did not survive, a
    /// process already gone -- and a tree still pointing at it would lay out a
    /// rectangle for something that does not exist.
    pub fn without_missing(self, kept: &[PaneId]) -> Node {
        match self {
            Node::Leaf(id) => Node::Leaf(id),
            Node::Split { dir, children } => {
                let mut left: Vec<_> = children
                    .into_iter()
                    .filter_map(|(c, n)| match &n {
                        Node::Leaf(id) if !kept.contains(id) => None,
                        _ => Some((c, n.without_missing(kept))),
                    })
                    .filter(|(_, n)| !n.is_empty(kept))
                    .collect();
                // A split with one child left is not a split.
                match left.len() {
                    1 => left.remove(0).1,
                    _ => Node::Split {
                        dir,
                        children: left,
                    },
                }
            }
        }
    }

    /// Whether nothing under here survived.
    fn is_empty(&self, kept: &[PaneId]) -> bool {
        match self {
            Node::Leaf(id) => !kept.contains(id),
            Node::Split { children, .. } => children.iter().all(|(_, n)| n.is_empty(kept)),
        }
    }

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

    /// Swap one pane for another in the same slot, keeping the shape.
    ///
    /// A restarted panel comes back where it was rather than the layout
    /// rearranging around it.
    /// Exchange two panes' places, leaving the shape of the tree alone.
    ///
    /// Which is the whole of what moving a pane is. Nothing is spawned, nothing
    /// is closed, and neither program notices anything beyond a resize -- the
    /// alternative, closing one and opening another, is a different operation
    /// wearing the same name and the thing running inside would not survive it.
    pub fn swap(&mut self, a: PaneId, b: PaneId) -> bool {
        if a == b {
            return false;
        }
        // Both, or neither: half a swap leaves the same pane in two places and
        // the other one nowhere, which draws as a workspace that lost a pane.
        if !self.holds(a) || !self.holds(b) {
            return false;
        }
        self.put(a, b);
        true
    }

    fn holds(&self, id: PaneId) -> bool {
        self.leaves().contains(&id)
    }

    fn put(&mut self, a: PaneId, b: PaneId) {
        match self {
            Node::Leaf(x) if *x == a => *x = b,
            Node::Leaf(x) if *x == b => *x = a,
            Node::Leaf(_) => {}
            Node::Split { children, .. } => children.iter_mut().for_each(|(_, n)| n.put(a, b)),
        }
    }

    pub fn replace(&mut self, old: PaneId, new: PaneId) -> bool {
        match self {
            Node::Leaf(x) if *x == old => {
                *x = new;
                true
            }
            Node::Leaf(_) => false,
            Node::Split { children, .. } => children.iter_mut().any(|(_, n)| n.replace(old, new)),
        }
    }

    /// Take `id` out of the tree.
    ///
    /// Returns true when this node is now empty and its parent should drop it —
    /// which for a leaf means it was the one being removed. A caller holding the
    /// root should read true as "the workspace has no panes left".
    ///
    /// A split left holding one child collapses to that child **in the slot the
    /// split occupied**, and is deliberately not flattened into the grandparent
    /// even when the directions match. Closing a pane therefore gives its space
    /// to its own siblings and never resizes panes elsewhere in the tree.
    ///
    /// The cost is that a same-direction split can end up nested inside
    /// another, so a tree's shape depends on the order it was built in. That is
    /// the right trade: flattening would make closing one pane silently resize
    /// unrelated ones, and a pane changing width because something far away
    /// closed is much harder to understand than a slightly deeper tree.
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
    #[test]
    fn swapping_two_panes_leaves_the_shape_alone() {
        // Moving a pane is exchanging places, not rebuilding: the tree keeps
        // its shape and neither program notices anything but a resize.
        let mut tree = Node::Leaf(1);
        tree.split(1, Dir::Cols, 2);
        tree.split(2, Dir::Cols, 3);
        assert_eq!(tree.leaves(), [1, 2, 3]);

        assert!(tree.swap(1, 3));
        assert_eq!(tree.leaves(), [3, 2, 1]);
        assert_eq!(tree.rects(Rect::new(0, 0, 30, 10)).len(), 3);
    }

    #[test]
    fn a_swap_with_a_pane_that_is_not_here_changes_nothing() {
        // Half a swap leaves the same pane in two places and the other one
        // nowhere, which draws as a workspace that lost a pane.
        let mut tree = Node::Leaf(1);
        tree.split(1, Dir::Cols, 2);
        assert!(!tree.swap(1, 99), "it swapped with something imaginary");
        assert_eq!(tree.leaves(), [1, 2]);
        assert!(!tree.swap(1, 1), "swapping with itself is not a move");
    }

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
    fn moves_the_boundary_the_pane_is_beside() {
        let area = Rect::new(0, 0, 80, 24);
        let mut t = Node::Leaf(1);
        t.split(1, Dir::Cols, 2);
        let width = |t: &Node, id: PaneId| {
            t.rects(area)
                .into_iter()
                .find(|(x, _)| *x == id)
                .map(|(_, r)| r.width)
                .unwrap()
        };
        let before = width(&t, 1);
        assert!(t.nudge(1, Dir::Cols, 10, area));
        assert_eq!(
            width(&t, 1),
            before + 10,
            "a press is a column, not a share"
        );
        assert_eq!(
            width(&t, 1) + width(&t, 2),
            80,
            "the boundary moved, not the area"
        );

        // The boundary moves, rather than the focused pane growing: asked from
        // the pane on the right, moving it right again makes that pane
        // narrower.
        assert!(t.nudge(2, Dir::Cols, 10, area));
        assert_eq!(width(&t, 1), before + 20);
        assert!(t.nudge(2, Dir::Cols, -20, area));
        assert_eq!(width(&t, 1), before);
    }

    #[test]
    fn the_boundary_stops_rather_than_closing_a_pane() {
        let area = Rect::new(0, 0, 80, 24);
        let mut t = Node::Leaf(1);
        t.split(1, Dir::Cols, 2);
        assert!(
            !t.nudge(1, Dir::Cols, 100, area),
            "it ran a pane off the screen"
        );
        assert!(!t.nudge(1, Dir::Cols, -100, area));
        // And a direction there is no boundary in does nothing at all.
        assert!(!t.nudge(1, Dir::Rows, 5, area));
    }

    #[test]
    fn the_innermost_split_is_the_one_that_moves() {
        // A column inside a row: `l` moves the column's edge, not the outer
        // one. Anything else resizes a boundary somewhere else on the screen.
        let area = Rect::new(0, 0, 80, 24);
        let mut t = Node::Leaf(1);
        t.split(1, Dir::Rows, 2);
        t.split(2, Dir::Cols, 3);
        let rect =
            |t: &Node, id: PaneId| t.rects(area).into_iter().find(|(x, _)| *x == id).unwrap().1;
        let one = rect(&t, 1);
        assert!(t.nudge(2, Dir::Cols, 6, area));
        assert_eq!(rect(&t, 1), one, "the outer split moved");
        assert_eq!(rect(&t, 2).width, 40 + 6);
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
    fn closing_a_pane_gives_its_space_to_its_siblings_and_nobody_else() {
        // A | B | C, then B split into rows, then its lower half into columns,
        // then B closed. D and E should inherit B's column; A and C should not
        // move at all.
        let mut t = leaf(1);
        t.split(1, Dir::Cols, 2);
        t.split(2, Dir::Cols, 3);
        let thirds = t.rects(area());
        let (a_before, b_before, c_before) = (thirds[0].1, thirds[1].1, thirds[2].1);

        t.split(2, Dir::Rows, 4);
        t.split(4, Dir::Cols, 5);
        t.remove(2);

        let after = t.rects(area());
        let at = |id: PaneId| after.iter().find(|(i, _)| *i == id).unwrap().1;

        assert_eq!(at(1), a_before, "A moved because something else closed");
        assert_eq!(at(3), c_before, "C moved because something else closed");
        assert_eq!(
            at(4).width + at(5).width,
            b_before.width,
            "D and E should share exactly the column B had"
        );
        assert_eq!(at(4).x, b_before.x, "and start where it started");
        assert_eq!(at(4).height, area().height, "and get its full height back");
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
