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

//! The project picker: the one overlay in dirk.
//!
//! `~/Code` has ninety directories in it and the sidebar lists three, because
//! the sidebar is what is *open*. This is how something gets opened — type to
//! filter, Enter to take the top match, click to take any of them.
//!
//! Directories are read once when the picker opens rather than watched. It is
//! a list of repositories, not a live view of a filesystem.

use crate::hit::{HitMap, Target};
use crate::theme::THEME;
use crate::ui::{elide, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use std::path::{Path, PathBuf};

pub struct Picker {
    pub query: String,
    entries: Vec<(String, PathBuf)>,
    pub selected: usize,
}

impl Picker {
    pub fn new(root: &Path) -> Self {
        let mut entries: Vec<(String, PathBuf)> = std::fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                // Dotfiles are configuration, not projects.
                (!name.starts_with('.')).then(|| (name, e.path()))
            })
            .collect();
        // Cached, not plain sort_by_key: the key allocates, and sort_by_key
        // would recompute it on every comparison.
        entries.sort_by_cached_key(|(name, _)| name.to_lowercase());
        Self {
            query: String::new(),
            entries,
            selected: 0,
        }
    }

    /// Subsequence match, the way every fuzzy finder works: "dsy" finds
    /// "design-system". Ranked by how early the match starts, so an exact
    /// prefix beats a scattered hit.
    pub fn matches(&self) -> Vec<(usize, &(String, PathBuf))> {
        let q = self.query.to_lowercase();
        let mut v: Vec<(usize, usize, &(String, PathBuf))> = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| score(&e.0.to_lowercase(), &q).map(|s| (s, i, e)))
            .collect();
        v.sort_by_key(|(s, i, _)| (*s, *i));
        v.into_iter().map(|(_, i, e)| (i, e)).collect()
    }

    pub fn path(&self, index: usize) -> Option<&Path> {
        self.entries.get(index).map(|e| e.1.as_path())
    }

    pub fn move_by(&mut self, delta: isize) {
        let n = self.matches().len();
        if n == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(n as isize) as usize;
    }
}

fn score(name: &str, query: &str) -> Option<usize> {
    if query.is_empty() {
        return Some(0);
    }
    let mut chars = name.char_indices();
    let mut first = None;
    for q in query.chars() {
        let hit = chars.find(|(_, c)| *c == q)?;
        first.get_or_insert(hit.0);
    }
    Some(first.unwrap_or(0))
}

pub fn render(buf: &mut Buffer, area: Rect, picker: &Picker, hits: &mut HitMap) {
    // Centred, and small: a picker that fills the screen hides the thing you
    // are picking for.
    let w = area.width.clamp(20, 56).min(area.width);
    let h = area.height.clamp(3, 18).min(area.height);
    let box_area = Rect {
        x: area.x + (area.width - w) / 2,
        y: area.y + (area.height - h) / 3,
        width: w,
        height: h,
    };

    fill(buf, box_area, THEME.panel());

    let inner_w = w.saturating_sub(4);
    let x = box_area.x + 2;

    let prompt = format!("open  {}", picker.query);
    write_str(buf, x, box_area.y, &prompt, THEME.text(), inner_w);
    // A block cursor, since the real one is parked on a pane behind this.
    write_str(
        buf,
        x + prompt.chars().count() as u16,
        box_area.y,
        "▌",
        THEME.working(),
        inner_w,
    );

    for col in 0..inner_w {
        write_str(
            buf,
            x + col,
            box_area.y + 1,
            "─",
            THEME.rule_strong(),
            inner_w,
        );
    }

    let rows = h.saturating_sub(2);
    for (row, (index, entry)) in picker.matches().into_iter().take(rows as usize).enumerate() {
        let y = box_area.y + 2 + row as u16;
        let full = Rect {
            x: box_area.x,
            y,
            width: w,
            height: 1,
        };
        let selected = row == picker.selected;
        if selected {
            fill(buf, full, THEME.selected());
        }
        let style = if selected {
            THEME.selected()
        } else {
            THEME.dim()
        };
        write_str(
            buf,
            x,
            y,
            &elide(&entry.0, inner_w as usize),
            style,
            inner_w,
        );
        hits.push(full, Target::PickerRow(index));
    }
}
