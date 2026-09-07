// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! A Markdown file, and what it becomes.
//!
//! Front matter is TOML between `+++` fences. Everything after it is prose,
//! and prose goes through `pulldown-cmark` — the hard part of Markdown is a
//! library. What this module owns is what happens either side of it: headings
//! that can be linked to, a table of contents taken from the same pass rather
//! than a second one that can disagree, and the two fences that mean something
//! here.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd, html};
use serde::Deserialize;
use std::collections::BTreeMap;

#[derive(Debug, Default, Deserialize)]
pub struct Front {
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Which group in the navigation, if any. A page with no section is
    /// reachable but not listed — which is what a 404 page wants to be.
    #[serde(default)]
    pub section: Option<String>,
    /// Position within the section. Absent sorts last, then by title.
    #[serde(default)]
    pub order: Option<i64>,
    /// The template to render into. Defaults to `page.html`.
    #[serde(default)]
    pub template: Option<String>,
    /// Off for pages whose own headings are the page — the index.
    #[serde(default)]
    pub toc: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct Heading {
    pub id: String,
    pub text: String,
    pub level: u8,
}

pub struct Page {
    pub front: Front,
    /// Site-rooted and without a trailing file name: `/docs/install/`.
    pub url: String,
    pub html: String,
    pub toc: Vec<Heading>,
}

/// Split `+++ … +++` off the front of a file.
///
/// A page with no front matter is an error rather than a page titled after its
/// file name: a title is the one thing every page needs, and guessing at it is
/// how a site ends up with a tab called `index`.
pub fn split_front(source: &str, path: &str) -> (Front, String) {
    let rest = source
        .strip_prefix("+++")
        .unwrap_or_else(|| panic!("{path}: no `+++` front matter"));
    let (front, body) = rest
        .split_once("+++")
        .unwrap_or_else(|| panic!("{path}: front matter is not closed with `+++`"));
    let front: Front =
        toml::from_str(front).unwrap_or_else(|e| panic!("{path}: front matter: {e}"));
    (front, body.trim_start().to_string())
}

/// Markdown to HTML, with the two fences that are not code.
///
/// ```` ```shot <name> ```` inlines a terminal render from `site/shots`, and
/// ```` ```callout <kind> ```` opens an aside. Both are fences rather than
/// shortcodes because a fence is still legible when the file is read as plain
/// Markdown, which is how it is read in a pull request.
pub fn to_html(markdown: &str, shots: &BTreeMap<String, String>) -> (String, Vec<Heading>) {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_SMART_PUNCTUATION;

    let events: Vec<Event> = Parser::new_ext(markdown, options).collect();
    let mut out: Vec<Event> = Vec::with_capacity(events.len());
    let mut toc: Vec<Heading> = Vec::new();
    let mut seen: BTreeMap<String, usize> = BTreeMap::new();

    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            // ── Headings: an id, and a link to it ────────────────────────
            Event::Start(Tag::Heading { level, .. }) => {
                let level = *level;
                let mut text = String::new();
                let mut j = i + 1;
                while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                    if let Event::Text(t) | Event::Code(t) = &events[j] {
                        text.push_str(t);
                    }
                    j += 1;
                }

                let id = unique(&slug(&text), &mut seen);
                let tag = heading_tag(level);
                out.push(Event::Html(format!("<{tag} id=\"{id}\">").into()));
                out.extend(events[i + 1..j].iter().cloned());
                // The anchor is after the text and hidden until the heading is
                // hovered: a column of § down the left margin is chrome
                // pretending to be content.
                out.push(Event::Html(
                    format!(
                        "<a class=\"anchor\" href=\"#{id}\" aria-label=\"Link to this section\">#</a></{tag}>"
                    )
                    .into(),
                ));

                if (2..=3).contains(&heading_number(level)) {
                    toc.push(Heading {
                        id,
                        text,
                        level: heading_number(level),
                    });
                }
                i = j + 1;
                continue;
            }

            // ── Fences ───────────────────────────────────────────────────
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(info))) => {
                let info = info.to_string();
                let mut body = String::new();
                let mut j = i + 1;
                while j < events.len() && !matches!(events[j], Event::End(TagEnd::CodeBlock)) {
                    if let Event::Text(t) = &events[j] {
                        body.push_str(t);
                    }
                    j += 1;
                }

                let mut words = info.split_whitespace();
                match (words.next(), words.next()) {
                    (Some("shot"), name) => {
                        let name = name.unwrap_or("").trim();
                        let render = shots.get(name).unwrap_or_else(|| {
                            panic!(
                                "no terminal render called `{name}` in site/shots; \
                                 `make shots` writes them"
                            )
                        });
                        out.push(Event::Html(render.clone().into()));
                    }
                    (Some("callout"), kind) => {
                        let kind = kind.unwrap_or("note");
                        out.push(Event::Html(
                            format!(
                                "<aside class=\"callout\" data-kind=\"{kind}\">{}</aside>",
                                inline(&body, options)
                            )
                            .into(),
                        ));
                    }
                    (lang, _) => {
                        let lang = lang.unwrap_or("text");
                        out.push(Event::Html(
                            format!(
                                "<figure class=\"code\" data-lang=\"{lang}\"><pre><code>{}</code></pre></figure>",
                                escape(&body)
                            )
                            .into(),
                        ));
                    }
                }
                i = j + 1;
                continue;
            }

            _ => out.push(events[i].clone()),
        }
        i += 1;
    }

    let mut html = String::with_capacity(markdown.len() * 2);
    html::push_html(&mut html, out.into_iter());
    (html, toc)
}

/// Markdown inside a construct that is already a block — a callout's body.
fn inline(markdown: &str, options: Options) -> String {
    let mut html = String::new();
    html::push_html(&mut html, Parser::new_ext(markdown, options));
    html
}

fn heading_number(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn heading_tag(level: HeadingLevel) -> String {
    format!("h{}", heading_number(level))
}

pub fn slug(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

/// Two headings with the same words are ordinary; two anchors with the same id
/// mean one of them cannot be linked to.
fn unique(base: &str, seen: &mut BTreeMap<String, usize>) -> String {
    let n = seen.entry(base.to_string()).or_insert(0);
    *n += 1;
    if *n == 1 {
        base.to_string()
    } else {
        format!("{base}-{n}")
    }
}

pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}
