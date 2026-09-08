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

//! A second source of intent, for panes whose title says nothing.
//!
//! Naming reads the agent's terminal title, and that is the right primary
//! source: the work is already done, it costs nothing and it needs no key. It
//! fails in one specific way — a pane where nothing publishes a title has no
//! intent at all, and its workspace keeps its project name for ever.
//!
//! **The model is a source, not a decider.** It supplies a candidate intent from
//! what is on the screen, and that candidate then goes through precisely the
//! policy every title goes through: junk rejection, the hand-written-name lock,
//! the debounce, the similarity check, the rate limit. Nothing about *when* a
//! name changes moves into the model.
//!
//! That is what lets this exist without contradicting the argument the project
//! started from. namesync's claim was never that models are bad at this — it
//! was that generating a name is the part already solved and deciding when to
//! use one is the part that is not.
//!
//! No HTTP dependency. dirk shells out for the process table, for git and for
//! notifications; one request for a caption is the same kind of thing, and a
//! client crate is a build cost and a transitive tree for it.

use crate::config::Llm;
use std::io::Write;
use std::process::{Command, Stdio};

/// What the model is asked for. Deliberately narrow: a phrase, not a sentence,
/// and an explicit way to say it cannot tell — which is more useful than a
/// guess, because a guess would go through the policy and become a name.
const SYSTEM: &str = "\
You are naming a terminal workspace from what is on its screen.

Reply with a short phrase describing what the work is: five words at most, no
punctuation at the end, no quotes, no preamble. Describe the task, not the
tool -- \"Porting the naming policy\", not \"running cargo test\".

If the screen does not show what anyone is working on, reply with exactly:
unknown";

/// Ask for a candidate intent. Blocking; never call this on the drawing thread.
///
/// Every failure returns `None`, which leaves naming exactly where it was
/// without this source: no key, no curl, a timeout, a rate limit, a malformed
/// answer, all the same. A naming source that can break naming is worse than
/// no naming source.
pub fn suggest(cfg: &Llm, viewport: &str) -> Option<String> {
    let key = std::env::var(&cfg.api_key_env)
        .ok()
        .filter(|k| !k.trim().is_empty())?;
    let screen = clamp(viewport, cfg.max_chars);
    if screen.trim().is_empty() {
        return None;
    }

    let body = serde_json::json!({
        "model": cfg.model,
        // Adaptive thinking is on by default and shares this budget with the
        // answer. Too small a number is spent thinking and returns no text at
        // all -- which this source treats as "no candidate" and would not
        // mention.
        "max_tokens": 4096,
        // The cheapest setting that suits the task. Naming a screen is not
        // work that rewards deliberation.
        "output_config": { "effort": "low" },
        "system": SYSTEM,
        "messages": [{ "role": "user", "content": screen }],
    })
    .to_string();

    let out = curl(cfg, &key, &body)?;
    let parsed: serde_json::Value = serde_json::from_slice(&out).ok()?;

    // A refusal or an error is an answer too, and both mean "no candidate".
    let text = parsed
        .get("content")?
        .as_array()?
        .iter()
        .find(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())?;

    let candidate = text.trim().trim_matches('"').trim();
    if candidate.is_empty() || candidate.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(candidate.to_string())
}

/// Run the request.
///
/// The key goes in curl's configuration on stdin rather than in an argument.
/// dirk reads the process table itself, once a second, and shows what it finds
/// — a key in `argv` would be one function call away from being drawn on the
/// screen.
fn curl(cfg: &Llm, key: &str, body: &str) -> Option<Vec<u8>> {
    let mut child = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--max-time",
            // Seconds, and never zero: `--max-time 0` is curl for *no* limit,
            // which would leave the thread waiting for ever.
            &cfg.timeout_ms.div_ceil(1000).max(1).to_string(),
            "--config",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;

    child
        .stdin
        .take()?
        .write_all(request(cfg, key, body).as_bytes())
        .ok()?;
    let out = child.wait_with_output().ok()?;
    out.status.success().then_some(out.stdout)
}

/// The configuration curl reads on stdin. Separate so it can be checked without
/// sending anything.
fn request(cfg: &Llm, key: &str, body: &str) -> String {
    format!(
        "url = \"{}\"\n\
         header = \"content-type: application/json\"\n\
         header = \"anthropic-version: 2023-06-01\"\n\
         header = \"x-api-key: {}\"\n\
         data = \"{}\"\n",
        escape(&cfg.endpoint),
        escape(key),
        escape(body),
    )
}

/// curl's configuration file quotes with backslash escapes.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "")
}

/// Keep the last of the screen rather than the first: the recent end of a
/// scrolling pane is the part that says what is happening now.
fn clamp(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let skip = text.chars().count() - max;
    text.chars().skip(skip).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_cannot_break_out_of_the_configuration() {
        // The config is a text format read on stdin; a value containing a quote
        // or a newline would otherwise start a new directive.
        let hostile = "abc\"\nurl = \"http://evil\n";
        let e = escape(hostile);
        assert!(!e.contains('\n'), "a newline survived: {e:?}");
        // Every quote is escaped, so the value is never left.
        for (i, c) in e.char_indices() {
            if c == '"' {
                assert_eq!(&e[i - 1..i], "\\", "unescaped quote at {i}");
            }
        }
    }

    #[test]
    fn the_request_carries_the_key_and_the_body() {
        let cfg = Llm::default();
        let body = serde_json::json!({"model": "m", "messages": []}).to_string();
        let r = request(&cfg, "sk-secret", &body);

        // One directive per line, each a `name = "value"` pair.
        for line in r.lines() {
            assert!(line.contains(" = \""), "not a directive: {line:?}");
            assert!(line.ends_with('"'), "unterminated: {line:?}");
        }
        assert!(r.contains("x-api-key: sk-secret"));
        assert!(r.contains("anthropic-version: 2023-06-01"));
        // The body is JSON inside a quoted value, so its quotes are escaped.
        assert!(
            r.contains("\\\"model\\\""),
            "the body was not escaped into the value"
        );
    }

    #[test]
    fn the_key_is_never_an_argument() {
        // dirk reads the process table itself, once a second, and draws what it
        // finds. A key in argv would be one function call from the screen.
        let cfg = Llm::default();
        assert!(request(&cfg, "sk-secret", "{}").contains("sk-secret"));
        // The only place it appears is the configuration, which goes on stdin.
        let args = [
            "--silent",
            "--show-error",
            "--max-time",
            "8",
            "--config",
            "-",
        ];
        assert!(!args.iter().any(|a| a.contains("sk-")));
    }

    #[test]
    fn the_recent_end_of_the_screen_is_what_is_kept() {
        assert_eq!(clamp("abcdef", 3), "def");
        assert_eq!(clamp("abc", 10), "abc");
        assert_eq!(clamp("", 10), "");
    }

    #[test]
    fn nothing_happens_without_a_key() {
        let cfg = Llm {
            enabled: true,
            api_key_env: "DIRK_TEST_KEY_THAT_IS_NOT_SET".into(),
            ..Llm::default()
        };
        assert_eq!(suggest(&cfg, "some screen"), None);
    }

    #[test]
    fn an_empty_screen_is_not_worth_asking_about() {
        let cfg = Llm {
            enabled: true,
            ..Llm::default()
        };
        // Even with a key present this returns before spending anything.
        unsafe { std::env::set_var("DIRK_TEST_KEY", "x") };
        let cfg = Llm {
            api_key_env: "DIRK_TEST_KEY".into(),
            ..cfg
        };
        assert_eq!(suggest(&cfg, "   \n  \n"), None);
        unsafe { std::env::remove_var("DIRK_TEST_KEY") };
    }
}
