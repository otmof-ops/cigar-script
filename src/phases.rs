// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Where the time went, written in the family's timings format (#17,
//! otmof-ops/cig-script#179 step 4).
//!
//! When `CIG_TIMINGS_FD` names an open descriptor of 3 or more, each phase of
//! a check or a run is written to it as one JSON line when the phase ends:
//!
//! ```text
//! {"v":1,"product":"cigar","phase":"typeck","ms":0.412}
//! ```
//!
//! The phases are `lex` (reading the strength declaration), `gate` (the
//! feature gate), `lower` (CigarScript to CigScript), `typeck` (types, and the
//! strict checker's lints), `own` (the ownership rules, strength full only),
//! `cig` (cigscript's own parse and check of what will run) and `run` (the
//! program, from `cigar run`). They are flat: a module checked while the file
//! that `use`s it is being lowered is inside `lower`, not a phase of its own.
//! What is not written is the reader's `other`.
//!
//! With the variable unset, or naming a descriptor that is not open, nothing
//! here reads a clock or writes a byte: [`phase`] is one load and a call.
//! The variable is removed from the environment once read, so the program
//! `cigar run` starts does not write the same phases a second time.
//!
//! The descriptor is reached through `/dev/fd/N`, opened for appending, so no
//! `unsafe` and no C library is needed. Elsewhere than Unix nothing is written.

use std::cell::Cell;
use std::fs::File;
use std::io::Write;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The environment variable that names the descriptor.
pub const ENV: &str = "CIG_TIMINGS_FD";
/// The line format's version.
pub const VERSION: u32 = 1;
/// The name written in every line's `product` key.
pub const PRODUCT: &str = "cigar";

static SINK: OnceLock<Option<Mutex<File>>> = OnceLock::new();

thread_local! {
    /// How many phases are open on this thread: only the outermost is written.
    static DEPTH: Cell<u32> = const { Cell::new(0) };
}

/// Read `CIG_TIMINGS_FD` once, remove it from the environment and open the
/// sink. Call it first thing in `main`; without it every [`phase`] is a no-op.
pub fn init_from_env() {
    let raw = std::env::var(ENV).ok();
    std::env::remove_var(ENV);
    let _ = SINK.set(raw.and_then(|v| open(&v)).map(Mutex::new));
}

/// Is a sink open? (Tests and callers that skip work when nothing listens.)
pub fn active() -> bool {
    matches!(SINK.get(), Some(Some(_)))
}

#[cfg(unix)]
fn open(value: &str) -> Option<File> {
    let fd: i32 = value.trim().parse().ok()?;
    // 0 to 2 are the program's own; a stale variable must never break a run.
    if fd < 3 {
        return None;
    }
    std::fs::OpenOptions::new()
        .append(true)
        .open(format!("/dev/fd/{fd}"))
        .ok()
}

#[cfg(not(unix))]
fn open(_value: &str) -> Option<File> {
    None
}

/// One line of the format.
pub fn line(phase: &str, ms: f64) -> String {
    format!("{{\"v\":{VERSION},\"product\":\"{PRODUCT}\",\"phase\":\"{phase}\",\"ms\":{ms:.3}}}\n")
}

/// A phase that has begun. Ends with [`Open::end`]; when nothing listens, or
/// the phase is inside another, it holds nothing and ending it does nothing.
pub struct Open(Option<Instant>);

/// Begin a phase. For work whose result is large (a closure returning it
/// trips clippy's `result_large_err`), use this and [`Open::end`] around the
/// call instead of [`phase`].
pub fn begin() -> Open {
    if !matches!(SINK.get(), Some(Some(_))) || DEPTH.with(|d| d.get()) > 0 {
        return Open(None);
    }
    DEPTH.with(|d| d.set(1));
    Open(Some(Instant::now()))
}

impl Open {
    /// End the phase `name` and write its time.
    pub fn end(self, name: &'static str) {
        let Some(t) = self.0 else { return };
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        DEPTH.with(|d| d.set(0));
        if let Some(Some(sink)) = SINK.get() {
            if let Ok(mut file) = sink.lock() {
                // One write per line: far under PIPE_BUF, so lines never mix.
                let _ = file.write_all(line(name, ms).as_bytes());
            }
        }
    }
}

/// Run `f` as the phase `name` and, when a sink is open, write its time.
/// Phases do not nest: one opened inside another runs untimed.
pub fn phase<T>(name: &'static str, f: impl FnOnce() -> T) -> T {
    let open = begin();
    let r = f();
    open.end(name);
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_is_the_v1_format() {
        let l = line("lex", 0.5);
        assert_eq!(
            l,
            "{\"v\":1,\"product\":\"cigar\",\"phase\":\"lex\",\"ms\":0.500}\n"
        );
        let v: serde_json::Value = serde_json::from_str(l.trim()).unwrap();
        assert_eq!(v["v"], 1);
    }

    #[test]
    fn with_no_sink_a_phase_only_runs_its_work() {
        // `init_from_env` is never called by the unit tests.
        assert!(!active());
        assert_eq!(phase("lex", || 7), 7);
    }

    #[cfg(unix)]
    #[test]
    fn a_descriptor_below_three_or_unopen_is_ignored() {
        assert!(open("1").is_none());
        assert!(open("0").is_none());
        assert!(open("nine").is_none());
        assert!(open("987").is_none());
    }
}
