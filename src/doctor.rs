// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar __doctor`: the checks only cigar can make, for `ciginstaller
//! doctor --tools` to show beside its own (the installer owns PATH,
//! packages and releases). It is hidden, it reads and never fixes, and it
//! never starts `ciginstaller`; the one program it starts is `cig
//! --version`, which does not start cigar. It always exits 0: the findings
//! are the answer, not the exit code.
//!
//! `--json` writes the installer doctor's own shape:
//! `{"tool":"cigar","version":"...","findings":[{"check","severity","found",
//! "because","fix"}]}`, where severity is `ok`, `note` or `action`.

use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

use crate::check::{check_source, Kind, ISSUES_URL};

/// A medium file that exercises lowering: a type, an enum, a function and a
/// `match`. It checks clean in every cigar that works.
const SAMPLE: &str = "\
strength medium

record Move { from: Path, to: Path }
enum Status { ok, failed(reason: Text), skipped }

pull label(s: Status) -> Text {
  match s {
    failed(reason) if reason != \"\" => snuff \"failed: ${reason}\"
    failed(_) => snuff \"failed\"
    ok => snuff \"ok\"
    skipped => snuff \"skipped\"
  }
}

stick m = Move { from: \"a.txt\", to: \"b/a.txt\" }
exhale m, label(Status.ok)
";

fn finding(check: &str, severity: &str, found: String, because: &str, fix: Option<&str>) -> Value {
    json!({
        "check": check,
        "severity": severity,
        "found": found,
        "because": because,
        "fix": fix,
    })
}

/// Every finding, in order. Nothing here writes, fixes or recurses.
pub fn findings() -> Vec<Value> {
    vec![lowering(), embedded_cig(), cache(&crate::run::cache_dir())]
}

/// The whole `--json` document.
pub fn report() -> Value {
    json!({
        "tool": "cigar",
        "version": env!("CARGO_PKG_VERSION"),
        "findings": findings(),
    })
}

/// Lowering a known sample and handing it to cigscript gives no error, and
/// in particular no `E1104` (cig refusing what cigar lowered).
fn lowering() -> Value {
    let checked = check_source(Kind::Cigar, SAMPLE, None);
    let because = "every medium and full file is lowered to CigScript before it is checked or run";
    if let Some(d) = checked.errors.iter().find(|d| d.code == "E1104") {
        return finding(
            "lowering",
            "action",
            format!("the known sample gives E1104: {}", d.message),
            because,
            Some(&format!("report it at {ISSUES_URL}")),
        );
    }
    if let Some(d) = checked.errors.first() {
        return finding(
            "lowering",
            "action",
            format!("the known sample gives {}: {}", d.code, d.message),
            because,
            Some(&format!("report it at {ISSUES_URL}")),
        );
    }
    finding(
        "lowering",
        "ok",
        "a known sample lowers and checks with no E1104".to_string(),
        because,
        None,
    )
}

/// The cigscript this cigar carries, against the `cig` found on this
/// machine, if there is one. `cigar run` does not need `cig`: it runs on
/// the embedded runtime. A different `cig` only means `cig check` can judge
/// a file differently from `cigar check`.
fn embedded_cig() -> Value {
    let mine = cigscript::VERSION;
    let because = "a mild file is meant to check exactly as `cig check` checks it";
    let found = cig_version();
    let Some(theirs) = found else {
        return finding(
            "embedded-cig",
            "note",
            format!("cigar carries CigScript {mine}; no cig was found to compare, and cigar run does not need one"),
            because,
            None,
        );
    };
    if theirs == mine {
        return finding(
            "embedded-cig",
            "ok",
            format!("cigar carries CigScript {mine}, the same as the cig installed"),
            because,
            None,
        );
    }
    // Nothing breaks when they differ: cigar runs on what it carries. It is
    // a note, with the installer's own check named for the pairing.
    finding(
        "embedded-cig",
        "note",
        format!("cigar carries CigScript {mine}; the cig installed is {theirs}"),
        because,
        Some("`ciginstaller check` says which carton holds a cigar and a cig that fit"),
    )
}

/// `cig --version` is `cig X.Y.Z`; `None` when there is no cig to run or it
/// said something else.
fn cig_version() -> Option<String> {
    let out = Command::new("cig")
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let said = String::from_utf8_lossy(&out.stdout);
    said.split_whitespace()
        .find(|w| w.chars().next().is_some_and(|c| c.is_ascii_digit()) && w.contains('.'))
        .map(|w| {
            w.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '.'))
                .to_string()
        })
}

/// The run cache: is it a folder cigar can write to, and how big is it. A
/// cache not made yet is fine; `cigar run` makes it. The only write is a
/// probe file, named for this process and removed at once.
fn cache(dir: &Path) -> Value {
    let because = "cigar run keeps the programs it builds there";
    let shown = dir.display();
    match fs::symlink_metadata(dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => finding(
            "run-cache",
            "ok",
            format!("{shown} is not made yet; the first cigar run makes it"),
            because,
            None,
        ),
        Err(e) => finding(
            "run-cache",
            "action",
            format!("{shown} cannot be read: {e}"),
            because,
            Some("check the folder's owner and mode"),
        ),
        Ok(m) if !m.is_dir() => finding(
            "run-cache",
            "action",
            format!("{shown} is not a folder"),
            because,
            Some("move it aside; cigar run makes the folder again"),
        ),
        Ok(_) => {
            let probe = dir.join(format!(".doctor-probe-{}", std::process::id()));
            let wrote = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&probe)
                .map(|_| ())
                .and_then(|_| fs::remove_file(&probe));
            match wrote {
                Ok(()) => finding(
                    "run-cache",
                    "ok",
                    format!("{shown} is writable, {}", human_bytes(dir_size(dir))),
                    because,
                    None,
                ),
                Err(e) => finding(
                    "run-cache",
                    "action",
                    format!("{shown} cannot be written: {e}"),
                    because,
                    Some("check the folder's owner and mode"),
                ),
            }
        }
    }
}

/// The cache is one flat folder of built programs.
fn dir_size(dir: &Path) -> u64 {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| e.metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum()
}

fn human_bytes(n: u64) -> String {
    const K: u64 = 1 << 10;
    const M: u64 = 1 << 20;
    const G: u64 = 1 << 30;
    if n >= G {
        format!("{} GB", n / G)
    } else if n >= M {
        format!("{} MB", n / M)
    } else if n >= K {
        format!("{} KB", n / K)
    } else {
        format!("{n} B")
    }
}

/// `cigar __doctor [--json]`. Always 0.
pub fn run(json_out: bool) -> i32 {
    let doc = report();
    if json_out {
        println!("{doc}");
        return 0;
    }
    println!("cigar {}", env!("CARGO_PKG_VERSION"));
    for f in doc["findings"].as_array().into_iter().flatten() {
        let sev = f["severity"].as_str().unwrap_or("");
        println!(
            "{:<6} {}: {}",
            sev,
            f["check"].as_str().unwrap_or(""),
            f["found"].as_str().unwrap_or("")
        );
        if let Some(fix) = f["fix"].as_str() {
            println!("       fix: {fix}");
        }
    }
    0
}
