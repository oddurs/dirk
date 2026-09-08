// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! The generator that builds dirk's website.
//!
//!     cargo run -p site -- build [--base /dirk/] [--out site/dist]
//!     cargo run -p site -- serve [--port 1111]
//!
//! It is a cargo target rather than an off-the-shelf generator because half of
//! what belongs on the site is generated from the program: the palette from
//! `src/theme.rs`, the roadmap from cairn's rendered file, the changelog from
//! NEWS, and the screens from the real binary. A tool that reads Markdown out
//! of a directory would need a build step in front of it for all of that, and
//! then there are two tools where there was one.
//!
//! It fails by panicking with a message. That is the right behaviour for a
//! build tool someone is watching: the first broken thing stops the build and
//! says which file it was in.

mod backlog;
mod content;
mod palette;
mod render;
mod serve;

use render::{NavLink, NavSection, Site, Templates};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Everything the site knows about itself that is not in a file.
const TITLE: &str = "dirk";
const TAGLINE: &str = "A terminal multiplexer that knows what its sessions are for.";
const REPO: &str = "https://github.com/oddurs/dirk";
/// Scheme and host. A canonical link and a sitemap entry are absolute or they
/// are not doing their job, and neither can be worked out from a base path.
/// `--origin` overrides it, so moving to a domain is two flags and a CNAME
/// file in `site/assets`, rather than a search through the templates.
const ORIGIN: &str = "https://oddurs.github.io";

fn main() {
    let mut args = std::env::args().skip(1);
    let command = args.next().unwrap_or_else(|| "build".into());
    let rest: Vec<String> = args.collect();

    match command.as_str() {
        "build" => {
            let base = flag(&rest, "--base").unwrap_or_else(|| "/".into());
            let origin = flag(&rest, "--origin").unwrap_or_else(|| ORIGIN.into());
            let out = flag(&rest, "--out").map(PathBuf::from);
            let dist = build_with(
                &normalise(&base),
                origin.trim_end_matches('/'),
                out.as_deref(),
            );
            eprintln!("built {}", dist.display());
        }
        "serve" => {
            let port: u16 = flag(&rest, "--port")
                .unwrap_or_else(|| "1111".into())
                .parse()
                .expect("--port takes a number");
            serve::run(port, || build("/", None));
        }
        "tokens" => {
            print!("{}", read_palette(&root()).to_css());
        }
        other => {
            eprintln!("site: unknown command `{other}`\n");
            eprintln!("  build [--base /dirk/] [--origin https://host] [--out site/dist]");
            eprintln!("  serve [--port 1111]");
            eprintln!("  tokens");
            std::process::exit(2);
        }
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// A base path is `/` or `/something/`. Getting either end wrong produces a
/// site whose links all work locally and none work when deployed, which is the
/// single most common way to ship a broken static site.
fn normalise(base: &str) -> String {
    let trimmed = base.trim_matches('/');
    if trimmed.is_empty() {
        "/".into()
    } else {
        format!("/{trimmed}/")
    }
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("site/ has a parent")
        .to_path_buf()
}

fn read_palette(root: &Path) -> palette::Palette {
    let theme = root.join("src/theme.rs");
    let source =
        std::fs::read_to_string(&theme).unwrap_or_else(|e| panic!("{}: {e}", theme.display()));
    let palette = palette::Palette::read(&source);
    assert!(
        !palette.is_empty(),
        "src/theme.rs parsed to nothing; the shape it is written in has changed"
    );
    palette
}

// ─── the build ───────────────────────────────────────────────────────────────

/// The default build: the site as it is deployed.
pub fn build(base: &str, out: Option<&Path>) -> PathBuf {
    build_with(base, ORIGIN, out)
}

pub fn build_with(base: &str, origin: &str, out: Option<&Path>) -> PathBuf {
    let root = root();
    let site_dir = root.join("site");
    let dist = out
        .map(Path::to_path_buf)
        .unwrap_or_else(|| site_dir.join("dist"));

    let _ = std::fs::remove_dir_all(&dist);
    std::fs::create_dir_all(&dist).expect("create dist");

    // ── generated from the program ───────────────────────────────────────
    let css_dir = dist.join("css");
    std::fs::create_dir_all(&css_dir).expect("create css dir");
    write(&css_dir.join("tokens.css"), &read_palette(&root).to_css());

    let shots = read_shots(&site_dir.join("shots"));

    // ── prose ────────────────────────────────────────────────────────────
    let mut pages = read_content(&site_dir.join("content"), &shots, base);
    pages.extend(generated_pages(&root, &shots, base));

    // ── navigation, from the pages themselves ────────────────────────────
    let nav = navigation(&pages);
    let site = Site {
        title: TITLE.into(),
        tagline: TAGLINE.into(),
        description: TAGLINE.into(),
        repo: REPO.into(),
        base: base.to_string(),
        origin: origin.to_string(),
        version: version(&root),
        nav,
        year: "2026".into(),
    };

    let templates = Templates::load(&site_dir.join("templates"));

    // The backlog is not prose and does not go through the Markdown pipeline.
    // It is a set of items with types, priorities and dates, and the page is
    // those fields — so it is built from the files cairn keeps rather than
    // from the Markdown cairn writes about them.
    write_page(
        &dist,
        "/roadmap/",
        &roadmap(&templates, &site, &backlog::read(&root)),
    );

    for page in &pages {
        let template = page.front.template.as_deref().unwrap_or("page.html");
        let html = templates.render(
            template,
            &site,
            minijinja::context! {
                title => page.front.title,
                description => page.front.description,
                section => page.front.section,
                url => page.url,
                toc => if page.front.toc.unwrap_or(true) { &page.toc } else { &[][..] },
                content => page.html,
            },
        );
        write_page(&dist, &page.url, &html);
    }

    copy_tree(&site_dir.join("assets"), &dist);
    write(&dist.join("sitemap.xml"), &sitemap(&pages, origin, base));
    write(
        &dist.join("robots.txt"),
        &format!("User-agent: *\nAllow: /\nSitemap: {origin}{base}sitemap.xml\n"),
    );
    // Pages runs Jekyll over an upload unless told not to, and Jekyll drops
    // every directory whose name begins with an underscore.
    write(&dist.join(".nojekyll"), "");

    dist
}

/// The roadmap page.
fn roadmap(templates: &Templates, site: &Site, backlog: &backlog::Backlog) -> String {
    let stages: Vec<_> = backlog
        .stages
        .iter()
        .map(|s| {
            minijinja::context! {
                key => s.milestone.key.clone().unwrap_or_default(),
                title => s.milestone.title,
                summary => s.milestone.summary(),
                due => s.milestone.due,
                percent => s.percent(),
                done => s.done(),
                total => s.items.len(),
                items => s.items,
            }
        })
        .collect();

    templates.render(
        "roadmap.html",
        site,
        minijinja::context! {
            title => "Roadmap",
            description => "What is done, what is next, and the reasoning behind each item.",
            section => "Project",
            url => "/roadmap/",
            toc => Vec::<content::Heading>::new(),
            stages => stages,
            doing => backlog.doing,
        },
    )
}

fn version(root: &Path) -> String {
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).unwrap_or_default();
    manifest
        .lines()
        .find_map(|l| l.strip_prefix("version = \""))
        .and_then(|v| v.split('"').next())
        .unwrap_or("0.0.0")
        .to_string()
}

// ─── content ─────────────────────────────────────────────────────────────────

fn read_content(dir: &Path, shots: &BTreeMap<String, String>, base: &str) -> Vec<content::Page> {
    let mut pages = Vec::new();
    let mut stack = vec![dir.to_path_buf()];

    while let Some(current) = stack.pop() {
        let entries =
            std::fs::read_dir(&current).unwrap_or_else(|e| panic!("{}: {e}", current.display()));
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                pages.push(page_from(&path, dir, shots, base));
            }
        }
    }

    pages.sort_by(|a, b| a.url.cmp(&b.url));
    pages
}

fn page_from(
    path: &Path,
    content_dir: &Path,
    shots: &BTreeMap<String, String>,
    base: &str,
) -> content::Page {
    let source =
        std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let name = path.display().to_string();
    let (front, body) = content::split_front(&source, &name);
    let (html, toc) = content::to_html(&body, shots);

    content::Page {
        front,
        url: url_for(path, content_dir),
        html: rebase(&html, base),
        toc,
    }
}

/// `content/docs/install.md` is `/docs/install/`; `content/index.md` is `/`.
///
/// Directories rather than `.html` files, so a URL never has to be renamed when
/// the generator changes and never carries an extension a reader has to type.
fn url_for(path: &Path, content_dir: &Path) -> String {
    let relative = path.strip_prefix(content_dir).expect("under content/");
    let stem = relative.with_extension("");
    let stem = stem.to_string_lossy().replace('\\', "/");
    if stem == "index" {
        "/".into()
    } else {
        format!("/{}/", stem.trim_end_matches("/index"))
    }
}

/// Site-rooted links written in Markdown have to survive being deployed under
/// a prefix. Templates already know the base; prose does not, so it is applied
/// here rather than asked of whoever is writing a page.
fn rebase(html: &str, base: &str) -> String {
    if base == "/" {
        return html.to_string();
    }
    html.replace("href=\"/", &format!("href=\"{base}"))
        .replace("src=\"/", &format!("src=\"{base}"))
}

/// The pages that are not written, but read out of the repository.
fn generated_pages(
    root: &Path,
    shots: &BTreeMap<String, String>,
    base: &str,
) -> Vec<content::Page> {
    let mut pages = Vec::new();

    if let Ok(news) = std::fs::read_to_string(root.join("NEWS")) {
        let (html, toc) = content::to_html(&news_to_markdown(&news), shots);
        pages.push(content::Page {
            front: content::Front {
                title: "Changelog".into(),
                description: "Every release, and what changed in it.".into(),
                section: Some("Project".into()),
                order: Some(20),
                ..Default::default()
            },
            url: "/changelog/".into(),
            html: rebase(&html, base),
            toc,
        });
    }

    pages
}

/// NEWS is GNU plain text: `*` opens a release and `**` opens a heading under
/// it. Nothing else about it needs translating — it was written to be read.
///
/// What it does need is a way in. Twenty-five kilobytes of releases with no
/// index means reaching a version by scrolling past every version after it, so
/// the releases are collected first and an index is written from them. Each
/// one already opens with a sentence saying what it is, which is exactly the
/// summary that index wants.
fn news_to_markdown(news: &str) -> String {
    struct Release {
        version: String,
        date: String,
        summary: String,
        /// The opening paragraph ends at the blank line after it, or at the
        /// first heading. Testing whether the body is empty does not work: the
        /// blank line between the release and its first sentence is already in
        /// the body by then.
        summary_done: bool,
        body: String,
    }
    let mut releases: Vec<Release> = Vec::new();

    for line in news.lines() {
        // The copying notice at the foot of the file is a licence, not news.
        if line.starts_with("---------") {
            break;
        }
        if let Some(rest) = line.strip_prefix("* Noteworthy changes in release ") {
            let (version, date) = rest.split_once(" (").unwrap_or((rest, ""));
            releases.push(Release {
                version: version.trim().to_string(),
                date: date.trim_end_matches(')').trim().to_string(),
                summary: String::new(),
                summary_done: false,
                body: String::new(),
            });
            continue;
        }
        let Some(release) = releases.last_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("** ") {
            // A release that opens straight into a heading has no paragraph to
            // take. The heading is what it is about, which is the next best
            // thing and better than a blank cell.
            if !release.summary_done && release.summary.is_empty() {
                release.summary = rest.trim().to_string();
            }
            release.summary_done = true;
            release.body.push_str(&format!("### {rest}\n"));
            continue;
        }
        // The first paragraph is what the release is. Everything after it
        // belongs to a heading, not to the release.
        if !release.summary_done {
            match line.trim().is_empty() {
                true => release.summary_done = !release.summary.is_empty(),
                false => {
                    if !release.summary.is_empty() {
                        release.summary.push(' ');
                    }
                    release.summary.push_str(line.trim());
                }
            }
        }
        release.body.push_str(line);
        release.body.push('\n');
    }

    let mut out = String::with_capacity(news.len() + 1024);
    out.push_str("| | | |\n| --- | --- | --- |\n");
    for r in &releases {
        let anchor = content::slug(&format!("{} {}", r.version, r.date));
        out.push_str(&format!(
            "| [**{}**](#{anchor}) | {} | {} |\n",
            r.version,
            r.date,
            first_sentence(&r.summary),
        ));
    }
    for r in &releases {
        out.push_str(&format!("\n## {} — {}\n{}", r.version, r.date, r.body));
    }
    out
}

/// Enough of the opening to say what a release was, and no more.
///
/// Sentences, not characters, so it never stops mid-word — but a first
/// sentence of one word ("Sessions.") says nothing, so it keeps taking them
/// until there is something to read or there is no more room.
fn first_sentence(text: &str) -> String {
    const ENOUGH: usize = 32;
    const TOO_MUCH: usize = 130;

    // The first sentence boundary past the point where it has said something.
    for (at, _) in text.match_indices(". ") {
        if at + 1 >= ENOUGH {
            return text[..at + 1].trim().to_string();
        }
    }
    // No boundary at all. A heading is the usual reason, and a heading is
    // short — so only something genuinely long gets cut, and it gets cut at a
    // word and says so.
    if text.chars().count() <= TOO_MUCH {
        return text.trim().to_string();
    }
    let limit = text
        .char_indices()
        .map(|(i, _)| i)
        .take_while(|&i| i <= TOO_MUCH)
        .last()
        .unwrap_or(0);
    let cut = text[..limit].rfind(' ').unwrap_or(limit);
    format!("{}…", text[..cut].trim())
}

// ─── navigation ──────────────────────────────────────────────────────────────

/// Sections in the order they are first met, pages within them by `order` then
/// title. Declared in the pages rather than in a config file, so adding a page
/// is one file and not two.
fn navigation(pages: &[content::Page]) -> Vec<NavSection> {
    let mut sections: Vec<NavSection> = vec![NavSection {
        title: "Project".into(),
        links: vec![NavLink {
            title: "Roadmap".into(),
            url: "/roadmap/".into(),
        }],
    }];
    let mut ordered: Vec<&content::Page> = pages.iter().collect();
    ordered.sort_by(|a, b| {
        a.front
            .order
            .unwrap_or(i64::MAX)
            .cmp(&b.front.order.unwrap_or(i64::MAX))
            .then_with(|| a.front.title.cmp(&b.front.title))
    });

    for page in ordered {
        let Some(name) = page.front.section.clone() else {
            continue;
        };
        let link = NavLink {
            title: page.front.title.clone(),
            url: page.url.clone(),
        };
        match sections.iter_mut().find(|s| s.title == name) {
            Some(section) => section.links.push(link),
            None => sections.push(NavSection {
                title: name,
                links: vec![link],
            }),
        }
    }
    sections
}

// ─── terminal renders ────────────────────────────────────────────────────────

/// The committed output of `make shots`.
///
/// Missing renders are not an error here — a fresh checkout has them, but a
/// build in an environment with no pseudo-terminal should still produce a site.
/// A page asking for one that is not there is an error, which is the right
/// place for it.
fn read_shots(dir: &Path) -> BTreeMap<String, String> {
    let mut shots = BTreeMap::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return shots;
    };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "html")
            && let Some(name) = path.file_stem().and_then(|s| s.to_str())
            && let Ok(body) = std::fs::read_to_string(&path)
        {
            shots.insert(name.to_string(), body);
        }
    }
    shots
}

// ─── writing it out ──────────────────────────────────────────────────────────

fn write_page(dist: &Path, url: &str, html: &str) {
    let relative = url.trim_matches('/');
    let dir = if relative.is_empty() {
        dist.to_path_buf()
    } else {
        dist.join(relative)
    };
    std::fs::create_dir_all(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    write(&dir.join("index.html"), html);
}

fn write(path: &Path, body: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap_or_else(|e| panic!("{}: {e}", parent.display()));
    }
    std::fs::write(path, body).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

fn copy_tree(from: &Path, to: &Path) {
    let Ok(entries) = std::fs::read_dir(from) else {
        return;
    };
    for entry in entries.filter_map(Result::ok) {
        let source = entry.path();
        let target = to.join(entry.file_name());
        if source.is_dir() {
            std::fs::create_dir_all(&target).expect("create dir");
            copy_tree(&source, &target);
        } else {
            std::fs::copy(&source, &target).unwrap_or_else(|e| panic!("{}: {e}", source.display()));
        }
    }
}

/// Absolute, and one `<url>` per page. A sitemap of site-rooted paths is a
/// well-formed file that no crawler will act on.
fn sitemap(pages: &[content::Page], origin: &str, base: &str) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for page in pages {
        // A page that is in no section and is not the front page is reachable
        // but not listed -- the 404 -- and does not belong in a sitemap.
        if page.front.section.is_none() && page.url != "/" {
            continue;
        }
        let path = page.url.trim_start_matches('/');
        xml.push_str(&format!("  <url><loc>{origin}{base}{path}</loc></url>\n"));
    }
    xml.push_str("</urlset>\n");
    xml
}
