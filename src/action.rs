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

//! Everything dirk can be asked to do, in one list.
//!
//! There were three copies of this: a `match` arm in the key handler, a row in
//! the README's table, and nothing at all for anybody who wanted to rebind one.
//! The arm was the only one that was true, and discovering what dirk could do
//! meant reading it.
//!
//! One table instead, and the three things that needed it read from here: the
//! keys, the palette, and the documentation of both. An action that is not in
//! this list does not exist, and one that is cannot be undiscoverable.

use crate::mux::{Focus, Session};

macro_rules! actions {
    ($($variant:ident $name:literal $title:literal $key:expr),* $(,)?) => {
        /// One thing dirk can be asked to do.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum Action { $($variant),* }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$variant),*];

            /// What it is called in a configuration file. A noun and a verb,
            /// like the socket commands, because they are the same vocabulary
            /// seen from two sides.
            pub fn name(self) -> &'static str {
                match self { $(Action::$variant => $name),* }
            }

            /// What it is called to a person.
            pub fn title(self) -> &'static str {
                match self { $(Action::$variant => $title),* }
            }

            /// The key it answers to when nobody has said otherwise.
            pub fn default_key(self) -> &'static str {
                match self { $(Action::$variant => $key),* }
            }
        }
    };
}

actions! {
    // Leaving. Two of them, and which is which matters more than anything else
    // in this table.
    Detach       "session.detach"   "Detach — leave it all running"      "d",
    Quit         "session.quit"     "Quit — end every shell and agent"   "q",

    // Making somewhere to work.
    NewWorkspace "workspace.new"    "New space in this project"          "n",
    OpenProject  "project.open"     "Open a project"                     "o",
    NewAgent     "agent.start"      "Start an agent here"                "a",
    NewWorktree  "worktree.add"     "New worktree, and a space in it"    "W",

    // Panes.
    SplitCols    "pane.split-cols"  "Split into columns"                 "v",
    SplitRows    "pane.split-rows"  "Split into rows"                    "s",
    ClosePane    "pane.close"       "Close this pane"                    "x",
    CyclePane    "pane.cycle"       "Next pane in this space"            ";",
    Zoom         "pane.zoom"        "Zoom this pane, and back"           "z",
    MovePaneBack "pane.move-back"   "Move this pane back"                "{",
    MovePaneOn   "pane.move-on"     "Move this pane on"                  "}",
    Restart      "pane.restart"     "Restart a stopped pane"             "r",

    // Tabs: the level between a space and its panes.
    // `c` and `&` are tmux's, and `[` is tmux's copy mode, which is why next
    // and previous are not its `n` and `p`: both are spoken for here.
    NewTab       "tab.new"          "New tab in this space"              "c",
    NextTab      "tab.next"         "Next tab"                           ".",
    PrevTab      "tab.prev"         "Previous tab"                       ",",
    CloseTab     "tab.close"        "Close this tab"                     "&",

    // Reading.
    Read         "pane.read"        "Read this pane's scrollback"        "[",
    Find         "session.find"     "Find a line, in any pane"           "/",

    // Moving about.
    NextSpace    "workspace.next"   "Next space"                         "j",
    PrevSpace    "workspace.prev"   "Previous space"                     "k",
    Nav          "nav.focus"        "Give the nav the keyboard"          "w",
    Sidebar      "nav.toggle"       "Hide the nav"                       "b",
    Palette      "session.palette"  "Everything dirk can do"             "p",

    // Naming.
    Release      "workspace.release" "Release a held name"               "u",
}

impl Action {
    pub fn named(name: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.name() == name)
    }

    /// Why this cannot be done right now, if it cannot.
    ///
    /// Shown beside a disabled entry rather than hiding it. A palette that
    /// silently omits what it cannot do teaches you that dirk cannot do it.
    pub fn why_not(self, session: &Session) -> Option<&'static str> {
        let in_space = matches!(session.focus, Focus::Ws { .. });
        let panes = session.focused_workspace().map_or(0, |ws| ws.panes().len());
        match self {
            Action::NewWorkspace | Action::NewAgent | Action::NewWorktree if !in_space => {
                Some("no project focused")
            }
            Action::Zoom | Action::MovePaneBack | Action::MovePaneOn if panes < 2 => {
                Some("only one pane here")
            }
            Action::CyclePane if panes < 2 => Some("only one pane here"),
            Action::NewTab | Action::CloseTab | Action::NextTab | Action::PrevTab if !in_space => {
                Some("boards have one arrangement")
            }
            Action::CloseTab | Action::NextTab | Action::PrevTab
                if session
                    .focused_workspace()
                    .is_some_and(|ws| ws.tabs.len() < 2) =>
            {
                Some("only one tab here")
            }
            Action::Restart
                if !session
                    .focused_workspace()
                    .and_then(|ws| ws.active_pane())
                    .is_some_and(|p| p.dead) =>
            {
                Some("nothing has stopped")
            }
            _ => None,
        }
    }
}

/// Which key runs what, after the configuration has had its say.
#[derive(Debug, Clone)]
pub struct Keys(Vec<(String, Action)>);

impl Default for Keys {
    fn default() -> Self {
        Keys(
            Action::ALL
                .iter()
                .map(|a| (a.default_key().to_string(), *a))
                .collect(),
        )
    }
}

impl Keys {
    /// Rebind one. The old key for that action goes: an action reachable from
    /// two keys, one of which you did not ask for, is how a rebind appears not
    /// to have worked.
    pub fn bind(&mut self, action: Action, key: &str) {
        self.0.retain(|(k, a)| *a != action && k != key);
        self.0.push((key.to_string(), action));
    }

    pub fn action(&self, key: &str) -> Option<Action> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, a)| *a)
    }

    /// What to press for this, for the palette and the documentation.
    pub fn key(&self, action: Action) -> Option<&str> {
        self.0
            .iter()
            .find(|(_, a)| *a == action)
            .map(|(k, _)| k.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_action_has_a_name_that_finds_it_again() {
        // The names are the configuration surface. One that does not round-trip
        // is an action nobody can rebind and a typo nobody can be told about.
        for &a in Action::ALL {
            assert_eq!(Action::named(a.name()), Some(a), "{}", a.name());
            assert!(
                a.name().contains('.'),
                "{} is not a noun and a verb",
                a.name()
            );
            assert!(!a.title().is_empty(), "{} has nothing to call it", a.name());
        }
    }

    #[test]
    fn no_two_actions_ship_bound_to_the_same_key() {
        // The second one would be unreachable, and nothing would say so.
        let keys = Keys::default();
        for &a in Action::ALL {
            assert_eq!(
                keys.action(a.default_key()),
                Some(a),
                "{} is not reachable by its own default key",
                a.name()
            );
        }
    }

    #[test]
    fn rebinding_takes_the_old_key_away() {
        // An action reachable from two keys, one of which you did not ask for,
        // is how a rebind looks like it did not work.
        let mut keys = Keys::default();
        keys.bind(Action::Quit, "Q");
        assert_eq!(keys.action("Q"), Some(Action::Quit));
        assert_eq!(keys.action("q"), None, "the old key still works");
    }

    #[test]
    fn binding_over_another_action_takes_that_one_s_key() {
        // Two actions on one key means one of them is unreachable, and which
        // one is an accident of ordering.
        let mut keys = Keys::default();
        keys.bind(Action::Quit, "n");
        assert_eq!(keys.action("n"), Some(Action::Quit));
        assert_eq!(
            keys.key(Action::NewWorkspace),
            None,
            "two actions are on one key"
        );
    }
}
