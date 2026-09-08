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

//! What a name is made of.
//!
//! A workspace label is a template over these, so they are an interface rather
//! than whatever the code happens to have to hand. namesync wrote the same set
//! down as a contract because herdr's metadata is a flat untyped map, where a
//! producer renaming a key breaks a consumer with no error and nothing in a
//! build log. dirk owns both ends and pays less for a mistake, but the meanings
//! still have to be decided rather than emerge.
//!
//! **Absent, never empty.** A token that is not known is not there, so a
//! template renders a gap rather than the word `None` or a stray separator.
//!
//! **Two clocks, and confusing them makes both useless.** `since` restarts
//! whenever the agent changes state and answers "who has been blocked
//! longest". `age` restarts only when the title changes and answers "what has
//! been grinding on the same thing all day". An agent that has started and
//! finished six times is still working on one task, and `age` is the number
//! that says so.
//!
//! **`n` is display, not identity.** It is positional and changes when spaces
//! are reordered.

use std::collections::BTreeMap;

/// The tokens of one workspace.
///
/// A map rather than a struct of options: templates address these by name, the
/// set grows, and every consumer already has to handle a token being absent.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Tokens {
    own: BTreeMap<&'static str, String>,
    /// What somebody outside dirk said about this workspace.
    ///
    /// Separate because the names are not dirk's and cannot be checked against
    /// anything: they are whatever the program reporting them chose. Addressed
    /// as `{said.summary}`, so a template makes it obvious which half of the
    /// vocabulary a token came from — and so a typo in one of dirk's own
    /// tokens is still caught.
    said: BTreeMap<String, String>,
}

/// The prefix a token reported from outside dirk is addressed under.
pub const SAID: &str = "said.";

/// Every token dirk publishes. Adding one means adding it here, which is the
/// point — the list is the contract.
pub const NAMES: &[&str] = &[
    "project",
    "branch",
    "worktree",
    "n",
    "intent",
    "intent-slug",
    "since",
    "age",
    "locked",
    "stale",
    "agent",
    "agents",
];

impl Tokens {
    /// Set a token, unless the value is empty — in which case it stays absent,
    /// which is not the same thing and renders differently.
    pub fn set(&mut self, name: &'static str, value: impl Into<String>) {
        let value = value.into();
        if !value.trim().is_empty() {
            self.own.insert(name, value);
        }
    }

    /// Take everything an outside program has said about this workspace.
    pub fn said(&mut self, said: &BTreeMap<String, String>) {
        self.said = said.clone();
    }

    /// Set only when the condition holds, for the tokens that are a flag with a
    /// word for a value: `worktree`, `locked`, `stale`.
    pub fn flag(&mut self, name: &'static str, value: &'static str, when: bool) {
        if when {
            self.own.insert(name, value.to_string());
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        match name.strip_prefix(SAID) {
            Some(key) => self.said.get(key).map(String::as_str),
            None => self.own.get(name).map(String::as_str),
        }
    }
}

/// Tokens a template asks for that do not exist.
///
/// The list above is the contract, so a template naming something else is a
/// typo that would otherwise render as silence — a label that is simply shorter
/// than intended, with nothing to say why.
pub fn unknown(template: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else { break };
        let name = &after[..close];
        // Anything under `said.` is somebody else's vocabulary, so there is
        // nothing to check it against. That is the price of the namespace and
        // the reason for it: dirk's own names stay checkable.
        if !name.is_empty() && !NAMES.contains(&name) && !name.starts_with(SAID) {
            out.push(name.to_string());
        }
        rest = &after[close + 1..];
    }
    out
}

/// Fill `{token}` placeholders from the tokens.
///
/// An unknown or absent token renders as nothing rather than as its own name,
/// and the result is collapsed and trimmed — so a template written for a
/// workspace that has a branch does not leave a hole in one that has not.
pub fn render(template: &str, tokens: &Tokens) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;

    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            // An unclosed brace is literal text, not the start of a token.
            out.push_str(&rest[open..]);
            return tidy(&out);
        };
        let name = &after[..close];
        if let Some(value) = tokens.get(name) {
            out.push_str(value);
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    tidy(&out)
}

/// Collapse the gaps an absent token leaves behind.
fn tidy(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn what_somebody_else_said_lives_in_its_own_namespace() {
        use std::collections::BTreeMap;
        let mut t = super::Tokens::default();
        t.set("agent", "claude");
        let mut said = BTreeMap::new();
        said.insert("summary".to_string(), "indexing".to_string());
        said.insert("agent".to_string(), "not this one".to_string());
        t.said(&said);

        assert_eq!(t.get("said.summary"), Some("indexing"));
        // The namespace is what stops somebody else's vocabulary shadowing
        // dirk's, which is also why a template has to say which it means.
        assert_eq!(t.get("agent"), Some("claude"));
        assert_eq!(t.get("said.agent"), Some("not this one"));
        assert_eq!(t.get("said.nothing"), None);
    }

    #[test]
    fn a_template_can_name_a_token_dirk_cannot_check() {
        // dirk checks its own names against the list and reports a typo. It
        // cannot check yours, so yours live where a template makes that plain.
        assert!(super::unknown("{said.anything}").is_empty());
        assert_eq!(super::unknown("{branchh}"), vec!["branchh".to_string()]);
    }

    use super::*;

    fn sample() -> Tokens {
        let mut t = Tokens::default();
        t.set("project", "dirk");
        t.set("branch", "naming-tokens");
        t.set("intent", "Building the mux core");
        t.set("n", "3");
        t.flag("worktree", "worktree", false);
        t
    }

    #[test]
    fn a_template_is_filled_from_the_tokens() {
        assert_eq!(render("{intent}", &sample()), "Building the mux core");
        assert_eq!(
            render("{n} {project} · {intent}", &sample()),
            "3 dirk · Building the mux core"
        );
    }

    #[test]
    fn an_absent_token_leaves_no_gap() {
        // The workspace above is not a worktree, so that token is absent
        // rather than empty — and the template must not show where it was.
        assert_eq!(
            render("{project} {worktree} {branch}", &sample()),
            "dirk naming-tokens"
        );
    }

    #[test]
    fn an_unknown_token_renders_as_nothing_not_as_its_name() {
        assert_eq!(render("{project} {nonsense}", &sample()), "dirk");
    }

    #[test]
    fn an_empty_value_is_absent_rather_than_empty() {
        let mut t = Tokens::default();
        t.set("project", "");
        t.set("branch", "   ");
        assert_eq!(t.get("project"), None, "a blank value is not a value");
        assert_eq!(render("{project}{branch}", &t), "");
    }

    #[test]
    fn a_brace_that_closes_nothing_is_literal_text() {
        assert_eq!(render("100% of {project", &sample()), "100% of {project");
        assert_eq!(render("}{", &sample()), "}{");
    }

    #[test]
    fn a_flag_token_is_a_word_or_is_absent() {
        let mut t = Tokens::default();
        t.flag("worktree", "worktree", true);
        t.flag("locked", "held", false);
        assert_eq!(t.get("worktree"), Some("worktree"));
        assert_eq!(t.get("locked"), None);
    }

    #[test]
    fn a_template_naming_something_that_does_not_exist_is_reported() {
        assert_eq!(unknown("{intent} {branch}"), Vec::<String>::new());
        assert_eq!(unknown("{intent} {brnach}"), vec!["brnach"]);
        // The defaults must not trip their own check.
        let d = crate::config::Templates::default();
        assert!(unknown(&d.workspace).is_empty(), "{}", d.workspace);
        assert!(unknown(&d.agent).is_empty(), "{}", d.agent);
    }

    #[test]
    fn the_contract_lists_every_token_that_can_be_set() {
        // The list is the interface. A token set but not listed is one nobody
        // reading the documentation would know to ask for.
        let mut t = Tokens::default();
        for name in NAMES {
            t.set(name, "x");
        }
        assert_eq!(
            t.own.len(),
            NAMES.len(),
            "a name in the list is not settable"
        );
    }
}
