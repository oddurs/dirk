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

//! The left sidebar: layouts above, the project tree below.
//!
//! Two lists, and keeping them apart is the point. **Layouts** are named
//! arrangements with no project — one system monitor, one dashboard — so they
//! sit above the rule and never indent. **Projects** are where work happens,
//! and a workspace is one unit of it, named by `name.rs` from whatever the
//! agent inside says it is doing.
//!
//! A project with one workspace still draws both rows. Collapsing that case
//! would make the tree change shape as you work in it, and a nav that moves
//! under the pointer is worse than one extra row.

use crate::hit::{HitMap, Target};
use crate::mux::{Focus, Session};
use crate::theme::THEME;
use crate::ui::{elide, fill, heading, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(buf: &mut Buffer, area: Rect, session: &Session, hits: &mut HitMap) {
    fill(buf, area, THEME.panel());
    if area.width < 6 {
        return;
    }

    let inner = Rect {
        x: area.x + 1,
        y: area.y,
        width: area.width.saturating_sub(2),
        ..area
    };
    let w = inner.width;
    let mut y = inner.y;
    let bottom = area.y + area.height;

    let row = |y: u16| Rect {
        x: area.x,
        y,
        width: area.width,
        height: 1,
    };

    // ── Layouts ─────────────────────────────────────────────────────────
    if !session.layouts.is_empty() {
        y += 1;
        if y >= bottom {
            return;
        }
        heading(
            buf,
            Rect {
                x: inner.x,
                y,
                width: w,
                height: 1,
            },
            "layouts",
            THEME.title(),
        );
        y += 1;

        for (i, layout) in session.layouts.iter().enumerate() {
            if y >= bottom {
                return;
            }
            let focused = session.focus == Focus::Layout(i);
            let live = layout.ws.is_some();

            if focused {
                fill(buf, row(y), THEME.selected());
            }
            let base = if focused {
                THEME.selected()
            } else if live {
                THEME.text()
            } else {
                THEME.dim()
            };

            let mut x = inner.x;
            // The key that jumps here, in the accent, so the column of keys
            // reads as keys rather than as part of the name.
            if let Some(k) = layout.def.key {
                x += write_str(
                    buf,
                    x,
                    y,
                    &format!("{k} "),
                    if focused { base } else { THEME.key() },
                    w,
                );
            }
            // A dot for a layout that is open, so "running" and "not yet
            // built" are distinguishable without a second column.
            let dot = if live { "• " } else { "  " };
            x += write_str(buf, x, y, dot, if focused { base } else { THEME.ok() }, w);
            let left = w.saturating_sub(x - inner.x) as usize;
            write_str(buf, x, y, &elide(&layout.def.name, left), base, w);

            hits.push(row(y), Target::Layout(i));
            y += 1;
        }
    }

    // ── Projects ────────────────────────────────────────────────────────
    y += 1;
    if y >= bottom {
        return;
    }
    heading(
        buf,
        Rect {
            x: inner.x,
            y,
            width: w,
            height: 1,
        },
        "projects",
        THEME.title(),
    );
    y += 1;

    for (p, proj) in session.projects.iter().enumerate() {
        if y >= bottom {
            return;
        }

        let arrow = if proj.expanded { "▾ " } else { "▸ " };
        let mut x = inner.x;
        x += write_str(buf, x, y, arrow, THEME.rule_strong(), w);
        let left = w.saturating_sub(x - inner.x).saturating_sub(3) as usize;
        x += write_str(buf, x, y, &elide(&proj.name, left), THEME.project(), w);

        // The workspace count, only once it says something a glance at the
        // expanded tree would not.
        if !proj.expanded && proj.workspaces.len() > 1 {
            write_str(
                buf,
                x + 1,
                y,
                &proj.workspaces.len().to_string(),
                THEME.faint(),
                w,
            );
        }

        hits.push(row(y), Target::ProjectFold(p));
        y += 1;

        if !proj.expanded {
            continue;
        }

        for (wi, ws) in proj.workspaces.iter().enumerate() {
            if y >= bottom {
                return;
            }
            let focused = session.focus == Focus::Ws { p, w: wi };
            if focused {
                fill(buf, row(y), THEME.selected());
            }

            // The state of the agent inside, in the column the tree spine
            // would otherwise waste.
            let (glyph, gstyle) = THEME.agent_state(state_of(ws));
            let mut x = inner.x;
            x += write_str(buf, x, y, "  ", THEME.rule_strong(), w);
            x += write_str(
                buf,
                x,
                y,
                glyph,
                if focused { THEME.selected() } else { gstyle },
                w,
            );
            x += write_str(buf, x, y, " ", THEME.text(), w);

            let base = if focused {
                THEME.selected()
            } else {
                THEME.intent()
            };
            let left = w.saturating_sub(x - inner.x) as usize;
            write_str(buf, x, y, &elide(&ws.label, left), base, w);

            hits.push(row(y), Target::Workspace { p, w: wi });
            y += 1;
        }

        if y >= bottom {
            return;
        }
        write_str(buf, inner.x + 2, y, "+ workspace", THEME.faint(), w);
        hits.push(row(y), Target::NewWorkspace(p));
        y += 1;
    }

    // ── Open ────────────────────────────────────────────────────────────
    if y < bottom {
        y += 1;
    }
    if y < bottom {
        let mut x = inner.x;
        x += write_str(buf, x, y, "o", THEME.key(), w);
        write_str(buf, x, y, " open project", THEME.dim(), w);
        hits.push(row(y), Target::OpenProject);
    }
}

/// A workspace's state is its active pane's, since a workspace with one pane is
/// the common case and a workspace with two has no single answer anyway.
fn state_of(ws: &crate::mux::Workspace) -> &'static str {
    match ws.active_pane() {
        None => "unknown",
        Some(p) if p.dead => "idle",
        // Without an agent protocol dirk cannot tell working from blocked yet;
        // a pane that has published an intent is doing something, and one that
        // has not is a shell. Real states arrive with the agent integration.
        Some(p) if p.title().is_some() => "working",
        Some(_) => "idle",
    }
}
