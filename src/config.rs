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

//! Configuration: the few things that are yours rather than dirk's.
//!
//! Everything here has a working default, so dirk runs with no config file at
//! all. `~/.config/dirk/config.toml` overrides what it names and leaves the
//! rest alone.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// One pane of a layout: either a program to run, or a split holding more
/// panes. A node with children ignores its own `command`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct PaneDef {
    /// Shown on the pane's border.
    pub title: String,
    pub command: Vec<String>,
    /// `"5"` for lines or columns, `"30%"` for a share, empty to take what is
    /// left. Panes with no size split the remainder evenly.
    pub size: String,
    /// How this pane's children divide it: `"cols"` or `"rows"`.
    pub split: String,
    /// Where to run. Empty means the directory of whatever was focused when the
    /// layout was opened, which is almost always what a panel wants: a board or
    /// a repository browser is about the project you are in, not about your
    /// home directory. `~` is expanded.
    pub cwd: String,
    pub pane: Vec<PaneDef>,
}

/// A named arrangement of programs, listed above the projects in the sidebar
/// and opened as a space of its own.
///
/// A layout with a `command` and no `pane` entries is a single-program layout,
/// which is what the three defaults are. Nothing distinguishes it structurally
/// from the dashboard except how many leaves it has.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct LayoutDef {
    pub name: String,
    /// Single key that jumps straight here.
    pub key: Option<char>,
    /// Used when there are no children.
    pub command: Vec<String>,
    pub split: String,
    pub pane: Vec<PaneDef>,
    /// What to say on the row without being opened.
    ///
    /// This is the difference between a link and an instrument: a dashboard you
    /// have to open to find out whether it matters is a link with extra steps.
    #[serde(default)]
    pub status: Option<StatusDef>,
    /// Whether the panes stay running when you look away.
    ///
    /// A monitoring dashboard keeps; a thing you opened for ten seconds should
    /// not sit there holding a lock. One boolean rather than a second concept:
    /// a separate "utility window" type would be two code paths that drift.
    pub keep: bool,
}

/// A command whose output becomes a badge on a board's row.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StatusDef {
    pub run: Vec<String>,
    /// How often, as a duration: `10s`, `2m`, or seconds on their own.
    #[serde(default = "ten_seconds")]
    pub every: String,
}

fn ten_seconds() -> String {
    "10s".into()
}

impl StatusDef {
    /// The interval, floored.
    ///
    /// Two seconds is the floor because this is a subprocess: four boards at
    /// ten seconds is already twenty-four processes a minute, and somebody
    /// writing `100ms` has not thought about what they are asking for.
    pub fn interval(&self) -> std::time::Duration {
        const FLOOR: std::time::Duration = std::time::Duration::from_secs(2);
        // Capped as well as floored: an interval of a century is a number, not
        // an intention, and the backoff multiplies whatever this returns.
        const CEILING: std::time::Duration = std::time::Duration::from_secs(24 * 3600);
        parse_every(&self.every)
            .unwrap_or(std::time::Duration::from_secs(10))
            .clamp(FLOOR, CEILING)
    }

    /// How long a run may take before it is killed.
    ///
    /// Its own interval, so a board asked every ten seconds may take ten -- but
    /// never less than a couple of seconds, because a slow disk is not a hang.
    pub fn cap(&self) -> std::time::Duration {
        self.interval().max(std::time::Duration::from_secs(2))
    }
}

/// `10s`, `2m`, `1h`, or a bare number of seconds.
pub fn parse_every(text: &str) -> Option<std::time::Duration> {
    let text = text.trim();
    let (digits, scale) = match text.chars().last()? {
        's' => (&text[..text.len() - 1], 1),
        'm' => (&text[..text.len() - 1], 60),
        'h' => (&text[..text.len() - 1], 3600),
        _ => (text, 1),
    };
    let n: u64 = digits.trim().parse().ok()?;
    Some(std::time::Duration::from_secs(n.saturating_mul(scale)))
}

impl PaneDef {
    /// This pane and its descendants, flattened to the ones that actually run
    /// something.
    fn leaves(&self) -> Vec<&PaneDef> {
        if self.pane.is_empty() {
            return vec![self];
        }
        self.pane.iter().flat_map(|p| p.leaves()).collect()
    }
}

impl LayoutDef {
    fn leaves(&self) -> Vec<&PaneDef> {
        if self.pane.is_empty() {
            return Vec::new();
        }
        self.pane.iter().flat_map(|p| p.leaves()).collect()
    }

    /// True when this layout can actually be built: every leaf names a program,
    /// and every one of those is installed.
    ///
    /// The first half matters as much as the second. `command` is optional in
    /// the file, so a pane can be written with a title and nothing to run, and
    /// a layout that is offered and then cannot start is worse than one that
    /// was never listed.
    pub fn runnable(&self) -> bool {
        let leaves = self.leaves();
        if leaves.is_empty() {
            // A single-program layout: the command lives on the layout itself.
            return self.command.first().is_some_and(|c| on_path(c));
        }
        leaves
            .iter()
            .all(|l| l.command.first().is_some_and(|c| on_path(c)))
    }
}

/// Who the rail says you are, and where.
///
/// The first cells of the bar are the strongest position it has, and they used
/// to hold the product's name — constant, unclickable, and an answer to "what
/// program is this", which you knew before you started it. The job of that
/// slot is *where am I*, and a product name cannot answer it.
///
/// What it answers now is whose account, which session, and which machine. The
/// last of those is not decoration: a local dirk and one reached over `ssh`
/// were identical on screen, which is how someone runs the right command in
/// the wrong place.
#[derive(Debug, Clone, Deserialize)]
pub struct Identity {
    #[serde(default = "default_mark")]
    pub mark: String,
    /// Empty means `$USER`. Naming one makes it yours, and a notification
    /// then uses it too — someone who has renamed the program has renamed all
    /// of it.
    #[serde(default)]
    pub name: String,
    /// `never`, `named` — anything but the default session — or `always`.
    #[serde(default = "default_session")]
    pub session: String,
    /// `never`, `remote` or `always`.
    #[serde(default = "default_host")]
    pub host: String,
}

/// Empty means the glyph set's mark, so a terminal that cannot draw `▾` is not
/// handed a `◆` either. Naming one here makes it content, and content is yours.
fn default_mark() -> String {
    String::new()
}
fn default_session() -> String {
    "named".into()
}
fn default_host() -> String {
    "remote".into()
}

/// What this program is called when something outside it has to name it.
const PRODUCT: &str = "dirk";

impl Identity {
    /// Who the rail says you are.
    pub fn who(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        std::env::var("USER")
            .or_else(|_| std::env::var("LOGNAME"))
            .ok()
            .filter(|u| !u.is_empty())
            // A container with neither is rare and is not worth an empty slot.
            .unwrap_or_else(|| PRODUCT.into())
    }

    /// What a desktop notification calls this program.
    ///
    /// Not `who`: a notification saying "oddurs" would be naming the wrong
    /// thing. Someone who set a name has renamed the program and means it
    /// everywhere; someone who has not gets the product.
    pub fn app(&self) -> String {
        match self.name.is_empty() {
            true => PRODUCT.into(),
            false => self.name.clone(),
        }
    }

    /// The session's name, when the rail should say it.
    pub fn session_shown<'a>(&self, name: Option<&'a str>) -> Option<&'a str> {
        let name = name.filter(|n| !n.is_empty())?;
        match self.session.as_str() {
            "always" => Some(name),
            "never" => None,
            // A session nobody named is the only session there is.
            _ => (name != "default").then_some(name),
        }
    }

    /// The machine's name, when the rail should say it.
    ///
    /// `remote` asks the environment whether this dirk is on the far side of
    /// an `ssh` connection, which is the same question every shell prompt
    /// asks and answers the same way. It is a heuristic and it is the only one
    /// available from inside a single process; `always` is the escape hatch
    /// for a machine that should always name itself.
    pub fn host_shown(&self) -> Option<String> {
        let show = match self.host.as_str() {
            "always" => true,
            "never" => false,
            _ => ["SSH_CONNECTION", "SSH_TTY", "SSH_CLIENT"]
                .iter()
                // Set-but-empty is not a connection. It is what a parent that
                // wanted to unset it left behind.
                .any(|v| std::env::var(v).is_ok_and(|s| !s.is_empty())),
        };
        show.then(hostname).flatten()
    }
}

/// The machine's name, short. `gethostname` rather than `$HOSTNAME`, which is
/// a shell variable that is often not exported.
///
/// Trimmed to the first label: a session on `build.example.com` is on `build`,
/// and the rest is a domain nobody is choosing a window by.
///
/// `[u8; 256]` and `.cast()` rather than `[i8; 256]`: `c_char` is signed on
/// x86-64 and on Darwin and unsigned on aarch64 Linux, so a buffer typed for
/// one of them does not compile for the other. This is public because there
/// was a second copy of it that got that wrong.
pub fn hostname() -> Option<String> {
    let mut buf = [0u8; 256];
    // SAFETY: the buffer outlives the call and its length is what is passed.
    let ok = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) } == 0;
    if !ok {
        return None;
    }
    let end = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    let name = String::from_utf8_lossy(&buf[..end]).into_owned();
    // `prod.example.com` in a bar is four columns of information and eleven of
    // domain.
    let short = name.split('.').next().unwrap_or(&name).to_string();
    (!short.is_empty()).then_some(short)
}

impl Default for Identity {
    fn default() -> Self {
        Self {
            mark: default_mark(),
            name: String::new(),
            session: default_session(),
            host: default_host(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Config {
    /// `[brand]` is still read: it named the same slot before the slot's job
    /// was settled, and a configuration file that works should keep working.
    #[serde(alias = "brand")]
    pub identity: Identity,
    /// Where `o` looks for projects to open.
    pub projects_root: PathBuf,
    /// Named arrangements you jump to. `board` is the word the interface uses;
    /// `layout` is what it was called first and still parses.
    #[serde(rename = "board", alias = "layout")]
    pub layouts: Vec<LayoutDef>,
    /// Empty means `$SHELL`.
    pub shell: String,
    /// Wide enough for two-line rows: a name, an age beside it, and a branch
    /// under it. 28 was right when a row was a glyph and a word.
    pub sidebar_width: u16,
    pub scrollback: usize,
    pub naming: Naming,
    pub notify: Notify,
    pub server: Server,
    pub terminal: Terminal,
    pub ui: Ui,
    pub session: SessionCfg,
    pub sound: Sound,
    /// What to pipe a selection into. Empty means whatever this platform is
    /// likely to have -- `pbcopy`, `wl-copy`, `xclip`.
    pub clipboard: Vec<String>,
    /// Keys, by the name of the thing they do. Anything not named here keeps
    /// the key it ships with.
    pub keys: std::collections::BTreeMap<String, Bind>,
    pub nav: Nav,
    /// Programs that run something else and are not it.
    ///
    /// A sandbox or a container shim shows itself as the foreground process, so
    /// dirk sees `fence` and no agent at all — no state, no naming, no
    /// notification, in exactly the setup where an agent is most likely to be
    /// left running unattended. Naming it here says its command line is worth
    /// reading, which is the one case where dirk reads anybody's.
    ///
    /// Added to the interpreters dirk already knows rather than replacing them.
    #[serde(default)]
    pub wrappers: Vec<String>,
    /// Harnesses dirk should recognise, on top of the ones it ships with.
    #[serde(rename = "agent")]
    pub agents: Vec<AgentDef>,
    /// Which agent `a` starts, when a project does not say.
    pub default_agent: String,
    /// Per-project settings. Everything here has a global answer too; this is
    /// where a dozen repositories stop wanting the same one.
    #[serde(rename = "project")]
    pub projects: Vec<ProjectDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectDef {
    pub path: PathBuf,
    /// Which agent `a` starts here.
    #[serde(default)]
    pub agent: String,
    /// Whether this project may make a noise. The repository you are
    /// babysitting should be able to be quiet without silencing the one you
    /// are not.
    #[serde(default = "yes")]
    pub sound: bool,
}

/// A noise when an agent blocks, and a different one when it finishes.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sound {
    /// Off unless asked for. It is an interruption, and one nobody chose is
    /// one they turn off rather than tune.
    pub enabled: bool,
    /// The command for each. Empty means the terminal's own bell, which needs
    /// no file and no player.
    pub blocked: Vec<String>,
    pub done: Vec<String>,
    /// Per harness: `on`, `off`, or `default`.
    ///
    /// The shape of the problem when you run three at once. One of them is
    /// chatty and the other two are not, and the only answers available were
    /// "all of them" and "none of them" — or silencing the whole repository,
    /// which silences the two you wanted to hear.
    #[serde(rename = "agents")]
    pub per_agent: std::collections::BTreeMap<String, String>,
}

/// What a per-harness or per-project answer says about making a noise.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Says {
    Yes,
    No,
    /// Nothing: whatever the next answer down says.
    Nothing,
}

impl Sound {
    /// What this harness's own entry says.
    ///
    /// An entry dirk does not understand says nothing rather than guessing,
    /// and is complained about at load — a typo that silently meant `off` is a
    /// notification you never hear and never find out about.
    pub fn about(&self, agent: Option<&str>) -> Says {
        match agent
            .and_then(|a| self.per_agent.get(a))
            .map(String::as_str)
        {
            Some("on") => Says::Yes,
            Some("off") => Says::No,
            _ => Says::Nothing,
        }
    }
}

impl Sound {
    /// What to run for one of the two, after the shipped answer.
    pub fn command(&self, which: crate::sound::Alert) -> Vec<String> {
        let named = match which {
            crate::sound::Alert::Blocked => &self.blocked,
            crate::sound::Alert::Done => &self.done,
        };
        if !named.is_empty() {
            return named
                .iter()
                .map(|a| expand(a).to_string_lossy().into())
                .collect();
        }
        shipped(which)
    }
}

/// Two sounds that exist on the machine already, so this works with no file to
/// find and no configuration to write.
///
/// Distinguishable on purpose: one rises and one lands. Elsewhere there is no
/// set of sounds anybody can count on, and an empty command falls through to
/// the bell rather than to a player that may not be installed.
#[cfg(target_os = "macos")]
fn shipped(which: crate::sound::Alert) -> Vec<String> {
    let file = match which {
        crate::sound::Alert::Blocked => "/System/Library/Sounds/Funk.aiff",
        crate::sound::Alert::Done => "/System/Library/Sounds/Glass.aiff",
    };
    match std::path::Path::new(file).exists() {
        true => vec!["afplay".into(), file.into()],
        false => Vec::new(),
    }
}

#[cfg(not(target_os = "macos"))]
fn shipped(_which: crate::sound::Alert) -> Vec<String> {
    Vec::new()
}

/// One harness, as a configuration file describes it.
///
/// The shipped ones are expressed in exactly this shape, so the schema is
/// proven by the things already using it rather than by a second code path.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDef {
    pub name: String,
    /// Process names it runs under. Empty means its own name.
    #[serde(default)]
    pub names: Vec<String>,
    /// Fragments of a command line, for when it runs under an interpreter.
    #[serde(default)]
    pub argv: Vec<String>,
    /// How to start one. Empty means its own name.
    #[serde(default)]
    pub command: Vec<String>,
    /// How to start one again on the conversation it was having, with
    /// `{session}` standing for whatever the harness calls its session. Empty
    /// means dirk does not know, and a restored pane gets a shell.
    #[serde(default)]
    pub resume: Vec<String>,
    #[serde(default)]
    pub blocked: BlockedDef,
}

impl AgentDef {
    /// The rules this block describes.
    fn kind(&self, from: crate::agent::From) -> crate::agent::Kind {
        crate::agent::Kind {
            name: self.name.clone(),
            names: match self.names.is_empty() {
                true => vec![self.name.clone()],
                false => self.names.clone(),
            },
            argv: self.argv.clone(),
            command: match self.command.is_empty() {
                true => vec![self.name.clone()],
                false => self.command.clone(),
            },
            blocked: self.blocked.markers.clone(),
            choices: self.blocked.menu,
            from,
            resume: self.resume.clone(),
        }
    }
}

/// One harness's rules, as its own file.
///
/// The same fields as an `[[agent]]` block without the name, which the filename
/// already carries.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct AgentFile {
    names: Vec<String>,
    argv: Vec<String>,
    command: Vec<String>,
    resume: Vec<String>,
    blocked: BlockedDef,
}

impl AgentFile {
    fn into_def(self) -> AgentDef {
        AgentDef {
            name: String::new(),
            names: self.names,
            argv: self.argv,
            command: self.command,
            resume: self.resume,
            blocked: self.blocked,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockedDef {
    /// Whether a marker only counts alongside a menu of numbered answers.
    #[serde(default = "yes")]
    pub menu: bool,
    #[serde(default, rename = "match")]
    pub markers: Vec<String>,
}

fn yes() -> bool {
    true
}

impl Default for BlockedDef {
    fn default() -> Self {
        BlockedDef {
            menu: true,
            markers: Vec::new(),
        }
    }
}

/// How the column down the left is drawn.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Nav {
    /// Which shipped set of marks to draw with: `unicode`, `ascii` or `round`.
    pub glyphs: String,
    /// Marks to replace, for a font dirk cannot check the presence of.
    #[serde(rename = "glyph")]
    pub glyphs_override: Vec<GlyphDef>,
    /// Whether the attention zone holds its place: `when-needed`, `always` or
    /// `never`.
    pub attention: String,
    /// `tall` gives a workspace a second line for its intent; `short` gives it
    /// one line, which is what forty of them need.
    pub rows: String,
}

impl Default for Nav {
    fn default() -> Self {
        Nav {
            glyphs: "unicode".into(),
            glyphs_override: Vec::new(),
            attention: "when-needed".into(),
            rows: "tall".into(),
        }
    }
}

/// One mark, replaced.
///
/// `cells` is not optional and has no sensible default: it is how many columns
/// the terminal gives this text, which dirk cannot measure and must not guess.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GlyphDef {
    pub name: String,
    pub text: String,
    pub cells: u16,
}

impl Config {
    /// The harnesses to recognise: the shipped ones, plus the file's.
    ///
    /// A block whose name matches a shipped one **replaces** it whole rather
    /// than merging field by field. Predictable beats clever -- somebody
    /// overriding claude's markers does not want to inherit half of ours -- and
    /// a merge would make "which markers am I actually using" unanswerable
    /// without reading two files.
    pub fn kinds(&self) -> Vec<crate::agent::Kind> {
        self.kinds_and_complaints().0
    }

    /// The same, and whatever was wrong with the files it read.
    ///
    /// Separated so that startup can print the complaints on a terminal that
    /// still exists, and a reload can hand them back to whoever asked for it.
    pub fn kinds_and_complaints(&self) -> (Vec<crate::agent::Kind>, Vec<String>) {
        let mut out = crate::agent::defaults();
        let mut said = Vec::new();

        // config.toml first, then the files, so that a file wins. A file is the
        // more specific statement: somebody wrote a document about that one
        // harness, and the block in config.toml is the older way of saying the
        // same thing.
        for def in &self.agents {
            replace(&mut out, def.kind(crate::agent::From::Config));
        }
        for (def, from) in agent_files(&mut said) {
            replace(&mut out, def.kind(from));
        }
        (out, said)
    }
}

/// Replace the rules of that name whole, or add them.
///
/// Whole rather than merged: somebody overriding claude's markers does not want
/// to inherit half of ours, and a merge would leave them with a set they never
/// wrote and cannot see.
fn replace(out: &mut Vec<crate::agent::Kind>, kind: crate::agent::Kind) {
    match out.iter().position(|k| k.name == kind.name) {
        Some(i) => out[i] = kind,
        None => out.push(kind),
    }
}

/// Every `agents/<name>.toml`, in name order.
///
/// One file per harness so that fixing one marker is an edit to a document
/// about that harness rather than to the file that also holds your projects,
/// your boards and your theme -- and so that the fix is a thing you can hand to
/// somebody.
///
/// A file that does not parse is complained about and skipped. Detection rules
/// that fail to load must not be able to take out the session that was going to
/// draw with them.
fn agent_files(said: &mut Vec<String>) -> Vec<(AgentDef, crate::agent::From)> {
    let dir = config_home().join("dirk").join("agents");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "toml"))
        .collect();
    // Ordered, so that two files cannot decide between themselves which of them
    // was read last.
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                said.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        match toml::from_str::<AgentFile>(&text) {
            Ok(file) => {
                let mut def = file.into_def();
                // The filename names the harness. A `name` inside that
                // disagreed with it would leave two ways to say which harness a
                // file is about, and no way to tell which one won.
                def.name = stem.to_string();
                out.push((def, crate::agent::From::File));
            }
            Err(e) => said.push(format!("{}: {e}", path.display())),
        }
    }
    out
}

impl Config {
    /// Every program whose command line is worth reading: the interpreters dirk
    /// ships with, plus whatever the file added.
    pub fn wrappers(&self) -> Vec<String> {
        crate::agent::INTERPRETERS
            .iter()
            .map(|s| s.to_string())
            .chain(self.wrappers.iter().cloned())
            .collect()
    }

    /// Which key runs what, after the file has had its say.
    pub fn keys(&self) -> crate::action::Keys {
        let mut keys = crate::action::Keys::default();
        for (name, bind) in &self.keys {
            if let Some(action) = crate::action::Action::named(name) {
                keys.bind_all(action, &bind.all());
            }
        }
        keys
    }

    /// Which agent to start in a project: what it asks for, then the global
    /// answer, then nothing -- which means asking.
    ///
    /// Twelve repositories do not want one answer, and being asked the same
    /// question twelve times a day is how a shortcut stops being one.
    pub fn agent_for(&self, path: &std::path::Path) -> Option<String> {
        let named = self
            .projects
            .iter()
            .find(|p| expand(&p.path.to_string_lossy()) == path)
            .map(|p| p.agent.clone())
            .filter(|a| !a.is_empty());
        named
            .or_else(|| Some(self.default_agent.clone()))
            .filter(|a| !a.is_empty())
    }
}

impl Nav {
    /// The marks to draw with, after the overrides.
    pub fn glyphs(&self) -> crate::glyph::Glyphs {
        let mut set = crate::glyph::Glyphs::set(&self.glyphs);
        for def in &self.glyphs_override {
            if let Some(g) = crate::glyph::G::named(&def.name) {
                set.set_one(g, &def.text, def.cells);
            }
        }
        set
    }

    /// Whether the attention zone is drawn when it is empty, or at all.
    pub fn attention_always(&self) -> bool {
        self.attention == "always"
    }

    pub fn attention_never(&self) -> bool {
        self.attention == "never"
    }

    /// Whether a workspace gets a second line for what is happening in it.
    pub fn tall(&self) -> bool {
        self.rows == "tall"
    }

    /// Whether the one line a workspace gets carries what it is *doing* rather
    /// than what it *is*.
    ///
    /// For somebody whose projects are one checkout each: the branch is `main`
    /// on every row and says nothing, while the intent is the whole of what
    /// distinguishes them.
    pub fn leads_with_intent(&self) -> bool {
        self.rows == "intent"
    }
}

/// When dirk is allowed to interrupt you.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Notify {
    pub enabled: bool,
    /// Floor between two notifications about the same workspace. An agent that
    /// blocks, unblocks and blocks again inside a minute is one interruption.
    pub min_interval_ms: u64,
}

impl Default for Notify {
    fn default() -> Self {
        Self {
            enabled: true,
            min_interval_ms: 60_000,
        }
    }
}

/// One key, or several ways of reaching the same action.
///
/// A bare string is what every binding was before, and what most of them still
/// want to be. A list is for an action you want both after the prefix and on a
/// chord of its own: the prefix form is the one somebody reading the manual
/// finds, and the direct chord is the one their hands learn.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Bind {
    One(String),
    Many(Vec<String>),
}

impl Bind {
    pub fn all(&self) -> Vec<String> {
        match self {
            Bind::One(k) => vec![k.clone()],
            Bind::Many(k) => k.clone(),
        }
    }
}

/// Chrome that is neither the nav nor the terminal inside a pane.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Ui {
    /// `auto`, `always` or `off`.
    ///
    /// A rule, not a border. Three sides of a box only repeat what the
    /// neighbouring pane's own edge already says, and the fourth is a row of
    /// terminal nobody gets to use — so dirk draws the top line and nothing
    /// else, and that line carries the label and the focus mark.
    ///
    /// `auto` draws one where there is something to say: a pane with a label,
    /// which is how a five-pane board says which panel is which. `always`
    /// draws one on every pane, which is what tells two shells side by side
    /// apart and which pane has the keyboard. `off` draws none.
    pub pane_rules: String,
    /// What dirk writes as the title of the terminal it is running in.
    ///
    /// dirk emulates the terminals in its panes, so a title written inside one
    /// stops at dirk — and dirk wrote none of its own, which left the window
    /// holding fourteen panes labelled whatever it was called before dirk
    /// started. Window managers, tab bars and application switchers all read
    /// that.
    ///
    /// `{host}`, `{workspace}`, `{tab}` and `{pane}` are the tokens; `{{` and
    /// `}}` are literal braces. A token with no value renders empty rather than
    /// as its own name. Empty leaves the outer title alone.
    pub window_title: String,
    /// Whether dirk asks the terminal for mouse events at all.
    ///
    /// All or nothing. A half-captured mouse is a mode to remember, and the
    /// reason dirk captures at all is that it can then decide per pane whether
    /// the program inside wanted the click. Off gives the outer terminal its
    /// own selection back, and everything a click reaches has a key.
    pub mouse: bool,
}

impl Default for Ui {
    fn default() -> Self {
        Self {
            pane_rules: "auto".into(),
            // The workspace, which is what somebody alt-tabbing is looking for.
            // Not the host: on the machine you are sitting at it is noise, and
            // `--remote` is the case that wants it and can say so.
            window_title: "{workspace}".into(),
            mouse: true,
        }
    }
}

impl Ui {
    /// The title to write now, or `None` when dirk should leave it alone.
    ///
    /// Rendered here rather than by the client, because these are the session's
    /// facts: `{host}` names the machine the panes are running on, which under
    /// `--remote` is not the machine the window is on.
    pub fn title(&self, host: &str, workspace: &str, tab: &str, pane: &str) -> Option<String> {
        if self.window_title.trim().is_empty() {
            return None;
        }
        let mut out = String::with_capacity(self.window_title.len());
        let mut rest = self.window_title.as_str();
        while let Some(open) = rest.find(['{', '}']) {
            out.push_str(&rest[..open]);
            let c = rest.as_bytes()[open] as char;
            let after = &rest[open + 1..];
            // `{{` and `}}` are one literal brace, so a title can contain one.
            if after.starts_with(c) {
                out.push(c);
                rest = &after[1..];
                continue;
            }
            if c == '}' {
                out.push('}');
                rest = after;
                continue;
            }
            let Some(close) = after.find('}') else {
                out.push('{');
                rest = after;
                continue;
            };
            out.push_str(match &after[..close] {
                "host" => host,
                "workspace" => workspace,
                "tab" => tab,
                "pane" => pane,
                // Rendered as nothing rather than as its own name: a title with
                // `{wokspace}` in it should be short, not wrong.
                _ => "",
            });
            rest = &after[close + 1..];
        }
        out.push_str(rest);
        Some(out.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    /// Should this pane give its top row to a rule?
    pub fn ruled(&self, labelled: bool) -> bool {
        match self.pane_rules.as_str() {
            "always" => true,
            "off" => false,
            _ => labelled,
        }
    }
}

/// How a pane's shell is started, and where.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Terminal {
    /// `auto`, `login` or `non_login`.
    ///
    /// A login shell reads the files that build a login `PATH` — on macOS that
    /// is `/usr/libexec/path_helper` and Homebrew's initialisation, both in
    /// `/etc/zprofile` and neither read by any other kind of shell. Without
    /// this, `PATH` inside a dirk pane was missing entries it has in every
    /// other terminal on the machine, which people reasonably diagnosed as dirk
    /// being broken.
    ///
    /// `auto` is login on macOS and unchanged elsewhere, because that is where
    /// the problem is and because Linux distributions put the same entries in
    /// files every interactive shell reads. A shell with no `-l` wants
    /// `non_login`.
    pub shell_mode: String,
    /// Whether dirk carries images through to the terminal it is running in.
    ///
    /// On, because a pane that cannot draw an image is the largest single thing
    /// that makes it not a real terminal — and a terminal that cannot draw one
    /// ignores what dirk sends, so the cost of being wrong is nothing. Off is
    /// for somebody whose terminal does something worse than ignore it.
    pub graphics: bool,
    /// `follow`, `home`, `current`, or a path.
    ///
    /// Where a pane made beside another one starts. `follow` inherits from the
    /// pane it was made from, which is what dirk has always done and what
    /// splitting usually means.
    pub new_cwd: String,
}

impl Default for Terminal {
    fn default() -> Self {
        Self {
            graphics: true,
            shell_mode: "auto".into(),
            new_cwd: "follow".into(),
        }
    }
}

impl Terminal {
    /// Should a shell started now be a login shell?
    pub fn login(&self) -> bool {
        match self.shell_mode.as_str() {
            "login" => true,
            "non_login" => false,
            // Only where the problem is. Linux distributions put the same
            // entries in files every interactive shell reads.
            _ => cfg!(target_os = "macos"),
        }
    }

    /// Where a pane made from `from` should start.
    pub fn start_in(&self, from: &Path) -> PathBuf {
        match self.new_cwd.as_str() {
            "home" => home(),
            // dirk's own directory, which is where it was launched from and is
            // what somebody who wants "wherever I started this" means.
            "current" => std::env::current_dir().unwrap_or_else(|_| home()),
            "follow" | "" => from.to_path_buf(),
            path => expand(path),
        }
    }
}

/// What a session brings back when it starts again.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SessionCfg {
    /// Whether a restored pane whose agent left a session reference is started
    /// on that conversation rather than as a shell.
    ///
    /// On, because restoring the shape of the work and losing the work is close
    /// to the worst place to stop. Off is for somebody who would rather decide
    /// each time, and for whom twelve agents coming back at once is twelve
    /// model sessions they did not ask for.
    pub resume_agents: bool,
    /// Whether each pane's last screen is written to disk and painted back when
    /// the session is restored.
    ///
    /// **Off, and it stays off unless you say otherwise.** A pane's output holds
    /// whatever went past in it: tokens echoed by a failed request, keys in an
    /// environment somebody dumped while debugging, the contents of a file that
    /// should not have been catted. Turning this on writes that to
    /// `sessions/<name>.history.json` in your configuration directory, in plain
    /// text, where it stays until the session is written again.
    ///
    /// That is a reasonable trade for a machine you alone use and an
    /// unreasonable one for a shared host, and dirk is not in a position to know
    /// which it is on. Turning it off again deletes what was already stored.
    pub pane_history: bool,
    /// Whether `dirk session handoff` will replace the running binary without
    /// stopping the work.
    ///
    /// **Experimental, and off.** It clears `FD_CLOEXEC` on every pty and
    /// `exec`s the new binary over this process, so every shell and agent keeps
    /// the parent and the terminal it had. When it works nothing notices; when
    /// it does not, the thing at risk is every running pane in the session,
    /// which is the most expensive thing dirk holds.
    ///
    /// The new binary is run once before anything is committed to, so the
    /// realistic failure -- a truncated download, the wrong architecture -- is
    /// caught while this process is still entirely intact. What that cannot
    /// catch is a binary that starts and then fails, and that is the reason
    /// this is off.
    pub handoff: bool,
}

impl Default for SessionCfg {
    fn default() -> Self {
        Self {
            resume_agents: true,
            pane_history: false,
            handoff: false,
        }
    }
}

/// How big the session is when nobody is looking at it.
///
/// A pane is sized from the client watching it, and a session driven from a
/// script has no client. Without an answer here the geometry of a workspace
/// created by `dirk workspace create` would be whatever the last person to
/// attach happened to have, or the size the server started with -- so what a
/// caller reads back from a pane it created depends on a terminal that is not
/// there.
///
/// 80x24 because that is what a terminal is when nobody has said otherwise,
/// and because it is what dirk already did. Raise it for orchestration: an
/// agent's output read back at 80 columns is an agent's output with the wrap
/// points of a screen nobody saw.
#[derive(Debug, Clone, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Server {
    pub headless_cols: u16,
    pub headless_rows: u16,
}

impl Default for Server {
    fn default() -> Self {
        Self {
            headless_cols: 80,
            headless_rows: 24,
        }
    }
}

/// The naming policy, as knobs.
///
/// Every default is what the policy did before it was configurable, so a file
/// that sets none of these changes nothing.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Naming {
    pub enabled: bool,
    /// How long a title must hold still before it is committed.
    pub debounce_ms: u64,
    /// Floor between two renames of one workspace.
    pub min_interval_ms: u64,
    /// Token overlap above which a new intent is "the same thing, reworded".
    pub similarity_threshold: f32,
    /// Never overwrite a name a human set. This is what stops the policy
    /// fighting you.
    pub respect_manual_names: bool,
    /// A blocked agent's title describes the dialog, not the work.
    pub skip_while_blocked: bool,
    /// Drop a leading project name, so a row under `ptop` does not read
    /// `ptop-adopt-remaining-lessons`.
    pub strip_project_prefix: bool,
    /// Titles that are programs rather than intents. Compared case-insensitively
    /// against the whole title.
    pub ignore_titles: Vec<String>,
    /// Mark an agent that has stopped revising its title. A signal for a
    /// human, never acted on.
    pub show_stale: bool,
    /// How many state transitions with no new intent count as stale.
    pub stale_after_turns: u32,
    pub targets: Targets,
    pub templates: Templates,
    pub sources: Sources,
}

/// Where an intent can come from.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Sources {
    pub llm: Llm,
}

/// A second source, for panes whose title says nothing.
///
/// Off unless asked for. The primary source is the agent's own terminal title,
/// which costs nothing and needs no key; this exists for the case that has no
/// title at all.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Llm {
    pub enabled: bool,
    pub endpoint: String,
    pub model: String,
    /// The *name* of the variable holding the key, never the key. A
    /// configuration file is a thing people paste into issues.
    pub api_key_env: String,
    pub timeout_ms: u64,
    /// How much of the screen to send, in characters.
    pub max_chars: usize,
    /// How many lines of the pane count as "the screen".
    pub viewport_lines: u16,
    /// Floor between two questions about one workspace. A naming call per turn
    /// per workspace, all day, for a caption, is not a trade anyone would make
    /// on purpose.
    pub interval_ms: u64,
}

impl Default for Llm {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "https://api.anthropic.com/v1/messages".into(),
            model: "claude-opus-5".into(),
            api_key_env: "ANTHROPIC_API_KEY".into(),
            // Generous: this is a non-streaming request to a model that thinks
            // before it answers, and a timeout is indistinguishable from "no
            // candidate" -- so one set too tight makes the feature quietly
            // never work.
            timeout_ms: 30_000,
            max_chars: 4_000,
            viewport_lines: 60,
            interval_ms: 600_000,
        }
    }
}

/// What naming is allowed to name.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Targets {
    pub workspace: bool,
    pub agent: bool,
}

impl Default for Targets {
    fn default() -> Self {
        Self {
            workspace: true,
            agent: true,
        }
    }
}

/// How each name is arranged, over the tokens in `tokens.rs`.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct Templates {
    pub workspace: String,
    pub agent: String,
}

impl Default for Templates {
    fn default() -> Self {
        Self {
            workspace: "{intent}".into(),
            agent: "{intent-slug}".into(),
        }
    }
}

/// Programs that show their own name as a terminal title. Without this list
/// they arrive as intents and become workspace labels.
fn default_ignore_titles() -> Vec<String> {
    [
        "claude",
        "claude code",
        "codex",
        "pi",
        "copilot",
        "cursor",
        "droid",
        "aider",
        "goose",
        "bash",
        "zsh",
        "fish",
        "sh",
        "nu",
        "pwsh",
        "powershell",
        "nvim",
        "vim",
        "nano",
        "helix",
        "hx",
        "emacs",
        "less",
        "man",
        "node",
        "python",
        "irb",
        "psql",
        "lazygit",
        "htop",
        "top",
        "btop",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

impl Default for Naming {
    fn default() -> Self {
        Self {
            enabled: true,
            debounce_ms: 1200,
            min_interval_ms: 15_000,
            similarity_threshold: 0.6,
            respect_manual_names: true,
            skip_while_blocked: true,
            strip_project_prefix: true,
            ignore_titles: default_ignore_titles(),
            show_stale: true,
            stale_after_turns: 6,
            targets: Targets::default(),
            templates: Templates::default(),
            sources: Sources::default(),
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            identity: Identity::default(),
            projects_root: home().join("Code"),
            layouts: default_layouts(),
            shell: String::new(),
            sidebar_width: 34,
            scrollback: 5000,
            naming: Naming::default(),
            notify: Notify::default(),
            server: Server::default(),
            terminal: Terminal::default(),
            ui: Ui::default(),
            session: SessionCfg::default(),
            sound: Sound::default(),
            clipboard: Vec::new(),
            keys: std::collections::BTreeMap::new(),
            nav: Nav::default(),
            wrappers: Vec::new(),
            agents: Vec::new(),
            default_agent: String::new(),
            projects: Vec::new(),
        }
    }
}

/// Three single-program layouts. One whose program is not installed is dropped
/// at startup rather than left to fail on first open — an entry that only ever
/// shows `command not found` is worse than no entry.
/// Written out rather than derived, because `#[serde(default = "yes")]` is a
/// *deserialization* default and does not reach `LayoutDef::default()` -- which
/// is how every shipped board was born with `keep: false` and shut itself down
/// the first time you looked at another one.
impl Default for LayoutDef {
    fn default() -> Self {
        LayoutDef {
            name: String::new(),
            key: None,
            command: Vec::new(),
            split: String::new(),
            pane: Vec::new(),
            status: None,
            keep: true,
        }
    }
}

fn default_layouts() -> Vec<LayoutDef> {
    let one = |name: &str, key: char, command: &[&str]| LayoutDef {
        name: name.into(),
        key: Some(key),
        command: command.iter().map(|s| (*s).to_string()).collect(),
        ..LayoutDef::default()
    };
    let pane = |title: &str, command: &[&str], size: &str| PaneDef {
        title: title.into(),
        command: command.iter().map(|s| (*s).to_string()).collect(),
        size: size.into(),
        ..PaneDef::default()
    };

    vec![
        // A dashboard, when the machine has the programs for one. It is
        // dropped like any other layout if they are not installed, so a fresh
        // install gets it without configuring anything and never gets a broken
        // version of it.
        LayoutDef {
            name: "Overview".into(),
            key: Some('1'),
            split: "rows".into(),
            pane: vec![
                pane("roadmap", &["cairn", "roadmap"], "10"),
                PaneDef {
                    split: "cols".into(),
                    pane: vec![
                        pane("ptop", &["ptop"], ""),
                        pane("board", &["cairn", "board"], "40%"),
                    ],
                    ..PaneDef::default()
                },
            ],
            ..LayoutDef::default()
        },
        one("ptop", '2', &["ptop"]),
        one("lazygit", '3', &["lazygit"]),
        one("cairn", '4', &["cairn", "board"]),
    ]
}

/// Expand a leading `~`, which is the only shell expansion a path in a config
/// file can reasonably expect.
pub fn expand(path: &str) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home().join(rest),
        None if path == "~" => home(),
        None => PathBuf::from(path),
    }
}

pub fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// What is wrong with a configuration that still loads.
///
/// Separate from reading it because the same complaints are owed to whoever
/// asked, whether that is a terminal at startup or a client that said `reload`.
/// Chords something else on the machine has already claimed.
///
/// `ctrl+alt` is the one modifier family terminals leave alone, which is why it
/// is the family to suggest — but a handful of them belong to the desktop, and
/// a binding the terminal never receives is one that appears not to have worked
/// with nothing to say why. Named here rather than silently, because dirk
/// cannot detect it and the person choosing can.
const TAKEN: &[(&str, &str)] = &[
    ("ctrl+alt+t", "the terminal launcher on Ubuntu and Fedora"),
    ("ctrl+alt+l", "the lock screen on KDE"),
    ("ctrl+alt+a", "the attention window on KDE"),
    ("ctrl+alt+s", "Konsole"),
    ("ctrl+alt+u", "Konsole"),
    ("ctrl+alt+left", "workspace switching on GNOME, and Ghostty"),
    (
        "ctrl+alt+right",
        "workspace switching on GNOME, and Ghostty",
    ),
    ("ctrl+alt+up", "workspace switching on GNOME, and Ghostty"),
    ("ctrl+alt+down", "workspace switching on GNOME, and Ghostty"),
];

pub fn complaints(cfg: &Config) -> Vec<String> {
    let mut out = Vec::new();
    for (name, bind) in &cfg.keys {
        if crate::action::Action::named(name).is_none() {
            out.push(format!("keys.{name:?}: no action of that name"));
            continue;
        }
        for key in bind.all() {
            let lower = key.to_lowercase();
            if crate::action::Binding::parse(&key).is_none() {
                out.push(format!("keys.{name}: {key:?} is not a key dirk can read"));
                continue;
            }
            if let Some((_, who)) = TAKEN.iter().find(|(c, _)| *c == lower) {
                out.push(format!(
                    "keys.{name}: {key:?} is usually taken by {who}, so dirk may never see it"
                ));
            }
            // Two families that reach dirk and mean something else to whatever
            // is in the pane. `ctrl+j` is Enter to every shell and editor.
            if matches!(lower.as_str(), "ctrl+j" | "ctrl+m" | "ctrl+i" | "ctrl+h") {
                out.push(format!(
                    "keys.{name}: {key:?} is what a terminal sends for a key programs \
                     already use; a pane will never see that key again"
                ));
            }
        }
    }
    if !matches!(cfg.ui.pane_rules.as_str(), "auto" | "always" | "off") {
        out.push(format!(
            "ui.pane_rules: {:?} is not auto, always or off; using auto",
            cfg.ui.pane_rules
        ));
    }
    if !matches!(
        cfg.terminal.shell_mode.as_str(),
        "auto" | "login" | "non_login"
    ) {
        out.push(format!(
            "terminal.shell_mode: {:?} is not auto, login or non_login; using auto",
            cfg.terminal.shell_mode
        ));
    }
    let known: Vec<String> = cfg.kinds().into_iter().map(|k| k.name).collect();
    for (name, says) in &cfg.sound.per_agent {
        if !matches!(says.as_str(), "on" | "off" | "default") {
            out.push(format!(
                "sound.agents.{name}: {says:?} is not on, off or default; ignored"
            ));
        }
        // A name nobody recognises is a rule that can never apply, and silence
        // about it is exactly the shape of "I turned that off and it still
        // rings".
        if !known.contains(name) {
            out.push(format!("sound.agents.{name}: no harness of that name"));
        }
    }
    for l in &cfg.layouts {
        for bad in crate::mux::layout::bad_sizes(l) {
            out.push(format!(
                "layout {}: bad size {bad:?}; using an even share",
                l.name
            ));
        }
        // Command keys win, so a layout bound to one is unreachable. Silently
        // is the problem: the entry is listed with a key that does nothing.
        const RESERVED: &[char] = &[
            'n', 'o', 'a', 'x', 'r', 'v', 's', 'd', 'b', 'w', 'q', 'j', 'k', ';', '[', '/', 'z',
            '{', '}',
        ];
        if let Some(k) = l.key.filter(|k| RESERVED.contains(k)) {
            out.push(format!(
                "layout {}: key {k:?} is a command key and will not reach it",
                l.name
            ));
        }
        if !l.runnable() {
            out.push(format!("layout {}: nothing to run; dropped", l.name));
        }
    }
    // Layouts are matched to what is open by name, so a name used twice is one
    // layout you can open and one you cannot -- and nothing to say which.
    for (i, l) in cfg.layouts.iter().enumerate() {
        if cfg.layouts[..i].iter().any(|e| e.name == l.name) {
            out.push(format!(
                "layout {}: defined twice; only the first is reachable",
                l.name
            ));
        }
    }
    // A binding for something that does not exist is a line in a file that
    // looks like it works. Reported with the nearest thing it might have meant,
    // since the usual cause is remembering the name slightly wrong.
    for name in cfg.keys.keys() {
        if crate::action::Action::named(name).is_none() {
            out.push(format!(
                "keys: nothing called {name:?}. `dirk --keys` lists them"
            ));
        }
    }
    // Two things on one key means one of them is unreachable, and which one is
    // an accident of ordering.
    let bound = cfg.keys();
    for a in crate::action::Action::ALL {
        if bound.key(*a).is_none() {
            out.push(format!(
                "keys: {} has no key; something else took it",
                a.name()
            ));
        }
    }
    // An agent that can never match anything is a definition that does nothing,
    // and saying so is cheaper than wondering why a harness is never
    // recognised.
    for l in &cfg.layouts {
        if let Some(status) = &l.status {
            if status.run.is_empty() {
                out.push(format!("board {}: a status with nothing to run", l.name));
            }
            match parse_every(&status.every) {
                None => out.push(format!(
                    "board {}: every = {:?} is not a duration like \"10s\"",
                    l.name, status.every
                )),
                // Said rather than silently obeyed: a board somebody set to a
                // tenth of a second is not going to run at a tenth of a second,
                // and finding that out by measurement is worse than being told.
                Some(d) if d < std::time::Duration::from_secs(2) => out.push(format!(
                    "board {}: every = {:?} is below the two second floor",
                    l.name, status.every
                )),
                Some(_) => {}
            }
        }
    }
    for def in &cfg.agents {
        if def.names.is_empty() && def.argv.is_empty() && def.name.is_empty() {
            out.push("agent: an entry with no name, names or argv can never match".into());
        }
    }
    // A set that does not exist is drawn as the default one, and a mark that
    // does not exist is not drawn at all -- both silently, and both looking
    // exactly like a setting that did not take.
    //
    // The list lives with the sets rather than here: a second copy of it is a
    // copy that goes stale the day a third set is added, and it did.
    let sets = crate::glyph::Glyphs::SETS;
    if !sets.contains(&cfg.nav.glyphs.as_str()) {
        out.push(format!(
            "nav: no glyph set {:?}; using unicode. One of {}",
            cfg.nav.glyphs,
            sets.join(", ")
        ));
    }
    for def in &cfg.nav.glyphs_override {
        if crate::glyph::G::named(&def.name).is_none() {
            out.push(format!("nav: no glyph called {:?}", def.name));
        }
    }
    for (field, value, allowed) in [
        (
            "attention",
            &cfg.nav.attention,
            &["when-needed", "always", "never"][..],
        ),
        ("rows", &cfg.nav.rows, &["tall", "short", "intent"][..]),
    ] {
        if !allowed.contains(&value.as_str()) {
            out.push(format!(
                "nav: {field} = {value:?} is not one of {}",
                allowed.join(", ")
            ));
        }
    }
    // The same reason: both fall through to a default, so `host = "alway"`
    // behaves as `remote` and looks exactly like a setting that did not take.
    for (field, value, allowed) in [
        (
            "session",
            &cfg.identity.session,
            &["never", "named", "always"][..],
        ),
        (
            "host",
            &cfg.identity.host,
            &["never", "remote", "always"][..],
        ),
    ] {
        if !allowed.contains(&value.as_str()) {
            out.push(format!(
                "identity: {field} = {value:?} is not one of {}",
                allowed.join(", ")
            ));
        }
    }
    // A mistyped token renders as silence otherwise: a label simply shorter
    // than intended, with nothing to say why.
    for (what, template) in [
        ("workspace", &cfg.naming.templates.workspace),
        ("agent", &cfg.naming.templates.agent),
    ] {
        for token in crate::tokens::unknown(template) {
            out.push(format!("{what} template: no such token {{{token}}}"));
        }
    }
    out
}

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
}

impl Config {
    pub fn load() -> Self {
        let path = config_home().join("dirk").join("config.toml");
        let mut cfg = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("dirk: {}: {e}", path.display());
                Config::default()
            }),
            Err(_) => Config::default(),
        };
        // Reported here, before the terminal is taken over, because a message
        // printed after that is written onto the alternate screen and vanishes
        // with it.
        for line in complaints(&cfg) {
            eprintln!("dirk: {line}");
        }
        // A rule file that does not parse is said out loud and then ignored.
        // Detection rules that fail to load must not be able to take out the
        // session that was going to draw with them.
        for line in cfg.kinds_and_complaints().1 {
            eprintln!("dirk: {line}");
        }
        cfg.layouts.retain(|l| l.runnable());
        cfg
    }

    /// Read the file again and apply what can be applied to a running session.
    ///
    /// A file that does not parse is reported and the running configuration is
    /// kept: a typo should not cost you the session you were working in.
    ///
    /// Answers with whatever it had to say about the file. At startup those go
    /// to stderr, where there is a terminal to read them; here there is not,
    /// and a reload that quietly dropped a layout would be a reload that
    /// reported success for removing something.
    pub fn reload(
        current: &mut Config,
        session: &mut crate::mux::Session,
    ) -> Result<Vec<String>, String> {
        let path = config_home().join("dirk").join("config.toml");
        let next = match std::fs::read_to_string(&path) {
            Ok(text) => toml::from_str::<Config>(&text).map_err(|e| e.to_string())?,
            // No file is a valid configuration: the defaults. Anything else is
            // a failure to read one that exists, and quietly installing the
            // defaults there would replace the layouts of somebody whose home
            // directory blinked.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut next = next;
        let said = complaints(&next);
        next.layouts.retain(|l| l.runnable());

        // Layouts that are open keep the panes they already have; the rest of
        // the list is replaced. Rebuilding an open dashboard because a colour
        // changed is not a reload, it is a restart.
        session.merge_layouts(&next.layouts);
        session.set_naming(&next.naming);
        // Cached at construction, so a reload that left them alone would report
        // success for a change that never reached anything. They take effect on
        // the next pane, which is what changing a shell means.
        session.shell = next.shell();
        // Cached beside the shell and for the same reason: both take effect on
        // the next pane, which is what changing either of them means.
        session.terminal = next.terminal.clone();
        session.ui = next.ui.clone();
        session.set_scrollback(next.scrollback);
        *current = next;
        Ok(said)
    }

    pub fn shell(&self) -> String {
        if !self.shell.is_empty() {
            return self.shell.clone();
        }
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into())
    }
}

/// Is this program actually runnable? Absolute paths are checked directly;
/// bare names are looked up on PATH.
fn on_path(cmd: &str) -> bool {
    let p = Path::new(cmd);
    if p.is_absolute() || cmd.contains('/') {
        return p.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| std::env::split_paths(&paths).any(|d| d.join(cmd).is_file()))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Config {
        toml::from_str::<Config>(text).expect("parses")
    }

    #[test]
    fn a_board_keeps_its_panes_unless_it_says_otherwise() {
        // `#[serde(default = ...)]` is a deserialization default and does not
        // reach `LayoutDef::default()`, which is what `default_layouts` builds
        // every shipped board through. They were all born `keep: false` and
        // shut themselves down the first time you looked at another one.
        assert!(LayoutDef::default().keep, "the struct default disagrees");
        for board in default_layouts() {
            assert!(board.keep, "shipped board {} does not keep", board.name);
        }
        let named: Config =
            toml::from_str("[[board]]\nname = \"x\"\ncommand = [\"sh\"]\nkeep = false\n")
                .expect("parses");
        assert!(!named.layouts[0].keep, "an explicit false was ignored");
        let quiet: Config =
            toml::from_str("[[board]]\nname = \"y\"\ncommand = [\"sh\"]\n").expect("parses");
        assert!(
            quiet.layouts[0].keep,
            "a board that said nothing lost its panes"
        );
    }

    #[test]
    fn a_status_interval_is_a_duration_and_has_a_floor() {
        assert_eq!(parse_every("10s"), Some(std::time::Duration::from_secs(10)));
        assert_eq!(parse_every("2m"), Some(std::time::Duration::from_secs(120)));
        assert_eq!(
            parse_every("1h"),
            Some(std::time::Duration::from_secs(3600))
        );
        assert_eq!(parse_every("30"), Some(std::time::Duration::from_secs(30)));
        assert_eq!(parse_every("soon"), None);

        // Four boards at ten seconds is already twenty-four processes a
        // minute. Somebody writing a tenth of a second has not thought about
        // what they are asking for, and is told rather than obeyed.
        let quick = StatusDef {
            run: vec!["true".into()],
            every: "0s".into(),
        };
        assert_eq!(quick.interval(), std::time::Duration::from_secs(2));
        let mut cfg = Config::default();
        cfg.layouts.push(LayoutDef {
            name: "fast".into(),
            status: Some(quick),
            ..LayoutDef::default()
        });
        assert!(
            complaints(&cfg).iter().any(|c| c.contains("floor")),
            "an interval under the floor was accepted in silence"
        );
    }

    #[test]
    fn a_window_title_is_arranged_by_its_template() {
        let ui = |t: &str| Ui {
            window_title: t.into(),
            ..Ui::default()
        };
        assert_eq!(
            ui("{host}: {workspace}").title("build", "the parser", "", ""),
            Some("build: the parser".to_string())
        );
        // A token with no value renders empty rather than as its own name, and
        // what is left is collapsed — so a template written for a session with
        // tabs does not leave a hole in one without them.
        assert_eq!(
            ui("{workspace} · {tab}").title("build", "the parser", "", ""),
            Some("the parser ·".to_string())
        );
        // A name dirk does not publish is nothing, not itself: a title with
        // `{wokspace}` in it should be short, not wrong.
        assert_eq!(
            ui("[{wokspace}]").title("h", "w", "", ""),
            Some("[]".into())
        );
        // Braces can be written.
        assert_eq!(
            ui("{{{workspace}}}").title("h", "w", "", ""),
            Some("{w}".into())
        );
        // And empty leaves the terminal's own title alone.
        assert_eq!(ui("").title("h", "w", "t", "p"), None);
        assert_eq!(ui("   ").title("h", "w", "t", "p"), None);
    }

    #[test]
    fn a_harness_can_be_the_only_one_that_makes_a_noise() {
        // The shape of the problem when three are running: one is chatty and
        // the other two are not, and the answers available were all or none.
        let cfg = parsed(
            r#"
            [sound]
            enabled = true

            [sound.agents]
            claude = "off"
            codex = "on"
            aider = "default"
            "#,
        );
        assert_eq!(cfg.sound.about(Some("claude")), Says::No);
        assert_eq!(cfg.sound.about(Some("codex")), Says::Yes);
        // `default` and absent are the same answer: say nothing, and let the
        // project or the global setting decide.
        assert_eq!(cfg.sound.about(Some("aider")), Says::Nothing);
        assert_eq!(cfg.sound.about(Some("goose")), Says::Nothing);
        assert_eq!(cfg.sound.about(None), Says::Nothing);
    }

    #[test]
    fn a_sound_rule_that_can_never_apply_is_complained_about() {
        // Silence here is the shape of "I turned that off and it still rings".
        let cfg = parsed(
            r#"
            [sound.agents]
            nosuchagent = "off"
            claude = "quiet"
            "#,
        );
        let said = complaints(&cfg);
        assert!(
            said.iter().any(|c| c.contains("no harness of that name")),
            "a rule for a harness that does not exist passed in silence: {said:?}"
        );
        assert!(
            said.iter().any(|c| c.contains("not on, off or default")),
            "a value nobody understands passed in silence: {said:?}"
        );
        // And the one it does not understand says nothing rather than guessing.
        assert_eq!(cfg.sound.about(Some("claude")), Says::Nothing);
    }

    #[test]
    fn a_harness_from_the_file_is_recognised_like_a_shipped_one() {
        // The whole point: the field adds one a month, and an addition should
        // be an edit rather than a release.
        let cfg = parsed(
            r#"
            [[agent]]
            name = "sculptor"
            blocked = { menu = true, match = ["Proceed?"] }
            "#,
        );
        let kinds = cfg.kinds();
        let it = kinds.iter().find(|k| k.name == "sculptor").expect("added");
        // A name is enough: it stands for the process name and the command too.
        assert_eq!(it.names, ["sculptor"]);
        assert_eq!(it.command, ["sculptor"]);
        assert_eq!(
            crate::agent::identify(
                &crate::agent::Proc {
                    program: "sculptor".into(),
                    args: String::new(),
                },
                &kinds,
                &cfg.wrappers(),
            )
            .agent()
            .map(|k| k.name.as_str()),
            Some("sculptor")
        );
        // And the shipped ones are still there.
        assert!(kinds.iter().any(|k| k.name == "claude"));
    }

    #[test]
    fn overriding_a_shipped_harness_replaces_it_rather_than_merging() {
        // Predictable beats clever. Somebody replacing claude's markers does
        // not want to inherit half of ours, and a merge makes "which markers am
        // I actually using" unanswerable without reading two files.
        let cfg = parsed(
            r#"
            [[agent]]
            name = "claude"
            blocked = { menu = false, match = ["Ready:"] }
            "#,
        );
        let kinds = cfg.kinds();
        assert_eq!(
            kinds.iter().filter(|k| k.name == "claude").count(),
            1,
            "the shipped one was left beside the replacement"
        );
        let claude = kinds.iter().find(|k| k.name == "claude").unwrap();
        assert_eq!(claude.blocked, ["Ready:"]);
        assert!(!claude.choices);
    }

    #[test]
    fn there_is_one_gethostname_in_the_tree() {
        // There were two, and they were the same twenty lines twice. One typed
        // its buffer `[i8; 256]`, which is `c_char` on x86-64 and on Darwin and
        // not on aarch64 Linux -- so dirk stopped compiling for a target it
        // ships, and nothing said so until somebody asked that target.
        //
        // Scanned out of the source because the duplicate was not a call to
        // this function; it was a second one of it.
        let sources = [
            include_str!("config.rs"),
            include_str!("main.rs"),
            include_str!("server.rs"),
            include_str!("client.rs"),
        ];
        // Assembled rather than written: this file is one of the ones being
        // scanned, and a needle spelled out here would find itself.
        let needle = ["libc", "::", "gethostname"].concat();
        let calls: usize = sources
            .iter()
            .map(|text| text.matches(needle.as_str()).count())
            .sum();
        assert_eq!(calls, 1, "gethostname is wrapped more than once again");
    }

    #[test]
    fn the_host_name_is_short_and_has_no_domain_on_it() {
        // Whatever this machine is called, what comes back is one label: a bar
        // is four columns of information and eleven of domain otherwise.
        let Some(host) = hostname() else {
            return; // A machine with no name is not this test's business.
        };
        assert!(!host.is_empty());
        assert!(!host.contains('.'), "{host:?} still has its domain");
    }

    #[test]
    fn a_harness_that_can_never_match_is_reported() {
        let mut cfg = Config::default();
        cfg.agents.push(AgentDef {
            name: String::new(),
            names: Vec::new(),
            argv: Vec::new(),
            command: Vec::new(),
            resume: Vec::new(),
            blocked: BlockedDef::default(),
        });
        assert!(
            complaints(&cfg).iter().any(|c| c.starts_with("agent:")),
            "an entry matching nothing was accepted in silence"
        );
    }
}
