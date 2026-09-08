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

//! Questions answered later.
//!
//! Every other command is answered from the session as it is when it is asked.
//! "Tell me when this agent stops" cannot be: the answer does not exist yet.
//!
//! It is held rather than polled. A caller that wants to know when an agent
//! blocks would otherwise ask every few hundred milliseconds, and everybody
//! writes that loop, picks a different interval, and gets the edges wrong —
//! a state entered and left between two polls is one nobody sees.
//!
//! Nothing about the wire changes. The socket thread is already blocked on the
//! reply channel while the event loop answers, so a held question is simply one
//! whose reply is sent from a later turn of that loop — the turn the state
//! actually changed on, which is the only place the change is visible without
//! looking for it.

use crate::agent::State;
use crate::mux::PaneId;
use crate::wire::Reply;
use std::sync::mpsc::SyncSender;
use std::time::Instant;

/// A question waiting for the session to become the answer.
pub struct Held {
    /// Where the reply goes. Holding this is what keeps the caller waiting:
    /// its socket thread is blocked reading the other end.
    pub back: SyncSender<Reply>,
    pub what: What,
    /// `None` waits indefinitely, which is what a caller with its own patience
    /// asked for.
    pub deadline: Option<Instant>,
}

pub enum What {
    /// An agent reaching one of these states.
    Agent { pane: PaneId, until: Vec<State> },
}

/// Why a held question is finished, once it is.
pub enum Settled {
    /// It happened. The value is what the caller asked about, as it now is.
    Reached(serde_json::Value),
    /// It cannot happen any more. Distinct from a timeout: a caller that waited
    /// for an agent which then exited should retry nothing.
    Gone(&'static str),
    /// The patience ran out. Says nothing about whether it would have happened.
    Late,
}

impl Settled {
    pub fn reply(self) -> Reply {
        match self {
            Settled::Reached(v) => Reply::ok(v),
            Settled::Gone(why) => Reply::err(why),
            Settled::Late => Reply::err("timeout"),
        }
    }
}

/// Read `--until` and `--timeout` out of the words a caller sent.
///
/// The positional words come back in order and the options are taken out, so a
/// handler reads its target from `words[0]` whether or not flags were passed
/// and whichever side of the target they were passed on.
pub fn options(args: &[String]) -> (Vec<String>, Vec<(String, String)>) {
    let mut words = Vec::new();
    let mut opts = Vec::new();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        let Some(name) = arg.strip_prefix("--") else {
            words.push(arg.clone());
            continue;
        };
        match name.split_once('=') {
            Some((k, v)) => opts.push((k.to_string(), v.to_string())),
            // A flag whose value is missing takes an empty one rather than
            // swallowing the next word, which might be the target.
            None => opts.push((name.to_string(), rest.next().cloned().unwrap_or_default())),
        }
    }
    (words, opts)
}

/// The states `--until` means when a caller does not say.
///
/// The three that mean the agent has stopped needing the processor. `blocked`
/// is in there because a caller waiting for work to finish also wants to know
/// when it has stopped to ask a question — waiting out the timeout on an agent
/// sitting on a prompt is the worst of the available answers.
pub const SETTLED: &[State] = &[State::Blocked, State::Done, State::Idle];

/// Read the `--until` values, or the default set.
pub fn until(opts: &[(String, String)]) -> Result<Vec<State>, String> {
    let mut out = Vec::new();
    for (k, v) in opts {
        if k != "until" {
            continue;
        }
        match State::named(v) {
            Some(s) => out.push(s),
            None => return Err(format!("no such state: {v}")),
        }
    }
    Ok(match out.is_empty() {
        true => SETTLED.to_vec(),
        false => out,
    })
}

/// Read `--timeout`, in milliseconds.
pub fn deadline(opts: &[(String, String)]) -> Result<Option<Instant>, String> {
    let Some((_, v)) = opts.iter().find(|(k, _)| k == "timeout") else {
        return Ok(None);
    };
    match v.parse::<u64>() {
        Ok(ms) => Ok(Some(
            Instant::now() + std::time::Duration::from_millis(ms.max(1)),
        )),
        Err(_) => Err(format!("--timeout wants milliseconds, not {v:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| w.to_string()).collect()
    }

    #[test]
    fn options_come_out_wherever_they_were_put() {
        // A caller writes the flags on whichever side it finds natural, and the
        // target is the target either way.
        let (words, opts) = options(&strings(&["--until", "blocked", "w1:p2"]));
        assert_eq!(words, vec!["w1:p2"]);
        assert_eq!(opts, vec![("until".into(), "blocked".into())]);

        let (words, opts) = options(&strings(&["w1:p2", "--timeout=5000"]));
        assert_eq!(words, vec!["w1:p2"]);
        assert_eq!(opts, vec![("timeout".into(), "5000".into())]);
    }

    #[test]
    fn until_repeats_and_defaults_to_having_stopped() {
        assert_eq!(until(&[]).unwrap(), SETTLED);
        let opts = vec![
            ("until".to_string(), "idle".to_string()),
            ("until".to_string(), "done".to_string()),
        ];
        assert_eq!(until(&opts).unwrap(), vec![State::Idle, State::Done]);
    }

    #[test]
    fn a_state_that_does_not_exist_is_refused_at_the_door() {
        // Rather than waiting for ever for something that can never happen,
        // which is what a silently ignored `--until` would produce.
        let opts = vec![("until".to_string(), "finished".to_string())];
        assert!(until(&opts).is_err());
    }

    #[test]
    fn a_timeout_is_milliseconds_or_an_error() {
        assert!(deadline(&[]).unwrap().is_none());
        let ok = vec![("timeout".to_string(), "250".to_string())];
        assert!(deadline(&ok).unwrap().is_some());
        let bad = vec![("timeout".to_string(), "2s".to_string())];
        assert!(deadline(&bad).is_err());
    }
}
