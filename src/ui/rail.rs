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

//! The bottom bar: the one surface that runs the full width of the terminal.
//!
//! It is the only thing in dirk that spans sidebar and panes both, which is
//! what makes it the footer rather than a pane: a pane can only ever start
//! where the sidebar ends. One row, always, in every state — a footer that
//! disappears when there is nothing to say is not a footer.
//!
//! Three jobs, left to right, and they are the reason it earns a row of every
//! screen: **where am I** (the brand, and it is the only place branding
//! appears), **where can I go** (a chip per workspace, clickable, focused one
//! filled), and **what is going on** (counts and the clock).

use crate::config::Config;
use crate::hit::{HitMap, Target};
use crate::mux::{Focus, Session};
use crate::theme::THEME;
use crate::ui::{elide, fill, write_str};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub fn render(
    buf: &mut Buffer,
    area: Rect,
    cfg: &Config,
    session: &Session,
    clock: &str,
    status: &str,
    hits: &mut HitMap,
) {
    fill(buf, area, THEME.rail());
    if area.width < 20 {
        return;
    }

    // ── Brand ───────────────────────────────────────────────────────────
    let mut x = area.x + 1;
    let b = &cfg.brand;
    if !b.mark.is_empty() {
        x += write_str(buf, x, area.y, &b.mark, THEME.working(), area.width);
        x += write_str(buf, x, area.y, " ", THEME.rail(), area.width);
    }
    x += write_str(
        buf,
        x,
        area.y,
        &b.name,
        THEME.text().patch(THEME.rail()),
        area.width,
    );

    // ── Right edge, measured before the chips so they cannot overrun it ──
    let right = {
        let mut parts: Vec<String> = Vec::new();
        if !status.is_empty() {
            parts.push(status.to_string());
        }
        let n = session
            .projects
            .iter()
            .map(|p| p.workspaces.len())
            .sum::<usize>();
        parts.push(format!("{n} {}", if n == 1 { "space" } else { "spaces" }));
        parts.push(clock.to_string());
        parts.join("  ·  ")
    };
    let right_w = right.chars().count() as u16;
    let right_x = area.x + area.width.saturating_sub(right_w + 1);
    write_str(
        buf,
        right_x,
        area.y,
        &right,
        THEME.dim().patch(THEME.rail()),
        right_w,
    );

    // ── Chips ───────────────────────────────────────────────────────────
    // Every workspace in tree order. The bar is a jump target, so the chips
    // are numbered by position the way the sidebar reads top to bottom.
    x += 2;
    let limit = right_x.saturating_sub(2);
    for (n, (p, w)) in session.flat().into_iter().enumerate() {
        let Some(ws) = session.workspace(p, w) else {
            continue;
        };
        let focused = session.focus == Focus::Ws { p, w };

        let label = elide(&ws.label, 14);
        let text = format!("{} {}", n + 1, label);
        let width = text.chars().count() as u16 + 2;
        if x + width > limit {
            break;
        }

        // A filled bar for the focused chip, a thin one otherwise: the same
        // distinction the sidebar makes with its selection row, in one column.
        let (bar, style) = if focused {
            ("▊", THEME.text().patch(THEME.rail()))
        } else {
            ("▏", THEME.dim().patch(THEME.rail()))
        };
        let mut cx = x;
        cx += write_str(
            buf,
            cx,
            area.y,
            bar,
            THEME.working().patch(THEME.rail()),
            limit - x,
        );
        write_str(buf, cx, area.y, &text, style, limit - cx);

        hits.push(
            Rect {
                x,
                y: area.y,
                width,
                height: 1,
            },
            Target::Workspace { p, w },
        );
        x += width;
    }
}
