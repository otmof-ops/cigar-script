// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Strengths mix across files (#7, DECISIONS 20): a mild file may `use` a
//! medium module, the module is checked at its own strength, and the values
//! the mild file passes are checked when it runs (E1210). The programs
//! themselves are in tests/corpus (run/ and runtime/); these tests hold the
//! rest: what the check says, and the experience warning for `full`.

use std::path::{Path, PathBuf};
use std::process::Command;

use cigarscript::check::{check_file_source, Kind};

fn case(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(name)
        .join("main.cigar")
}

fn check(path: &Path) -> cigarscript::check::Checked {
    let src = std::fs::read_to_string(path).unwrap();
    check_file_source(Kind::Cigar, &src, None, Some(path))
}

/// `cigar args` in a scratch home, under a hard timeout, stdin closed.
fn cigar(home: &Path, dir: &Path, args: &[&str]) -> (i32, String, String) {
    let out = Command::new("timeout")
        .arg("120")
        .arg(env!("CARGO_BIN_EXE_cigar"))
        .args(args)
        .current_dir(dir)
        .stdin(std::process::Stdio::null())
        .env("HOME", home)
        .env("USERPROFILE", home)
        .env("CIGSCRIPT_HOME", home.join("cig-home"))
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("NO_COLOR", "1")
        .env("CIG_DOCTOR", "0")
        .output()
        .expect("cigar runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn a_mild_file_may_use_a_medium_module() {
    let c = check(&case("run/mild-uses-medium-module"));
    assert!(c.ok(), "{:?}", c.errors);
    assert_eq!(c.strength, cigarscript::features::Strength::Mild);
}

#[test]
fn the_module_is_still_checked_at_its_own_strength() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("tools")).unwrap();
    // A module that is wrong inside: its error is its own.
    std::fs::write(
        t.path().join("tools/bad.cigar"),
        "strength medium\n\npub pull f(n: Int) -> Int {\n  snuff \"x\"\n}\n",
    )
    .unwrap();
    let main = t.path().join("main.cigar");
    std::fs::write(&main, "use tools.bad\n\nexhale bad.f(1)\n").unwrap();
    let c = check(&main);
    assert!(!c.ok());
}

#[test]
fn a_mild_file_still_may_not_write_other_medium_words() {
    let t = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(t.path().join("tools")).unwrap();
    std::fs::copy(
        case("run/mild-uses-medium-module")
            .parent()
            .unwrap()
            .join("tools/plan.cigar"),
        t.path().join("tools/plan.cigar"),
    )
    .unwrap();
    let main = t.path().join("main.cigar");
    std::fs::write(
        &main,
        "use tools.plan\n\nrecord P { a: Int }\nexhale plan.limit\n",
    )
    .unwrap();
    let c = check(&main);
    assert!(c.errors.iter().any(|d| d.code == "E1101"), "{:?}", c.errors);
    assert!(!c.errors.iter().any(|d| d.message.contains("modules")));
}

#[test]
fn a_wrong_typed_value_at_the_boundary_stops_the_run_with_e1210() {
    let home = tempfile::tempdir().unwrap();
    let dir = case("runtime/mild-boundary-e1210")
        .parent()
        .unwrap()
        .to_path_buf();
    assert!(check(&dir.join("main.cigar")).ok());
    let (code, out, err) = cigar(home.path(), &dir, &["--plain", "run", "main.cigar"]);
    assert_ne!(code, 0, "{out}{err}");
    assert!(err.contains("E1210"), "{err}");
    assert!(
        err.contains("n should be Int"),
        "names the parameter: {err}"
    );
    assert!(out.is_empty(), "nothing ran past the boundary: {out}");
}

fn full_project(t: &Path) -> PathBuf {
    let project = t.join("proj");
    std::fs::create_dir_all(project.join(".git")).unwrap();
    std::fs::write(project.join("a.cigar"), "strength full\n\nexhale 1\n").unwrap();
    project
}

#[test]
fn a_run_nobody_can_answer_goes_on_and_says_so_once_and_remembers_nothing() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let project = full_project(t.path());
    let (code, out, err) = cigar(&home, &project, &["--plain", "run", "a.cigar"]);
    assert_eq!(code, 0, "{out}{err}");
    assert_eq!(out.trim(), "1");
    assert_eq!(err.matches("not interactive").count(), 1, "{err}");
    assert!(
        !home.join("state/cigar/full-warning").exists(),
        "nothing was asked, so nothing is remembered"
    );
    // The project holds nothing of it either.
    let mut inside: Vec<String> = std::fs::read_dir(&project)
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    inside.sort();
    assert_eq!(inside, [".git", "a.cigar"]);
}

#[test]
fn the_config_line_turns_the_notice_off() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let project = full_project(t.path());
    std::fs::create_dir_all(home.join("config/cigar")).unwrap();
    std::fs::write(
        home.join("config/cigar/config"),
        "strength_full_warning = off\n",
    )
    .unwrap();
    let (code, out, err) = cigar(&home, &project, &["--plain", "run", "a.cigar"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(!err.contains("not interactive"), "{err}");
}

#[test]
fn a_remembered_project_is_not_noted_again() {
    let t = tempfile::tempdir().unwrap();
    let home = t.path().join("home");
    let project = full_project(t.path());
    std::fs::create_dir_all(home.join("state/cigar")).unwrap();
    let root = std::fs::canonicalize(&project).unwrap();
    std::fs::write(
        home.join("state/cigar/full-warning"),
        format!("{}\n", root.display()),
    )
    .unwrap();
    let (code, _, err) = cigar(&home, &project, &["--plain", "run", "a.cigar"]);
    assert_eq!(code, 0, "{err}");
    assert!(!err.contains("not interactive"), "{err}");
}
