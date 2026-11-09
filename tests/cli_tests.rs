// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! CLI smoke tests: exit codes and a `--json` shape check.

use std::process::Command;

fn cigar_bin() -> std::path::PathBuf {
    // $CARGO_BIN_EXE_<name> is set by cargo for integration tests.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_cigar"))
}

fn fixtures() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

#[test]
fn ok_exits_zero() {
    let tmp = tempfile::tempdir().unwrap();
    let f = tmp.path().join("hello.cig");
    std::fs::write(&f, "exhale \"hi\"\n").unwrap();
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}

#[test]
fn a_failing_file_exits_two() {
    let f = fixtures().join("mild/record.cigar");
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn usage_exits_three() {
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn unknown_strength_exits_three() {
    let f = fixtures().join("mild/record.cigar");
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color", "--strength", "spicy"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
}

#[test]
fn json_output_has_the_cig_shape() {
    let f = fixtures().join("mild/record.cigar");
    let out = Command::new(cigar_bin())
        .args(["check", "--json", "--no-color"])
        .arg(&f)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    // The diagnostic is written to stdout in JSON mode (as cig's
    // `report` does). Parse the first non-empty line as JSON and check
    // the fields we rely on.
    let stdout = String::from_utf8_lossy(&out.stdout);
    let first = stdout
        .lines()
        .find(|l| !l.trim().is_empty())
        .expect("one JSON object on stdout");
    let v: serde_json::Value = serde_json::from_str(first).expect("stdout is JSON");
    assert_eq!(v["code"], "E1101");
    assert_eq!(v["kind"], "check");
    assert!(v["message"].is_string());
    assert!(v["file"].is_string());
    assert!(v["line"].is_number());
    assert!(v["col"].is_number());
}

#[test]
fn a_folder_checks_every_top_level_file() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("ok.cig"), "exhale \"hi\"\n").unwrap();
    std::fs::write(
        tmp.path().join("bad.cigar"),
        "record Move { from: Path, to: Path }\n",
    )
    .unwrap();
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color"])
        .arg(tmp.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

/// A diagnostic shows its line as written: the `strength` declaration is
/// blanked for the checker, never in what the reader sees.
#[test]
fn a_declaration_error_shows_the_line_as_written() {
    let f = fixtures().join("declaration/spicy.cigar");
    let out = Command::new(cigar_bin())
        .args(["check", "--no-color"])
        .arg(&f)
        .output()
        .unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("1 | strength spicy"), "{err}");
}

/// Usage errors carry cig's own codes, so `cig explain` says what they mean:
/// E800 for a command line that names nothing to check, E801 for a path
/// that cannot be read.
#[test]
fn usage_errors_carry_cigs_codes() {
    let tmp = tempfile::tempdir().unwrap();
    let txt = tmp.path().join("a.txt");
    std::fs::write(&txt, "x\n").unwrap();
    let missing = tmp.path().join("nope.cigar");
    for (path, code) in [(&txt, "E800"), (&missing, "E801")] {
        let out = Command::new(cigar_bin())
            .args(["--json", "check"])
            .arg(path)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(3), "{}", path.display());
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        assert!(said.contains(&format!("\"code\":\"{code}\"")), "{said}");
    }
}
