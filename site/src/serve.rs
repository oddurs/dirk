// dirk — a terminal multiplexer that knows what its sessions are for.
//
// Copyright (C) 2026 Oddur Sigurdsson
//
// This program is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free Software
// Foundation, either version 3 of the License, or (at your option) any later
// version.  See <https://www.gnu.org/licenses/>.

//! The development server.
//!
//! It serves the site and rebuilds it when a file changes. Polling mtimes
//! rather than watching the filesystem: a poll every quarter second over a few
//! hundred files costs nothing measurable, and it is the same on both
//! platforms, which a watcher is not.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

/// `build` is a plain function pointer rather than a closure so that the
/// watcher thread can hold it without the generator having to be told about
/// lifetimes it does not otherwise have.
pub fn run(port: u16, build: fn() -> PathBuf) {
    let dist = Arc::new(Mutex::new(build()));
    let watched = watch_roots();

    {
        let dist = Arc::clone(&dist);
        std::thread::spawn(move || {
            let mut seen = stamps(&watched);
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let now = stamps(&watched);
                if now != seen {
                    seen = now;
                    eprintln!("rebuilding");
                    *dist.lock().unwrap() = build();
                }
            }
        });
    }

    let server = tiny_http::Server::http(("127.0.0.1", port))
        .unwrap_or_else(|e| panic!("cannot listen on {port}: {e}"));
    eprintln!("http://127.0.0.1:{port}");

    for request in server.incoming_requests() {
        let root = dist.lock().unwrap().clone();
        let (status, body, kind) = resolve(&root, request.url());
        let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], kind.as_bytes())
            .expect("content type");
        let response = tiny_http::Response::from_data(body)
            .with_status_code(status)
            .with_header(header);
        let _ = request.respond(response);
    }
}

/// Everything a build reads. `src/theme.rs` is in here because the palette is
/// generated from it, and a colour changed in the program should show up in
/// the browser without restarting anything.
fn watch_roots() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("site/ has a parent")
        .to_path_buf();
    vec![
        root.join("site/content"),
        root.join("site/templates"),
        root.join("site/assets"),
        root.join("site/shots"),
        root.join("src/theme.rs"),
        root.join("ROADMAP.md"),
        root.join("NEWS"),
    ]
}

fn stamps(roots: &[PathBuf]) -> BTreeMap<PathBuf, SystemTime> {
    let mut out = BTreeMap::new();
    let mut stack: Vec<PathBuf> = roots.to_vec();
    while let Some(path) = stack.pop() {
        let Ok(meta) = std::fs::metadata(&path) else {
            continue;
        };
        if meta.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                stack.extend(entries.filter_map(Result::ok).map(|e| e.path()));
            }
        } else if let Ok(modified) = meta.modified() {
            out.insert(path, modified);
        }
    }
    out
}

/// A URL, and the file it means.
///
/// `/docs/install/` is `docs/install/index.html`, the same shape GitHub Pages
/// serves, so a link that works here works there.
fn resolve(root: &Path, url: &str) -> (u16, Vec<u8>, &'static str) {
    let path = url.split('?').next().unwrap_or("/").trim_start_matches('/');

    // Nothing outside the built site is servable, whatever the URL says.
    if path.split('/').any(|part| part == "..") {
        return (403, b"forbidden".to_vec(), "text/plain; charset=utf-8");
    }

    let candidate = root.join(path);
    let file = if candidate.is_dir() || path.is_empty() {
        candidate.join("index.html")
    } else if candidate.exists() {
        candidate
    } else {
        root.join(path).with_extension("html")
    };

    match std::fs::read(&file) {
        Ok(body) => (200, body, mime(&file)),
        Err(_) => match std::fs::read(root.join("404/index.html")) {
            Ok(body) => (404, body, "text/html; charset=utf-8"),
            Err(_) => (404, b"not found".to_vec(), "text/plain; charset=utf-8"),
        },
    }
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("html") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("xml") => "application/xml",
        Some("woff2") => "font/woff2",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    }
}
