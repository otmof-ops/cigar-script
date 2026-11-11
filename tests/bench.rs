// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The benchmark manifest's promises (#17, otmof-ops/cig-script#179): the
//! generated programs are the same on every machine, the superset programs
//! print what CigScript prints, and a check or a run writes its phases to
//! `CIG_TIMINGS_FD` in the family's v1 line format.
//!
//! Every child process runs in a scratch `HOME` under a hard timeout.

mod common;

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// Run `cmd` for at most `secs`, capturing its output; a child that outlives
/// the limit is killed and the test fails.
fn run_for(mut cmd: Command, secs: u64) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("cannot start the command");
    let mut out = child.stdout.take().unwrap();
    let mut err = child.stderr.take().unwrap();
    let t1 = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut v);
        v
    });
    let t2 = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err, &mut v);
        v
    });
    let status = match child.wait_timeout(Duration::from_secs(secs)).unwrap() {
        Some(s) => s,
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the command ran over {secs} s and was killed");
        }
    };
    Output {
        status,
        stdout: t1.join().unwrap(),
        stderr: t2.join().unwrap(),
    }
}

/// A command for `cigar` that touches nothing of the developer's.
fn cigar(home: &Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_cigar"));
    scratch(&mut c, home);
    c
}

fn scratch(c: &mut Command, home: &Path) {
    c.env("HOME", home)
        .env("USERPROFILE", home)
        .env("CIGSCRIPT_HOME", home.join("cigscript"))
        .env("XDG_CONFIG_HOME", home.join("xdg-config"))
        .env("XDG_DATA_HOME", home.join("xdg-data"))
        .env("XDG_CACHE_HOME", home.join("xdg-cache"))
        .env("XDG_STATE_HOME", home.join("xdg-state"))
        .env("NO_COLOR", "1")
        .env("CIG_DOCTOR", "0")
        .env_remove("CIG_TIMINGS_FD")
        .env_remove("CIG_STATS_FD");
}

/// `python3`, or a skip with a note on a machine without it. CI has it, so
/// there a missing one fails: a skipped guard is not a guard.
fn python() -> Option<&'static str> {
    let found = Command::new("python3")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if found {
        return Some("python3");
    }
    assert!(
        std::env::var_os("CI").is_none(),
        "python3 is needed for the generated fixtures and is not installed"
    );
    eprintln!("note: skipping; python3 is not installed");
    None
}

// ----- generated_fixtures_are_reproducible --------------------------------------

#[test]
fn generated_fixtures_are_reproducible() {
    let Some(py) = python() else { return };
    let script = root().join("bench/fixtures/generate.py");
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();

    // The first run is held to the committed sha256 of every file.
    let mut c1 = Command::new(py);
    c1.arg(&script).arg("--check").arg(a.path());
    let first = run_for(c1, 120);
    assert!(
        first.status.success(),
        "the generated files differ from bench/fixtures/SHA256SUMS: {}",
        String::from_utf8_lossy(&first.stderr)
    );
    // A second run in another folder, from another working directory, writes the
    // same bytes: nothing depends on where or when it runs.
    let mut c2 = Command::new(py);
    c2.arg(&script).arg(b.path()).current_dir(home.path());
    let second = run_for(c2, 120);
    assert!(second.status.success());
    assert_eq!(
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&second.stdout),
        "two runs listed different sums"
    );

    let sums = std::fs::read_to_string(root().join("bench/fixtures/SHA256SUMS")).unwrap();
    assert_eq!(String::from_utf8_lossy(&second.stdout), sums);
    let mut names = 0;
    for line in sums.lines() {
        let (_, name) = line.split_once("  ").expect("a sum, two spaces, a name");
        let x = std::fs::read(a.path().join(name)).unwrap();
        let y = std::fs::read(b.path().join(name)).unwrap();
        assert_eq!(x, y, "{name} differs between runs");
        assert!(!x.contains(&b'\r'), "{name} has a CR");
        // `size-<N>-...` has exactly N lines, declaration included.
        if let Some(n) = name
            .strip_prefix("size-")
            .and_then(|r| r.split('-').next())
            .and_then(|n| n.parse::<usize>().ok())
        {
            assert_eq!(x.iter().filter(|c| **c == b'\n').count(), n, "{name}");
        }
        names += 1;
    }
    // 4 sizes x 3 strengths, and 2 sizes x 2 strengths x {generics, match}.
    assert_eq!(names, 12 + 8);

    // Every generated file is named by the manifest, so none is left unmeasured.
    let manifest = std::fs::read_to_string(root().join("bench/bench.toml")).unwrap();
    for line in sums.lines() {
        let name = line.split_once("  ").unwrap().1;
        assert!(
            manifest.contains(&format!("generated/{name}")),
            "{name} is not in bench/bench.toml"
        );
    }
}

#[test]
fn generated_programs_check_clean_at_their_strength() {
    let Some(py) = python() else { return };
    let dir = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut generator = Command::new(py);
    generator
        .arg(root().join("bench/fixtures/generate.py"))
        .arg(dir.path());
    assert!(run_for(generator, 120).status.success());
    // The smallest of each family, at every strength it comes in: a generator
    // that made a program the checker refuses would measure the error path.
    for name in [
        "size-100-mild",
        "size-100-medium",
        "size-100-full",
        "size-1000-full",
        "generics-1000-medium",
        "generics-1000-full",
        "match-1000-medium",
        "match-1000-full",
    ] {
        let mut c = cigar(home.path());
        c.arg("check").arg(dir.path().join(format!("{name}.cigar")));
        let out = run_for(c, 120);
        assert!(
            out.status.success() && out.stderr.is_empty(),
            "{name}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

// ----- superset_programs_same_output --------------------------------------------

/// The programs of cig's shared compute set the manifest runs, as `.cig` and
/// as mild `.cigar`.
const SUPERSET: &[&str] = &[
    "closures",
    "csv",
    "deltablue",
    "dispatch",
    "fannkuch",
    "fib",
    "json-deep",
    "json",
    "lists",
    "loop",
    "mandel",
    "maps",
    "nbody",
    "regex-ish",
    "richards",
    "sieve",
    "spectral",
    "strbuild",
    "strings",
    "trees",
    "words",
];

#[test]
fn superset_programs_same_output() {
    // The manifest and this list say the same twenty-one programs.
    let manifest = std::fs::read_to_string(root().join("bench/bench.toml")).unwrap();
    for name in SUPERSET {
        assert!(
            manifest.contains(&format!("generated/superset/{name}.cigar"))
                && manifest.contains(&format!("generated/superset/{name}.cig\"")),
            "{name} is not run both ways in bench/bench.toml"
        );
    }
    let Some(src) = common::cigscript_src() else {
        eprintln!("note: skipping; CIGSCRIPT_SRC does not point at a CigScript checkout");
        return;
    };
    let set = src.join("bench/compute");
    if !set.is_dir() {
        eprintln!("note: skipping; {} has no bench/compute", set.display());
        return;
    }
    let work = tempfile::tempdir().unwrap();
    let home = tempfile::tempdir().unwrap();
    let mut wrong = Vec::new();
    for name in SUPERSET {
        let cig_file = set.join(format!("{name}.cig"));
        let expect = std::fs::read(set.join(format!("{name}.expect")))
            .unwrap_or_else(|e| panic!("{name}.expect: {e}"));
        // The same bytes, named `.cigar`, no strength line: mild.
        let as_cigar = work.path().join(format!("{name}.cigar"));
        std::fs::copy(&cig_file, &as_cigar).unwrap();
        let mut c = cigar(home.path());
        c.arg("run").arg(&as_cigar).current_dir(work.path());
        let out = run_for(c, 300);
        if out.status.code() != Some(0) || out.stdout != expect {
            wrong.push(format!(
                "{name}: exit {:?}; stdout {} bytes, expected {}; stderr: {}",
                out.status.code(),
                out.stdout.len(),
                expect.len(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

// ----- strength_phases_reported -------------------------------------------------

/// Run `cigar ARGS FILE` with `CIG_TIMINGS_FD=9` and descriptor 9 open on a
/// file, as the harness hands it over, and return the lines that were written.
#[cfg(unix)]
fn phases(home: &Path, args: &[&str], file: &Path, fd_var: Option<&str>) -> (Output, String) {
    let out_file = home.join("timings.out");
    std::fs::write(&out_file, "").unwrap();
    let mut c = Command::new("sh");
    c.arg("-c")
        .arg("exec \"$0\" \"$@\" 9>\"$CIGAR_TEST_OUT\"")
        .arg(env!("CARGO_BIN_EXE_cigar"))
        .args(args)
        .arg(file);
    scratch(&mut c, home);
    c.env("CIGAR_TEST_OUT", &out_file);
    if let Some(v) = fd_var {
        c.env("CIG_TIMINGS_FD", v);
    }
    let out = run_for(c, 180);
    let written = std::fs::read_to_string(&out_file).unwrap();
    (out, written)
}

/// Parse the v1 lines: (phase, ms), each line held to the format.
#[cfg(unix)]
fn parse(lines: &str) -> Vec<(String, f64)> {
    lines
        .lines()
        .map(|l| {
            let v: serde_json::Value =
                serde_json::from_str(l).unwrap_or_else(|e| panic!("not JSON ({e}): {l}"));
            assert_eq!(v["v"], 1, "{l}");
            assert_eq!(v["product"], "cigar", "{l}");
            let phase = v["phase"].as_str().expect("phase").to_string();
            assert!(
                !phase.is_empty()
                    && phase
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{l}"
            );
            let ms = v["ms"].as_f64().expect("ms is a number");
            assert!(ms.is_finite() && ms >= 0.0, "{l}");
            (phase, ms)
        })
        .collect()
}

#[cfg(unix)]
#[test]
fn strength_phases_reported() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let sources = [
        ("mild", "strength mild\nstick a = 1\nexhale a\n"),
        ("medium", "strength medium\nstick a: Int = 1\nexhale a\n"),
        ("full", "strength full\nstick a: Int = 1\nexhale a\n"),
    ];
    for (strength, text) in sources {
        let file = dir.path().join(format!("{strength}.cigar"));
        std::fs::write(&file, text).unwrap();
        let (out, written) = phases(home.path(), &["check"], &file, Some("9"));
        assert_eq!(
            out.status.code(),
            Some(0),
            "{strength}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let got = parse(&written);
        let names: Vec<&str> = got.iter().map(|(p, _)| p.as_str()).collect();
        for want in ["lex", "gate", "cig"] {
            assert!(
                names.contains(&want),
                "{strength}: no `{want}` phase in {names:?}"
            );
        }
        // The deeper strengths are lowered and typed; only full has ownership.
        assert_eq!(
            names.contains(&"lower"),
            strength != "mild",
            "{strength}: {names:?}"
        );
        assert_eq!(
            names.contains(&"typeck"),
            strength != "mild",
            "{strength}: {names:?}"
        );
        assert_eq!(
            names.contains(&"own"),
            strength == "full",
            "{strength}: {names:?}"
        );
        // The run is not a check: no `run` phase.
        assert!(!names.contains(&"run"), "{strength}: {names:?}");
    }

    // `cigar run` adds the program's own time as `run`, once, and nothing the
    // runtime underneath writes: the variable is not passed on.
    let file = dir.path().join("run.cigar");
    std::fs::write(&file, "strength medium\nstick a: Int = 1\nexhale a\n").unwrap();
    let (out, written) = phases(home.path(), &["run"], &file, Some("9"));
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "1");
    let got = parse(&written);
    assert_eq!(
        got.iter().filter(|(p, _)| p == "run").count(),
        1,
        "{written}"
    );
    assert_eq!(
        got.last().map(|(p, _)| p.as_str()),
        Some("run"),
        "{written}"
    );
}

#[cfg(unix)]
#[test]
fn phases_cost_nothing_when_not_asked_for() {
    let home = tempfile::tempdir().unwrap();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.cigar");
    std::fs::write(&file, "strength medium\nstick a: Int = 1\nexhale a\n").unwrap();
    // Descriptor 9 is open on a file, but the variable is not set: no bytes.
    let (out, written) = phases(home.path(), &["check"], &file, None);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(written, "");
    // A stale variable: a descriptor that is not open, and one that is the
    // program's own, are ignored; the run is as if nothing was said.
    for stale in ["77", "1", "2", "x"] {
        let (out, written) = phases(home.path(), &["check"], &file, Some(stale));
        assert_eq!(out.status.code(), Some(0), "{stale}");
        assert!(
            !String::from_utf8_lossy(&out.stdout).contains("\"v\":1"),
            "{stale}"
        );
        assert!(
            !String::from_utf8_lossy(&out.stderr).contains("\"v\":1"),
            "{stale}"
        );
        assert_eq!(written, "", "{stale}");
    }
}
