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

//! Replacing the binary with a newer one.
//!
//! Upgrading was: find the release page, pick the right triple, download,
//! untar, `sudo install`. Six steps and a chance to get the architecture
//! wrong, which is why people stay several versions behind.
//!
//! ## What this refuses to do
//!
//! **Touch an installation it did not make.** Homebrew and nix own their
//! copies, and a binary swapped underneath one of them is a broken
//! installation two weeks later, when the package manager next has an opinion
//! about a file that is no longer the file it put there. Those are named and
//! refused, with their own command printed instead.
//!
//! **Have two channels.** Two is a project with a release manager, and this one
//! does not have one.
//!
//! ## Why curl and not a crate
//!
//! The same reason `llm.rs` shells out: a terminal multiplexer does not want a
//! TLS stack and its transitive tree in it, and `curl` is on every machine dirk
//! runs on. The hash is written out instead, because that is eighty lines of
//! arithmetic with published test vectors rather than a protocol.

use std::path::Path;
use std::process::{Command, Stdio};

/// Where releases are published.
const RELEASES: &str = "https://api.github.com/repos/oddurs/dirk/releases/latest";

/// The triple this binary was built for.
///
/// Worked out at compile time rather than from `uname`: the running binary
/// knows what it is, and asking the machine gets it wrong on exactly the
/// interesting cases -- an x86 build under Rosetta, a musl build on a glibc
/// host.
pub const fn target() -> &'static str {
    if cfg!(all(
        target_arch = "x86_64",
        target_os = "linux",
        target_env = "musl"
    )) {
        "x86_64-unknown-linux-musl"
    } else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
        "x86_64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
        "aarch64-unknown-linux-gnu"
    } else if cfg!(all(target_arch = "x86_64", target_os = "macos")) {
        "x86_64-apple-darwin"
    } else if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
        "aarch64-apple-darwin"
    } else {
        ""
    }
}

/// Who put this binary here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// dirk may replace it.
    Ours,
    Homebrew,
    Nix,
    Cargo,
}

impl Owner {
    /// What to tell somebody to run instead, when it is not ours to replace.
    pub fn instead(self) -> Option<&'static str> {
        match self {
            Owner::Ours => None,
            Owner::Homebrew => Some("brew upgrade dirk"),
            Owner::Nix => Some("nix profile upgrade dirk"),
            Owner::Cargo => Some("cargo install dirk --force"),
        }
    }
}

/// Work out who owns a path.
///
/// By where it sits, which is all there is to go on. A false "ours" breaks
/// somebody's package manager; a false "theirs" prints a command that does not
/// work. The second is the better failure, so anything ambiguous is theirs.
pub fn owner_of(path: &Path) -> Owner {
    let p = path.to_string_lossy();
    // `/nix/store` first: a nix profile can also be under a Cellar-shaped path
    // on a machine with both, and the store is the stronger claim.
    if p.starts_with("/nix/store/") || p.contains("/.nix-profile/") {
        return Owner::Nix;
    }
    if p.contains("/Cellar/") || p.contains("/homebrew/") || p.starts_with("/usr/local/Cellar") {
        return Owner::Homebrew;
    }
    if p.contains("/.cargo/bin/") || p.contains("/.rustup/") {
        return Owner::Cargo;
    }
    Owner::Ours
}

/// Compare two versions the way a release does.
///
/// Numeric field by field, so `0.10.0` is newer than `0.9.0` -- which string
/// comparison gets backwards, and which is the version this project reaches in
/// a few months.
///
/// Anything unparseable sorts as older than anything parseable, so a build
/// calling itself `0.5.0-dirty` is offered the release rather than told it is
/// ahead of it.
pub fn newer(candidate: &str, running: &str) -> bool {
    fields(candidate) > fields(running)
}

fn fields(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        // A pre-release or build suffix is not a number and not compared.
        .split(['-', '+'])
        .next()
        .unwrap_or("")
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// The tag of the most recent release, or nothing.
///
/// Nothing covers every way this can fail -- no curl, no network, a rate
/// limit, GitHub answering something this does not understand -- because the
/// caller does the same thing in all of them: says it could not ask.
pub fn latest() -> Option<String> {
    let out = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--max-time",
            "20",
            // GitHub refuses a request with no agent, and says so in JSON that
            // parses -- which would otherwise read as "no release".
            "--user-agent",
            concat!("dirk/", env!("CARGO_PKG_VERSION")),
            "--header",
            "accept: application/vnd.github+json",
            RELEASES,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let doc: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    let tag = doc.get("tag_name")?.as_str()?.trim().to_string();
    (!tag.is_empty()).then_some(tag)
}

/// SHA-256, written out.
///
/// A hash is arithmetic with published test vectors, not a protocol, and this
/// is the one place dirk needs one -- the same trade the base64 in
/// `clipboard.rs` makes. Shelling out would mean `sha256sum` on Linux and
/// `shasum -a 256` on macOS, which is a platform branch in the one function
/// that must not be wrong.
pub fn sha256(bytes: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    let mut msg = bytes.to_vec();
    let bits = (bytes.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_be_bytes());

    for block in msg.as_chunks::<64>().0 {
        let mut w = [0u32; 64];
        for (i, c) in block.as_chunks::<4>().0.iter().enumerate() {
            w[i] = u32::from_be_bytes(*c);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = hh
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (slot, v) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *slot = slot.wrapping_add(v);
        }
    }
    h.iter().map(|w| format!("{w:08x}")).collect()
}

/// Find one file's hash in a `SHA256SUMS`.
///
/// The name has to match exactly. A prefix match would accept
/// `dirk-0.9.0-x86_64-apple-darwin.tar.gz` for
/// `dirk-0.9.0-x86_64-apple-darwin.tar.gz.sig`, which is the sort of thing that
/// works until somebody adds a signature file.
pub fn digest_for(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let (hash, file) = line.split_once(char::is_whitespace)?;
        (file.trim().trim_start_matches('*') == name).then(|| hash.trim().to_lowercase())
    })
}

/// The asset this build wants, at a version.
pub fn asset_name(version: &str) -> String {
    format!(
        "dirk-{}-{}.tar.gz",
        version.trim_start_matches('v'),
        target()
    )
}

/// Fetch one release asset by URL.
fn fetch(url: &str) -> Option<Vec<u8>> {
    let out = Command::new("curl")
        .args([
            "--silent",
            "--show-error",
            "--location",
            "--fail",
            "--max-time",
            "120",
            "--user-agent",
            concat!("dirk/", env!("CARGO_PKG_VERSION")),
            url,
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Put the new binary where the old one is.
///
/// Written beside it and renamed over it, because a rename within a directory
/// is atomic and a copy is not: a copy interrupted halfway leaves a truncated
/// file where the program was, and the next thing anybody does is run it.
///
/// The mode is copied from the binary being replaced rather than assumed. An
/// installation that is deliberately group-executable and not world-executable
/// should stay that way.
fn replace(binary: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let dir = binary
        .parent()
        .ok_or_else(|| std::io::Error::other("the binary has no directory"))?;
    let temp = dir.join(".dirk.update");
    let mode = std::fs::metadata(binary).ok().map(|m| {
        use std::os::unix::fs::PermissionsExt;
        m.permissions().mode()
    });

    let written = (|| {
        let mut f = std::fs::File::create(&temp)?;
        f.write_all(bytes)?;
        f.sync_all()
    })();
    if let Err(e) = written {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp, std::fs::Permissions::from_mode(mode));
    }
    if let Err(e) = std::fs::rename(&temp, binary) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    Ok(())
}

/// Pull `dirk` out of the release tarball.
///
/// Through `tar`, which is on every machine this ships to, and into a
/// directory of our own that is removed afterwards -- so a tarball naming a
/// path outside itself has nowhere interesting to land.
fn unpack(tarball: &[u8], into: &Path) -> Option<Vec<u8>> {
    use std::io::Write;
    std::fs::create_dir_all(into).ok()?;
    let archive = into.join("dirk.tar.gz");
    std::fs::write(&archive, tarball).ok()?;
    let out = Command::new("tar")
        .arg("-xzf")
        .arg(&archive)
        .arg("-C")
        .arg(into)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .ok()?;
    if !out.success() {
        return None;
    }
    // One level down, in the directory the tarball is named after.
    let found = std::fs::read_dir(into)
        .ok()?
        .filter_map(|e| e.ok())
        .find_map(|e| {
            let candidate = e.path().join("dirk");
            candidate.is_file().then_some(candidate)
        })?;
    let bytes = std::fs::read(found).ok()?;
    let _ = std::io::stdout().flush();
    Some(bytes)
}

/// What `dirk update` did, or would do.
pub enum Outcome {
    UpToDate(String),
    Available { from: String, to: String },
    Updated { from: String, to: String },
    NotOurs(Owner),
    Unsupported,
    Failed(String),
}

/// The whole of it.
pub fn run(check_only: bool) -> Outcome {
    let running = env!("CARGO_PKG_VERSION");
    if target().is_empty() {
        return Outcome::Unsupported;
    }
    let Ok(binary) = std::env::current_exe() else {
        return Outcome::Failed("cannot tell where this binary is".into());
    };
    // Through any symlink, so `/usr/local/bin/dirk -> ../Cellar/...` is judged
    // by what it points at rather than by where it is pointed from.
    let binary = std::fs::canonicalize(&binary).unwrap_or(binary);
    let owner = owner_of(&binary);
    if owner != Owner::Ours {
        return Outcome::NotOurs(owner);
    }
    let Some(tag) = latest() else {
        return Outcome::Failed("cannot reach the releases; is there a network?".into());
    };
    let version = tag.trim_start_matches('v').to_string();
    if !newer(&version, running) {
        return Outcome::UpToDate(running.into());
    }
    if check_only {
        return Outcome::Available {
            from: running.into(),
            to: version,
        };
    }

    let base = format!("https://github.com/oddurs/dirk/releases/download/{tag}");
    let name = asset_name(&version);
    let Some(sums) = fetch(&format!("{base}/SHA256SUMS")) else {
        return Outcome::Failed("cannot fetch the checksums".into());
    };
    let Some(want) = digest_for(&String::from_utf8_lossy(&sums), &name) else {
        return Outcome::Failed(format!("the release has no {name}"));
    };
    let Some(tarball) = fetch(&format!("{base}/{name}")) else {
        return Outcome::Failed(format!("cannot fetch {name}"));
    };
    // Before it is unpacked, not after. An archive is a program's input and a
    // corrupt one is exactly what `tar` should not be handed.
    let got = sha256(&tarball);
    if got != want {
        return Outcome::Failed(format!("{name} does not match its checksum"));
    }

    let scratch = std::env::temp_dir().join(format!("dirk-update-{}", std::process::id()));
    let unpacked = unpack(&tarball, &scratch);
    let _ = std::fs::remove_dir_all(&scratch);
    let Some(bytes) = unpacked else {
        return Outcome::Failed("the release does not contain a dirk".into());
    };
    match replace(&binary, &bytes) {
        Ok(()) => Outcome::Updated {
            from: running.into(),
            to: version,
        },
        Err(e) => Outcome::Failed(format!("cannot replace {}: {e}", binary.display())),
    }
}

/// Print a line, surviving a closed pipe.
///
/// `dirk update --check | head -1` is a reasonable thing to type, and `print!`
/// panics when the reader has gone.
pub fn say(line: &str) {
    use std::io::Write;
    let _ = writeln!(std::io::stdout(), "{line}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ten_is_newer_than_nine() {
        // The thing string comparison gets backwards, and the version this
        // project reaches in a few months.
        assert!(newer("0.10.0", "0.9.0"));
        assert!(!newer("0.9.0", "0.10.0"));
        assert!(newer("1.0.0", "0.99.99"));
    }

    #[test]
    fn the_same_version_is_not_newer_than_itself() {
        // Or `dirk update` downloads and replaces the binary it is running,
        // every time anybody asks, for ever.
        assert!(!newer("0.5.0", "0.5.0"));
        assert!(!newer("v0.5.0", "0.5.0"), "a leading v is not a difference");
    }

    #[test]
    fn a_build_that_is_not_a_release_is_offered_the_release() {
        // `0.5.0-dirty` is behind `0.5.0` in the only sense that matters here:
        // it is not the published one. Telling somebody on a local build that
        // they are ahead of the release is how they stay on it.
        assert!(!newer("0.5.0-rc1", "0.5.0"));
        assert!(newer("0.6.0", "0.5.0-dirty"));
    }

    #[test]
    fn a_package_manager_owns_its_own_copy() {
        // Swapping one of these leaves a broken installation two weeks later,
        // when the package manager next has an opinion about a file that is no
        // longer the file it put there.
        use std::path::PathBuf;
        let cases = [
            ("/opt/homebrew/Cellar/dirk/0.5.0/bin/dirk", Owner::Homebrew),
            ("/usr/local/Cellar/dirk/0.5.0/bin/dirk", Owner::Homebrew),
            ("/nix/store/abc-dirk-0.5.0/bin/dirk", Owner::Nix),
            ("/home/x/.nix-profile/bin/dirk", Owner::Nix),
            ("/home/x/.cargo/bin/dirk", Owner::Cargo),
            ("/usr/local/bin/dirk", Owner::Ours),
            ("/home/x/bin/dirk", Owner::Ours),
        ];
        for (path, want) in cases {
            assert_eq!(owner_of(&PathBuf::from(path)), want, "{path}");
        }
    }

    #[test]
    fn every_owner_but_ours_has_a_command_to_offer_instead() {
        // Refusing without saying what to do instead is a dead end.
        for owner in [Owner::Homebrew, Owner::Nix, Owner::Cargo] {
            assert!(owner.instead().is_some(), "{owner:?} has no way forward");
        }
        assert!(Owner::Ours.instead().is_none());
    }

    #[test]
    fn sha256_matches_the_vectors_everybody_is_checked_against() {
        // The published ones. A hash that is subtly wrong verifies nothing and
        // says it verified something, which is worse than not checking.
        assert_eq!(
            sha256(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha256_spans_more_than_one_block() {
        // The padding and the length field are where a hand-written one goes
        // wrong, and neither is exercised by a short string.
        assert_eq!(
            sha256(&b"a".repeat(1_000_000)),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn replacing_keeps_the_mode_the_old_binary_had() {
        // An installation that is deliberately not world-executable should not
        // become world-executable because it was updated.
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("dirk-replace-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dir");
        let binary = dir.join("dirk");
        std::fs::write(&binary, b"old").expect("write");
        std::fs::set_permissions(&binary, std::fs::Permissions::from_mode(0o750)).expect("mode");

        replace(&binary, b"new").expect("replace");
        assert_eq!(std::fs::read(&binary).expect("read"), b"new");
        let mode = std::fs::metadata(&binary)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o750, "the mode changed under the update");
        // And nothing left beside it: a scratch file next to the binary is one
        // somebody finds a year later and wonders about.
        let strays: Vec<_> = std::fs::read_dir(&dir)
            .expect("dir")
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n != "dirk")
            .collect();
        assert!(strays.is_empty(), "left behind: {strays:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_digest_is_found_by_the_whole_name() {
        // A prefix match would take the tarball's hash for its signature file,
        // which works until somebody adds one.
        let sums = "\
aaaa  dirk-0.9.0-x86_64-apple-darwin.tar.gz
bbbb  dirk-0.9.0-x86_64-apple-darwin.tar.gz.sig
cccc *dirk-0.9.0-aarch64-apple-darwin.tar.gz
";
        assert_eq!(
            digest_for(sums, "dirk-0.9.0-x86_64-apple-darwin.tar.gz").as_deref(),
            Some("aaaa")
        );
        assert_eq!(
            digest_for(sums, "dirk-0.9.0-aarch64-apple-darwin.tar.gz").as_deref(),
            Some("cccc"),
            "the binary marker is not part of the name"
        );
        assert_eq!(digest_for(sums, "dirk-0.9.0-nothing.tar.gz"), None);
    }

    #[test]
    fn this_build_knows_which_asset_is_its_own() {
        // An empty triple means `update` refuses rather than downloading
        // somebody else's architecture and replacing itself with it.
        let name = asset_name("0.9.0");
        if target().is_empty() {
            return;
        }
        assert!(name.starts_with("dirk-0.9.0-"), "{name}");
        assert!(name.ends_with(".tar.gz"), "{name}");
        assert!(name.contains(target()), "{name} is not for {}", target());
        assert_eq!(asset_name("v0.9.0"), name, "a leading v is not part of it");
    }
}
