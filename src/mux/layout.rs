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

//! Turning a layout in the configuration file into a split tree.
//!
//! This is planning only: it allocates pane ids and shapes the tree, and starts
//! nothing. Programs are spawned by the caller afterwards, once the tree has
//! been asked where each pane will go — otherwise every program in a layout
//! would come up at the wrong size and be resized a redraw later, which for a
//! full-screen program is a visible flash.
//!
//! The root of a layout is a `PaneDef` like any other. That is not a
//! convenience: a single-program layout and a five-pane dashboard differ only
//! in how many leaves they have, and the code should say so.

use crate::config::{LayoutDef, PaneDef};
use crate::mux::PaneId;
use crate::mux::tree::{Dir, Node};
use ratatui::layout::Constraint;

/// `"cols"` or `"rows"`; anything else is columns, which is what a wide
/// terminal wants.
fn direction(s: &str) -> Dir {
    if s.eq_ignore_ascii_case("rows") {
        Dir::Rows
    } else {
        Dir::Cols
    }
}

/// `"5"` is five lines or columns, `"30%"` is a share of the parent, and
/// nothing at all takes an even part of what is left.
pub fn constraint(size: &str) -> Option<Constraint> {
    let s = size.trim();
    if s.is_empty() {
        return Some(Constraint::Fill(1));
    }
    if let Some(pct) = s.strip_suffix('%') {
        return pct.trim().parse().ok().map(Constraint::Percentage);
    }
    s.parse().ok().map(Constraint::Length)
}

/// The root of a layout, as the pane definition it is.
fn root(def: &LayoutDef) -> PaneDef {
    PaneDef {
        title: def.name.clone(),
        command: def.command.clone(),
        split: def.split.clone(),
        pane: def.pane.clone(),
        size: String::new(),
    }
}

/// Shape the tree and say which program belongs in each leaf.
///
/// Ids are handed out from `base` in tree order, so the caller reserves a block
/// of that many and does not need to thread an allocator through the recursion.
pub fn plan(def: &LayoutDef, base: PaneId) -> (Node, Vec<(PaneId, PaneDef)>) {
    let mut next = base;
    let mut leaves = Vec::new();
    let node = plan_pane(&root(def), &mut next, &mut leaves);
    (node, leaves)
}

fn plan_pane(def: &PaneDef, next: &mut PaneId, leaves: &mut Vec<(PaneId, PaneDef)>) -> Node {
    if def.pane.is_empty() {
        let id = *next;
        *next += 1;
        leaves.push((id, def.clone()));
        return Node::Leaf(id);
    }
    let children = def
        .pane
        .iter()
        .map(|c| {
            (
                constraint(&c.size).unwrap_or(Constraint::Fill(1)),
                plan_pane(c, next, leaves),
            )
        })
        .collect();
    Node::Split {
        dir: direction(&def.split),
        children,
    }
}

/// Every size in a layout that does not parse, for reporting before the
/// terminal is taken over and the message becomes unreadable.
pub fn bad_sizes(def: &LayoutDef) -> Vec<String> {
    fn walk(def: &PaneDef, out: &mut Vec<String>) {
        if constraint(&def.size).is_none() {
            out.push(def.size.clone());
        }
        def.pane.iter().for_each(|c| walk(c, out));
    }
    let mut out = Vec::new();
    def.pane.iter().for_each(|c| walk(c, &mut out));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::layout::Rect;

    fn dashboard() -> LayoutDef {
        // brief across the top, then ptop beside spaces.
        LayoutDef {
            name: "Overview".into(),
            key: Some('1'),
            command: Vec::new(),
            split: "rows".into(),
            pane: vec![
                PaneDef {
                    title: "brief".into(),
                    command: vec!["smali".into(), "brief".into()],
                    size: "5".into(),
                    ..PaneDef::default()
                },
                PaneDef {
                    split: "cols".into(),
                    pane: vec![
                        PaneDef {
                            title: "ptop".into(),
                            command: vec!["ptop".into()],
                            ..PaneDef::default()
                        },
                        PaneDef {
                            title: "spaces".into(),
                            command: vec!["smali".into(), "tree".into()],
                            ..PaneDef::default()
                        },
                    ],
                    ..PaneDef::default()
                },
            ],
        }
    }

    #[test]
    fn a_single_program_layout_is_one_leaf() {
        let def = LayoutDef {
            name: "ptop".into(),
            command: vec!["ptop".into()],
            ..LayoutDef::default()
        };
        let (node, leaves) = plan(&def, 10);
        assert!(matches!(node, Node::Leaf(10)));
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].1.command, vec!["ptop"]);
        // The layout's name becomes the pane's label when it has only one.
        assert_eq!(leaves[0].1.title, "ptop");
    }

    #[test]
    fn a_nested_layout_gets_the_geometry_it_asked_for() {
        let (node, leaves) = plan(&dashboard(), 1);
        assert_eq!(leaves.len(), 3);

        let rects = node.rects(Rect {
            x: 0,
            y: 0,
            width: 80,
            height: 25,
        });
        let by_id = |id: PaneId| rects.iter().find(|(i, _)| *i == id).unwrap().1;

        // brief: five lines, full width.
        assert_eq!(by_id(1).height, 5);
        assert_eq!(by_id(1).width, 80);
        // ptop and spaces: side by side, sharing what is left.
        assert_eq!(by_id(2).height, 20);
        assert_eq!(by_id(3).height, 20);
        assert_eq!(by_id(2).width + by_id(3).width, 80);
        assert_eq!(by_id(2).y, 5);
    }

    #[test]
    fn ids_are_handed_out_in_tree_order_from_the_base() {
        let (node, leaves) = plan(&dashboard(), 100);
        assert_eq!(
            leaves.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![100, 101, 102]
        );
        assert_eq!(node.leaves(), vec![100, 101, 102]);
    }

    #[test]
    fn sizes_parse_the_three_ways_and_no_others() {
        assert_eq!(constraint("5"), Some(Constraint::Length(5)));
        assert_eq!(constraint("30%"), Some(Constraint::Percentage(30)));
        assert_eq!(constraint(""), Some(Constraint::Fill(1)));
        // Whitespace is forgiven: a human writing "40 %" in a config file
        // meant a percentage, and refusing it teaches nothing.
        assert_eq!(constraint("  40 % "), Some(Constraint::Percentage(40)));
        assert_eq!(constraint("half"), None);
        assert_eq!(constraint("-3"), None);
    }

    #[test]
    fn a_size_that_does_not_parse_is_reported() {
        let mut def = dashboard();
        def.pane[0].size = "half".into();
        assert_eq!(bad_sizes(&def), vec!["half"]);
        assert!(bad_sizes(&dashboard()).is_empty());
    }

    #[test]
    fn programs_are_collected_from_every_leaf() {
        assert_eq!(dashboard().programs(), vec!["smali", "ptop", "smali"]);
    }
}
