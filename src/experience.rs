// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The experience warning for `strength full` (#7): asked once per project,
//! remembered, skippable.
//!
//! What is remembered lives in the person's state folder, keyed by the
//! project's root, and never in the project: a repository carries nothing
//! about who has read the warning. The opt-out is a line in the person's
//! own config, `strength_full_warning = off`.
//!
//! * State: `$XDG_STATE_HOME/cigar/full-warning` (else
//!   `~/.local/state/cigar/full-warning`; `%LOCALAPPDATA%\cigar\` on
//!   Windows), one project root per line.
//! * Config: `$XDG_CONFIG_HOME/cigar/config` (else `~/.config/cigar/config`;
//!   `%APPDATA%\cigar\` on Windows), lines of `key = value`.
//!
//! A run that is not interactive never asks: it says so once per project, and goes on.
//! Nothing here stands in for anything else: the opt-out turns this one
//! notice off and no more.

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

/// What the notice came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Go on: the person has said yes (now or before), or has turned the
    /// notice off.
    Proceed,
    /// Go on, though nobody was asked: the run is not interactive.
    NotAsked,
    /// The person said no. Nothing is remembered; the next run asks again.
    Declined,
}

/// Where the person's state and config live.
#[derive(Clone, Debug)]
pub struct Dirs {
    pub state: PathBuf,
    pub config: PathBuf,
}

fn env_dir(var: &str) -> Option<PathBuf> {
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

impl Dirs {
    /// The folders for this person, from the environment.
    pub fn from_env() -> Dirs {
        let home = env_dir("HOME").or_else(|| env_dir("USERPROFILE"));
        let state = env_dir("XDG_STATE_HOME")
            .or_else(|| {
                if cfg!(windows) {
                    env_dir("LOCALAPPDATA")
                } else {
                    home.as_ref().map(|h| h.join(".local").join("state"))
                }
            })
            .unwrap_or_else(std::env::temp_dir);
        let config = env_dir("XDG_CONFIG_HOME")
            .or_else(|| {
                if cfg!(windows) {
                    env_dir("APPDATA")
                } else {
                    home.as_ref().map(|h| h.join(".config"))
                }
            })
            .unwrap_or_else(std::env::temp_dir);
        Dirs {
            state: state.join("cigar"),
            config: config.join("cigar"),
        }
    }

    fn remembered_file(&self) -> PathBuf {
        self.state.join("full-warning")
    }

    /// The projects a run that could not ask has already said so for.
    fn noted_file(&self) -> PathBuf {
        self.state.join("full-warning-noted")
    }

    fn config_file(&self) -> PathBuf {
        self.config.join("config")
    }
}

/// The text of the warning.
pub const WARNING: &str = "\
strength full holds a script to Rust's rules: every list, map, record and
channel has one owner, giving a name away is a move, and a borrow that
outlives what it borrows is an error. It is written for someone who has
worked with an ownership checker before. `strength medium` has the types
and the modules without those rules, and changing a file between them
never breaks one that already checks.";

/// Has the person turned the notice off (`strength_full_warning = off`)?
pub fn turned_off(dirs: &Dirs) -> bool {
    let Ok(text) = fs::read_to_string(dirs.config_file()) else {
        return false;
    };
    let mut off = false;
    for line in text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if k.trim() == "strength_full_warning" {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            off = v.eq_ignore_ascii_case("off")
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("no");
        }
    }
    off
}

/// The project a file belongs to: the nearest folder above it (or its own)
/// that holds `cigar.toml`, `.cig-version` or `.git`; else the file's folder.
pub fn project_root(file: &Path) -> PathBuf {
    let abs = fs::canonicalize(file).unwrap_or_else(|_| file.to_path_buf());
    let start = abs.parent().map(Path::to_path_buf).unwrap_or(abs.clone());
    for dir in start.ancestors() {
        if [".git", "cigar.toml", ".cig-version"]
            .iter()
            .any(|m| dir.join(m).exists())
        {
            return dir.to_path_buf();
        }
    }
    start
}

fn key_of(root: &Path) -> Option<String> {
    let s = root.to_string_lossy().to_string();
    // One root per line: a name with a line break in it is never remembered.
    (!s.contains('\n') && !s.contains('\r')).then_some(s)
}

/// Has this project's warning been read and accepted?
pub fn remembered(dirs: &Dirs, root: &Path) -> bool {
    listed(&dirs.remembered_file(), root)
}

fn remember(dirs: &Dirs, root: &Path) -> io::Result<()> {
    list(dirs, &dirs.remembered_file(), root)
}

fn listed(file: &Path, root: &Path) -> bool {
    let Some(key) = key_of(root) else {
        return false;
    };
    fs::read_to_string(file)
        .map(|t| t.lines().any(|l| l == key))
        .unwrap_or(false)
}

fn list(dirs: &Dirs, file: &Path, root: &Path) -> io::Result<()> {
    let Some(key) = key_of(root) else {
        return Ok(());
    };
    fs::create_dir_all(&dirs.state)?;
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(file)?;
    writeln!(f, "{key}")
}

/// Is this run one a person can answer: a terminal on stdin and on stderr?
pub fn interactive() -> bool {
    io::stdin().is_terminal() && io::stderr().is_terminal()
}

/// The notice for a project at `strength full`.
///
/// `interactive` says whether anyone can answer; `input` is where the
/// answer is read and `out` where the question is written (stderr, in the
/// command line). When the person has turned the notice off, or has said
/// yes before for this project, nothing is written at all.
pub fn confirm_full(
    dirs: &Dirs,
    root: &Path,
    interactive: bool,
    input: &mut dyn BufRead,
    out: &mut dyn Write,
) -> Outcome {
    if turned_off(dirs) || remembered(dirs, root) {
        return Outcome::Proceed;
    }
    if !interactive {
        // Said once per project: a CI job at full is not told on every run.
        if listed(&dirs.noted_file(), root) {
            return Outcome::NotAsked;
        }
        let _ = list(dirs, &dirs.noted_file(), root);
        let _ = writeln!(
            out,
            "note: this project uses strength full, and this run is not interactive, so it is not asking about it; \
the next run in a terminal will (strength_full_warning = off in {} turns it off)",
            dirs.config_file().display()
        );
        return Outcome::NotAsked;
    }
    let _ = writeln!(out, "{WARNING}");
    let _ = write!(out, "Use strength full in this project? [y/N] ");
    let _ = out.flush();
    let mut answer = String::new();
    let read = input.read_line(&mut answer).unwrap_or(0);
    let yes = read > 0 && matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes");
    if !yes {
        let _ = writeln!(
            out,
            "not run: nothing is remembered, and the next run asks again"
        );
        return Outcome::Declined;
    }
    if let Err(e) = remember(dirs, root) {
        let _ = writeln!(
            out,
            "note: could not remember the answer in {}: {e}",
            dirs.state.display()
        );
    }
    Outcome::Proceed
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn dirs(t: &Path) -> Dirs {
        Dirs {
            state: t.join("state").join("cigar"),
            config: t.join("config").join("cigar"),
        }
    }

    fn ask(d: &Dirs, root: &Path, tty: bool, said: &str) -> (Outcome, String) {
        let mut out = Vec::new();
        let o = confirm_full(d, root, tty, &mut Cursor::new(said.to_string()), &mut out);
        (o, String::from_utf8(out).unwrap())
    }

    #[test]
    fn asked_once_per_project_and_remembered() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(t.path());
        let (a, b) = (t.path().join("a"), t.path().join("b"));
        let (o, said) = ask(&d, &a, true, "y\n");
        assert_eq!(o, Outcome::Proceed);
        assert!(said.contains("Rust's rules"));
        // The same project is not asked again, and nothing is written.
        let (o, said) = ask(&d, &a, true, "");
        assert_eq!((o, said.as_str()), (Outcome::Proceed, ""));
        // Another project is.
        let (o, said) = ask(&d, &b, true, "yes\n");
        assert_eq!(o, Outcome::Proceed);
        assert!(said.contains("[y/N]"));
    }

    #[test]
    fn a_no_is_not_remembered() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(t.path());
        let root = t.path().join("p");
        assert_eq!(ask(&d, &root, true, "n\n").0, Outcome::Declined);
        assert_eq!(ask(&d, &root, true, "").0, Outcome::Declined);
        assert!(!remembered(&d, &root));
        assert_eq!(ask(&d, &root, true, "y\n").0, Outcome::Proceed);
    }

    #[test]
    fn a_run_nobody_can_answer_never_asks_and_says_so() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(t.path());
        let root = t.path().join("p");
        let (o, said) = ask(&d, &root, false, "y\n");
        assert_eq!(o, Outcome::NotAsked);
        assert!(said.contains("not interactive"));
        assert_eq!(said.matches("note:").count(), 1);
        // Nothing was accepted: the next run in a terminal asks.
        assert!(!remembered(&d, &root));
        // A second run that cannot ask says nothing more.
        let (o, said) = ask(&d, &root, false, "");
        assert_eq!(o, Outcome::NotAsked);
        assert!(said.is_empty(), "{said}");
        // A run in a terminal still asks.
        let (_, said) = ask(&d, &root, true, "y\n");
        assert!(said.contains("Rust's rules"), "{said}");
    }

    #[test]
    fn the_config_line_turns_it_off() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(t.path());
        fs::create_dir_all(&d.config).unwrap();
        fs::write(d.config_file(), "# mine\nstrength_full_warning = off\n").unwrap();
        let (o, said) = ask(&d, &t.path().join("p"), false, "");
        assert_eq!((o, said.as_str()), (Outcome::Proceed, ""));
        fs::write(d.config_file(), "strength_full_warning = on\n").unwrap();
        assert!(!turned_off(&d));
    }

    #[test]
    fn the_memory_stays_out_of_the_project() {
        let t = tempfile::tempdir().unwrap();
        let d = dirs(t.path());
        let project = t.path().join("proj");
        fs::create_dir_all(project.join(".git")).unwrap();
        fs::write(project.join("a.cigar"), "strength full\n").unwrap();
        let root = project_root(&project.join("a.cigar"));
        assert_eq!(root, fs::canonicalize(&project).unwrap());
        assert_eq!(ask(&d, &root, true, "y\n").0, Outcome::Proceed);
        let mut inside: Vec<String> = fs::read_dir(&project)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        inside.sort();
        assert_eq!(inside, [".git", "a.cigar"]);
        assert!(d.state.join("full-warning").is_file());
    }
}
