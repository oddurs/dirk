// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! Templates.
//!
//! One base, and pages that fill it in. Inheritance is the only feature this
//! needs and the only reason there is a template engine here at all.

use minijinja::{Environment, Value, context};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
pub struct NavLink {
    pub title: String,
    pub url: String,
}

#[derive(Serialize)]
pub struct NavSection {
    pub title: String,
    pub links: Vec<NavLink>,
}

#[derive(Serialize)]
pub struct Site {
    pub title: String,
    pub tagline: String,
    pub description: String,
    pub repo: String,
    pub base: String,
    /// Scheme and host, for the links that have to be absolute.
    pub origin: String,
    pub version: String,
    pub nav: Vec<NavSection>,
    /// The year, for the footer. Taken from the build rather than hard-coded,
    /// so a copyright line does not quietly become wrong.
    pub year: String,
}

pub struct Templates<'a> {
    env: Environment<'a>,
}

impl Templates<'_> {
    pub fn load(dir: &Path) -> Self {
        let mut env = Environment::new();
        // Autoescaping is off: everything reaching a template has already been
        // through the Markdown renderer, which escapes what it must and emits
        // HTML on purpose. Escaping it a second time prints the tags.
        env.set_auto_escape_callback(|_| minijinja::AutoEscape::None);

        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
            .filter_map(Result::ok);
        for entry in entries {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "html") {
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .expect("template name")
                    .to_string();
                let source = std::fs::read_to_string(&path)
                    .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
                env.add_template_owned(name, source).expect("add template");
            }
        }
        Templates { env }
    }

    pub fn render(&self, template: &str, site: &Site, page: Value) -> String {
        self.env
            .get_template(template)
            .unwrap_or_else(|e| panic!("template {template}: {e}"))
            .render(context! { site => Value::from_serialize(site), page })
            .unwrap_or_else(|e| panic!("rendering {template}: {e:#}"))
    }
}
