// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! The palette, read out of the program.
//!
//! `src/theme.rs` is the definition of Gotham for dirk: a ramp of constants,
//! and a method per job a colour does. This module parses both and emits them
//! as custom properties, so the site cannot hold a colour the program does not.
//!
//! It parses Rust with regular string matching rather than a syntax tree. That
//! is a deliberate trade: `theme.rs` is a flat file of two shapes and has been
//! for its whole life, and a parser dependency to read six lines of it would
//! cost more than it saves. The guard against the trade going wrong is that a
//! role naming a constant which no longer exists is a hard error, not a
//! silently missing variable.

use std::collections::BTreeMap;
use std::fmt::Write as _;

/// One entry of the Gotham ramp: `BASE0`, `ACCENT`, `RAIL`.
struct Ramp {
    hex: String,
    /// The trailing comment, if the constant carries one. It is the best
    /// available description of what the colour is for, and it is written by
    /// whoever chose the colour.
    note: Option<String>,
}

/// One semantic role: a method on `Theme`, and the constants it reaches for.
struct Role {
    fg: Option<String>,
    bg: Option<String>,
    bold: bool,
}

pub struct Palette {
    ramp: BTreeMap<String, Ramp>,
    roles: BTreeMap<String, Role>,
}

impl Palette {
    /// Read `src/theme.rs`.
    pub fn read(theme_rs: &str) -> Palette {
        Palette {
            ramp: ramp(theme_rs),
            roles: roles(theme_rs),
        }
    }

    /// The dark palette, as CSS. Every value here came out of the program.
    pub fn to_css(&self) -> String {
        let mut css = String::new();
        css.push_str(
            "/* Generated from src/theme.rs by `cargo run -p site`. Do not edit.\n \
             *\n \
             * Gotham, as the program defines it. A colour that changes there\n \
             * changes here, because there is nowhere else for it to be. */\n\n",
        );

        css.push_str(":root {\n  /* ── The ramp ──────────────────────────── */\n");
        for (name, ramp) in &self.ramp {
            let var = name.to_lowercase().replace('_', "-");
            match &ramp.note {
                Some(note) => {
                    let _ = writeln!(css, "  --gotham-{var}: {}; /* {note} */", ramp.hex);
                }
                None => {
                    let _ = writeln!(css, "  --gotham-{var}: {};", ramp.hex);
                }
            }
        }

        css.push_str("\n  /* ── The jobs a colour does ────────────── */\n");
        for (name, role) in &self.roles {
            let var = name.replace('_', "-");
            if let Some(fg) = &role.fg {
                let _ = writeln!(css, "  --{var}: {};", self.reference(name, fg));
            }
            if let Some(bg) = &role.bg {
                let suffix = if role.fg.is_some() { "-bg" } else { "" };
                let _ = writeln!(css, "  --{var}{suffix}: {};", self.reference(name, bg));
            }
            if role.bold {
                let _ = writeln!(css, "  --{var}-weight: 600;");
            }
        }
        css.push_str("}\n");
        css
    }

    /// A role's constant, as a `var()` — and a hard error if the constant it
    /// names has gone. Silently emitting `var(--gotham-base9)` would give a
    /// page with one invisible word on it and nothing to explain why.
    fn reference(&self, role: &str, konst: &str) -> String {
        assert!(
            self.ramp.contains_key(konst),
            "theme.rs: Theme::{role} uses {konst}, which is not a constant in the ramp",
        );
        format!("var(--gotham-{})", konst.to_lowercase().replace('_', "-"))
    }

    pub fn is_empty(&self) -> bool {
        self.ramp.is_empty() || self.roles.is_empty()
    }
}

/// `const BASE0: Color = Color::Rgb(0x0a, 0x0f, 0x14); // panel ground`
fn ramp(src: &str) -> BTreeMap<String, Ramp> {
    let mut out = BTreeMap::new();
    for line in src.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("const ") else {
            continue;
        };
        let Some((name, rest)) = rest.split_once(':') else {
            continue;
        };
        let Some(args) = rest.split_once("Color::Rgb(").map(|(_, a)| a) else {
            continue;
        };
        let Some((args, tail)) = args.split_once(')') else {
            continue;
        };

        let channels: Vec<u8> = args
            .split(',')
            .filter_map(|c| {
                let c = c.trim();
                u8::from_str_radix(c.trim_start_matches("0x"), 16).ok()
            })
            .collect();
        if channels.len() != 3 {
            continue;
        }

        let note = tail
            .split_once("//")
            .map(|(_, n)| n.trim().to_string())
            .filter(|n| !n.is_empty());

        out.insert(
            name.trim().to_string(),
            Ramp {
                hex: format!("#{:02x}{:02x}{:02x}", channels[0], channels[1], channels[2]),
                note,
            },
        );
    }
    out
}

/// `pub fn dim(self) -> Style { Style::default().fg(BASE6) }`
///
/// Methods whose body is a `match` — the state and priority tables — are not
/// one colour and are skipped. The site reaches for the roles they are built
/// out of instead.
fn roles(src: &str) -> BTreeMap<String, Role> {
    let mut out = BTreeMap::new();
    let mut lines = src.lines().peekable();

    while let Some(line) = lines.next() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub fn ") else {
            continue;
        };
        let Some((name, sig)) = rest.split_once('(') else {
            continue;
        };
        if !sig.contains("self)") || !sig.contains("-> Style") {
            continue;
        }

        // The body is the next line, in a file `cargo fmt` decides the shape of.
        let Some(body) = lines.peek() else { continue };
        let body = body.trim();
        if body.starts_with("match") || !body.starts_with("Style::default()") {
            continue;
        }

        let pick = |call: &str| -> Option<String> {
            let arg = body.split_once(call)?.1.split_once(')')?.0.trim();
            arg.chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
                .then(|| arg.to_string())
        };

        out.insert(
            name.trim().to_string(),
            Role {
                fg: pick(".fg("),
                bg: pick(".bg("),
                bold: body.contains("Modifier::BOLD"),
            },
        );
    }
    out
}
