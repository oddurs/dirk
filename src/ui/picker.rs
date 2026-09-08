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

use crate::glyph::G;
use crate::hit::{HitMap, Target};
use crate::theme::THEME;
use crate::ui::elide;
use crate::ui::overlay::{self, Overlay};
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

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    g: &crate::glyph::Glyphs,
    picker: &Picker,
    hits: &mut HitMap,
) {
    // Centred, and small: a picker that fills the screen hides the thing you
    // are picking for.
    let all = picker.matches();
    let at = overlay::centred(area, (20, 56), (3, 18), 3);
    let panel = Overlay::open(
        buf,
        g,
        at,
        &format!("open  {}", picker.query),
        picker.selected,
        all.len(),
    );

    for row in panel.showing(all.len()) {
        let (index, entry) = &all[row];
        let mut strip = panel.strip(
            buf,
            hits,
            row - panel.first,
            row == picker.selected,
            Target::PickerRow(*index),
        );
        let style = match strip.chosen {
            true => THEME.selected(),
            false => THEME.dim(),
        };
        let room = strip.room as usize;
        strip.write(buf, &elide(&entry.0, room, g.text(G::Ellipsis)), style);
    }
}
