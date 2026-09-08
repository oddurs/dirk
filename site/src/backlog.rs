// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! The backlog, read as items rather than as a picture of them.
//!
//! cairn keeps structured items and renders them to `ROADMAP.md`. The site used
//! to parse that Markdown back into HTML, which is a lossy round trip: by the
//! time it arrived there was no type left, no priority, no area, no dates and
//! no dependencies — only the words cairn had chosen to write about them.
//!
//! So this reads `cairn/items` instead. The front matter is `key: value` and
//! `- item` lists, which needs no parser crate and no cairn on the path, and
//! that last part is what matters: the site builds in CI, where installing the
//! tool that owns the format would cost minutes on every push.
//!
//! The trade is the same one `palette.rs` makes with `theme.rs`, and it is
//! guarded the same way: a format this does not recognise is a hard error with
//! a message, not a page that renders empty.

use std::collections::BTreeMap;
use std::path::Path;

/// The on-disk format this knows how to read. cairn refuses to open a project
/// written in a format it does not know rather than misreading it; so does this.
const FORMAT: &str = "2";

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Item {
    pub id: u32,
    /// `0110`, the way it is written everywhere else.
    pub ref_: String,
    pub title: String,
    pub kind: String,
    pub status: String,
    /// Which milestone, by its key: `v0.5`.
    pub milestone: Option<String>,
    /// A milestone's own key. Only milestones have one.
    pub key: Option<String>,
    pub priority: Option<String>,
    pub area: Option<String>,
    pub effort: Option<String>,
    pub assignee: Option<String>,
    pub labels: Vec<String>,
    pub depends_on: Vec<u32>,
    pub due: Option<String>,
    pub updated: Option<String>,
    /// Repository-relative, so a row can link to the reasoning behind it.
    pub path: String,
    pub body: String,
}

impl Item {
    pub fn is_milestone(&self) -> bool {
        self.kind == "milestone"
    }
    /// The first paragraph of the body, which is what an item leads with.
    pub fn summary(&self) -> String {
        self.body
            .split("\n\n")
            .map(str::trim)
            .find(|p| !p.is_empty() && !p.starts_with('#') && !p.starts_with("- ["))
            .unwrap_or_default()
            .replace('\n', " ")
    }
}

/// One milestone and the items filed under it, in dependency order.
pub struct Stage {
    pub milestone: Item,
    pub items: Vec<Item>,
}

impl Stage {
    pub fn done(&self) -> usize {
        self.items.iter().filter(|i| i.status == "done").count()
    }
    pub fn open(&self) -> usize {
        self.items.len() - self.done()
    }
    /// Whole percent, and never 100 unless it is finished. A milestone with one
    /// item left rounding up to done is the one number nobody would forgive.
    pub fn percent(&self) -> u32 {
        if self.items.is_empty() {
            return 0;
        }
        if self.open() == 0 {
            return 100;
        }
        (self.done() * 100 / self.items.len()).min(99) as u32
    }
}

pub struct Backlog {
    pub stages: Vec<Stage>,
    /// Everything with an active status, whatever milestone it is under.
    pub doing: Vec<Item>,
}

pub fn read(root: &Path) -> Backlog {
    let dir = root.join("cairn/items");
    check_format(root);

    let mut items: Vec<Item> = Vec::new();
    let entries = std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            items.push(parse(&path, root));
        }
    }
    assert!(!items.is_empty(), "cairn/items parsed to nothing");
    items.sort_by_key(|i| i.id);

    // Milestones are items too, and they carry their own order through
    // `depends_on`: each one is the thing that has to be true before the next
    // is worth starting. That is the order the roadmap is read in.
    let mut milestones: Vec<Item> = items.iter().filter(|i| i.is_milestone()).cloned().collect();
    milestones.sort_by(
        |a, b| match (a.depends_on.contains(&b.id), b.depends_on.contains(&a.id)) {
            (true, false) => std::cmp::Ordering::Greater,
            (false, true) => std::cmp::Ordering::Less,
            _ => a.due.cmp(&b.due).then(a.id.cmp(&b.id)),
        },
    );

    let mut by_milestone: BTreeMap<String, Vec<Item>> = BTreeMap::new();
    for item in items.iter().filter(|i| !i.is_milestone()) {
        by_milestone
            .entry(item.milestone.clone().unwrap_or_default())
            .or_default()
            .push(item.clone());
    }

    let stages = milestones
        .into_iter()
        .map(|m| {
            let key = m.key.clone().unwrap_or_default();
            let mut items = by_milestone.remove(&key).unwrap_or_default();
            // Open work first, then done: a milestone is read for what is left.
            items.sort_by_key(|i| (i.status == "done", i.id));
            Stage {
                milestone: m,
                items,
            }
        })
        .filter(|s| !s.items.is_empty())
        .collect();

    let doing = items
        .iter()
        .filter(|i| {
            !i.is_milestone() && matches!(i.status.as_str(), "doing" | "blocked" | "review")
        })
        .cloned()
        .collect();

    Backlog { stages, doing }
}

/// A format this does not know is an error here rather than a page that
/// renders empty and looks like a project with no plans.
fn check_format(root: &Path) {
    let toml = root.join("cairn.toml");
    let text = std::fs::read_to_string(&toml).unwrap_or_else(|e| panic!("{}: {e}", toml.display()));
    let found = text
        .lines()
        .find_map(|l| l.trim().strip_prefix("format = "))
        .map(str::trim);
    assert_eq!(
        found,
        Some(FORMAT),
        "cairn.toml is format {found:?}; site/src/backlog.rs reads {FORMAT}"
    );
}

fn parse(path: &Path, root: &Path) -> Item {
    let text = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let rest = text
        .strip_prefix("---\n")
        .unwrap_or_else(|| panic!("{}: no front matter", path.display()));
    let (front, body) = rest
        .split_once("\n---\n")
        .unwrap_or_else(|| panic!("{}: front matter is not closed", path.display()));

    let mut item = Item {
        path: path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .into_owned(),
        body: body.trim().to_string(),
        ..Item::default()
    };

    // `key: value`, and `- value` continuing whichever key came last.
    let mut last = String::new();
    for line in front.lines() {
        if let Some(value) = line.trim().strip_prefix("- ") {
            let value = unquote(value);
            match last.as_str() {
                "labels" => item.labels.push(value),
                "depends_on" => item.depends_on.extend(value.parse::<u32>().ok()),
                _ => {}
            }
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (key, value) = (key.trim(), unquote(value.trim()));
        last = key.to_string();
        if value.is_empty() {
            continue;
        }
        match key {
            "id" => item.id = value.parse().unwrap_or(0),
            "title" => item.title = value,
            "type" => item.kind = value,
            "status" => item.status = value,
            "milestone" => item.milestone = Some(value),
            "key" => item.key = Some(value),
            "priority" => item.priority = Some(value),
            "area" => item.area = Some(value),
            "effort" => item.effort = Some(value),
            "assignee" => item.assignee = Some(value),
            "due" => item.due = Some(value),
            "updated" => item.updated = Some(value),
            _ => {}
        }
    }
    item.ref_ = format!("{:04}", item.id);
    item
}

/// cairn quotes a value only when it has to — a title with a colon in it.
fn unquote(value: &str) -> String {
    let value = value.trim();
    for mark in ['\'', '"'] {
        if let Some(inner) = value.strip_prefix(mark).and_then(|v| v.strip_suffix(mark)) {
            return inner.replace(&format!("{mark}{mark}"), &mark.to_string());
        }
    }
    value.to_string()
}
