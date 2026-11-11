// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Layered depth (#13): the design document's four examples compile, run,
//! and have their reductions printed by `cigar explain --layers`.
//!
//! Every child process runs in a scratch folder with its own HOME,
//! CIGSCRIPT_HOME and XDG folders, under a hard timeout.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use wait_timeout::ChildExt;

/// What one `cigar` run gave: exit code, stdout, stderr.
struct Out {
    code: i32,
    out: String,
    err: String,
}

fn cigar(dir: &Path, args: &[&str]) -> Out {
    let state = dir.join(".state");
    let mut child = Command::new(env!("CARGO_BIN_EXE_cigar"))
        .arg("--plain")
        .args(args)
        .current_dir(dir)
        .env("HOME", &state)
        .env("CIGSCRIPT_HOME", state.join("cig"))
        .env("XDG_CACHE_HOME", state.join("cache"))
        .env("XDG_CONFIG_HOME", state.join("config"))
        .env("XDG_DATA_HOME", state.join("data"))
        .env("XDG_STATE_HOME", state.join("state"))
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cigar starts");
    let code = match child
        .wait_timeout(Duration::from_secs(60))
        .expect("waiting on cigar")
    {
        Some(status) => status.code().unwrap_or(-1),
        None => {
            let _ = child.kill();
            let _ = child.wait();
            panic!("cigar {args:?} ran past 60 s");
        }
    };
    let mut out = String::new();
    let mut err = String::new();
    child
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    child
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    Out { code, out, err }
}

/// A scratch folder holding `main.cigar`.
fn folder(source: &str) -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("main.cigar"), source).unwrap();
    d
}

/// The reduction lines `--layers` printed: (level, text).
fn steps(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let (level, text) = l.split_once(' ')?;
            matches!(level, "full" | "medium" | "mild" | "core")
                .then(|| (level.to_string(), text.trim().to_string()))
        })
        .collect()
}

fn text_of<'a>(steps: &'a [(String, String)], level: &str) -> &'a str {
    steps
        .iter()
        .find(|(l, _)| l == level)
        .map(|(_, t)| t.as_str())
        .unwrap_or_else(|| panic!("no `{level}` line in {steps:?}"))
}

// ---- Example 1: one expression, many meanings: `into` ----------------------

const EXAMPLE_1: &str = r#"strength full

roll seen = []
stick rows = ["a", "b"]
stick workers = channel<List<Text>>()

rows into seen
rows into workers
burn {
  rows into "report.csv"
}
exhale seen
exhale workers.receive()
"#;

#[test]
fn example_1_into_runs_and_writes_only_inside_the_burn() {
    let d = folder(EXAMPLE_1);
    let r = cigar(d.path(), &["run", "main.cigar"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.out, "[[\"a\", \"b\"]]\n[\"a\", \"b\"]\n");
    assert_eq!(
        std::fs::read_to_string(d.path().join("report.csv")).unwrap(),
        "[\"a\",\"b\"]"
    );
}

#[test]
fn example_1_layers_for_a_list_a_channel_and_a_path() {
    let d = folder(EXAMPLE_1);
    // Line 7: `rows into seen`.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:7:1"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .contains("`into` means *append to a list*, because `seen` is a List"),
        "{}",
        r.out
    );
    let s = steps(&r.out);
    assert_eq!(text_of(&s, "full"), "rows into seen");
    assert_eq!(text_of(&s, "mild"), "seen.push(rows)");

    // Line 8: `rows into workers`.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:8:6"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let s = steps(&r.out);
    assert_eq!(text_of(&s, "mild"), "workers.send(rows)");

    // Line 10: the file case, the one the design doc peels.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:10:3"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .contains("`into` means *write file*, because `\"report.csv\"` is a Path"),
        "{}",
        r.out
    );
    let s = steps(&r.out);
    assert_eq!(text_of(&s, "full"), "rows into \"report.csv\"");
    assert_eq!(
        text_of(&s, "mild"),
        "fs.write_text(\"report.csv\", json.stringify(rows))"
    );
    assert!(
        text_of(&s, "core").starts_with("burn op: write \"report.csv\""),
        "{s:?}"
    );
    assert!(text_of(&s, "core").contains("journaled, reversible"));
}

#[test]
fn example_1_layers_json_has_the_same_reduction() {
    let d = folder(EXAMPLE_1);
    let r = cigar(
        d.path(),
        &["--json", "explain", "--layers", "main.cigar:10:3"],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(r.out.trim()).expect("one JSON object");
    assert_eq!(v["what"], "into");
    assert_eq!(v["type"], "Path");
    assert_eq!(v["source"], "built in");
    assert_eq!(v["effect"], true);
    let levels: Vec<&str> = v["reduction"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["level"].as_str().unwrap())
        .collect();
    assert_eq!(levels, ["full", "medium", "mild", "core"]);
}

#[test]
fn example_1_outside_a_burn_the_core_line_says_it_is_refused() {
    // The same write, as a function's body: a warning (E303), so it checks,
    // and the reduction says what the run would refuse.
    let d = folder(
        "strength medium\n\npull save(rows: List<Int>) {\n  rows into \"report.csv\"\n}\n\nburn {\n  save([1])\n}\n",
    );
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:4:3"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("refused outside a burn: E303"), "{}", r.out);
}

#[test]
fn example_1_stats_totals_waits_for_scripts_that_read_each_other() {
    // `rows into stats.totals` and `stick total = stats.total` are #14's
    // (context reads); today the name is simply unknown.
    let d = folder("strength full\n\nstick total = stats.total\n");
    let r = cigar(d.path(), &["check", "main.cigar"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("E301"), "{}", r.err);
}

// ---- Example 2: context blocks: `within` -------------------------------------

const EXAMPLE_2: &str = r#"strength full

roll done = 0
within retry(3, backoff: 1ms) {
  within parallel(threads: 8) {
    burn {
      for n in [1, 2, 3] {
        "x" into "thumb-${n}.txt"
        done += 1
      }
    }
  }
}
exhale done
"#;

#[test]
fn example_2_within_runs_and_the_journal_keeps_every_write_in_order() {
    let d = folder(EXAMPLE_2);
    let r = cigar(d.path(), &["run", "--dry-run", "main.cigar"]);
    assert_eq!(r.code, 0, "{}", r.err);
    let plan = format!("{}{}", r.out, r.err);
    let at = |n: u32| {
        plan.find(&format!("write thumb-{n}.txt"))
            .unwrap_or_else(|| panic!("thumb-{n} not in the plan: {plan}"))
    };
    assert!(at(1) < at(2) && at(2) < at(3), "{plan}");
    let r = cigar(d.path(), &["run", "main.cigar"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.out, "3\n");
    assert!(d.path().join("thumb-3.txt").exists());
}

#[test]
fn example_2_layers_show_each_context_and_the_nesting_order() {
    let d = folder(EXAMPLE_2);
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:4:1"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r
        .out
        .contains("`within retry` means *try the block up to 3 times*"));
    let s = steps(&r.out);
    assert_eq!(text_of(&s, "full"), "within retry(3, backoff: 1ms)");
    assert!(text_of(&s, "mild").contains("up to 3 tries, sleeping 1 ms"));

    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:5:3"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .contains("inside within retry(3, backoff: 1ms) (outermost first"),
        "{}",
        r.out
    );
    assert!(text_of(&steps(&r.out), "mild").contains("in loop order"));

    // The write inside both blocks names both, outermost first.
    let r = cigar(
        d.path(),
        &["--json", "explain", "--layers", "main.cigar:8:9"],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(r.out.trim()).unwrap();
    assert_eq!(
        v["within"],
        serde_json::json!([
            "within retry(3, backoff: 1ms)",
            "within parallel(threads: 8)"
        ])
    );
}

#[test]
fn example_2_swapping_the_order_swaps_the_reported_order() {
    let d = folder(
        "strength full\n\nwithin parallel(threads: 2) {\n  within retry(2) {\n    exhale 1\n  }\n}\n",
    );
    let r = cigar(
        d.path(),
        &["--json", "explain", "--layers", "main.cigar:4:3"],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(r.out.trim()).unwrap();
    assert_eq!(
        v["within"],
        serde_json::json!(["within parallel(threads: 2)"])
    );
    assert_eq!(v["what"], "within");
}

// ---- Example 3: scripts that read each other: #14 ----------------------------
// Covered above (`example_1_stats_totals_...`): it is cig-orchestrator's, and
// is refused with an unknown name until #14 builds the reference syntax.

// ---- Example 4: one question, the right answer for each thing: `measure` ----

const EXAMPLE_4: &str = r#"strength full

pull size_of(p: Path) -> Int {
  snuff measure p
}

stick p: Path = "data.txt"
exhale measure [1, 2, 3]
exhale size_of(p)
exhale measure "héllo"
"#;

#[test]
fn example_4_measure_answers_by_type() {
    let d = folder(EXAMPLE_4);
    std::fs::write(d.path().join("data.txt"), "0123456789").unwrap();
    let r = cigar(d.path(), &["run", "main.cigar"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.out, "3\n10\n5\n");
}

#[test]
fn example_4_layers_for_a_list_a_path_and_text() {
    let d = folder(EXAMPLE_4);
    // A path: its size, a read, so no burn is needed.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:4:9"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(
        r.out
            .contains("`measure` means *the size of the file*, because `p` is a Path"),
        "{}",
        r.out
    );
    let s = steps(&r.out);
    assert_eq!(text_of(&s, "medium"), "fs.size(p)");
    assert!(text_of(&s, "core").starts_with("read op: the size of the file"));

    // A list: its length.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:8:8"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("*count the items*"), "{}", r.out);
    assert_eq!(text_of(&steps(&r.out), "medium"), "len([1, 2, 3])");

    // Text: characters, not bytes.
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:10:8"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r.out.contains("*count the characters*"), "{}", r.out);
}

#[test]
fn example_4_a_declared_layer_is_named_as_declared() {
    let d = folder(
        "strength full\n\nrecord Move { from: Text, to: Text }\n\npull cost(m: Move) -> Int {\n  snuff 1\n}\n\nmeasure for Move = cost\n\nstick m = Move { from: \"a\", to: \"b\" }\nexhale measure m\n",
    );
    let r = cigar(
        d.path(),
        &["--json", "explain", "--layers", "main.cigar:12:8"],
    );
    assert_eq!(r.code, 0, "{}", r.err);
    let v: serde_json::Value = serde_json::from_str(r.out.trim()).unwrap();
    assert_eq!(v["source"], "declared");
    assert_eq!(v["type"], "Move");
    assert_eq!(v["reduction"][1]["text"], "cost(m)");
}

// ---- errors the command itself gives -----------------------------------------

#[test]
fn explain_says_what_is_at_the_place_when_nothing_is() {
    let d = folder(EXAMPLE_1);
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:1:1"]);
    assert_eq!(r.code, 3);
    assert!(r.err.contains("no `into`, `measure` or `within` here"));
    assert!(r.err.contains("7:1 into"), "{}", r.err);
}

#[test]
fn explain_refuses_a_bad_position_a_cig_file_and_a_mild_file() {
    let d = folder("strength mild\n\nexhale 1\n");
    std::fs::write(d.path().join("a.cig"), "exhale 1\n").unwrap();
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar"]);
    assert_eq!(r.code, 3);
    assert!(r.err.contains("file:line:col"), "{}", r.err);
    let r = cigar(d.path(), &["explain", "--layers", "a.cig:1:1"]);
    assert_eq!(r.code, 3);
    assert!(
        r.err.contains("layers belong to a .cigar file"),
        "{}",
        r.err
    );
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:3:1"]);
    assert_eq!(r.code, 3);
    assert!(r.err.contains("no layers to peel"), "{}", r.err);
    let r = cigar(d.path(), &["explain"]);
    assert_eq!(r.code, 3);
    assert!(r.err.contains("name a place to peel"), "{}", r.err);
}

#[test]
fn explain_reports_the_error_when_a_layer_does_not_resolve() {
    let d = folder("strength medium\n\nexhale measure 5\n");
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:3:8"]);
    assert_eq!(r.code, 2);
    assert!(r.err.contains("E1402"), "{}", r.err);
}

// ---- `within timeout` ---------------------------------------------------------

#[test]
fn timeout_coughs_in_the_run_and_rolls_the_burns_back() {
    let d = folder(
        "strength full\n\nburn {\n  \"x\" into \"late.txt\"\n}\nwithin timeout(20ms) {\n  while true {\n    time.sleep_ms(5)\n  }\n}\n",
    );
    let r = cigar(d.path(), &["run", "main.cigar"]);
    assert_eq!(r.code, 1, "{}", r.err);
    assert!(r.err.contains("E1404"), "{}", r.err);
    assert!(
        !d.path().join("late.txt").exists(),
        "a failed run rolls its burns back"
    );
}

#[test]
fn timeout_under_retry_is_tried_again() {
    let d = folder(
        "strength full\n\nroll tries = 0\nwithin retry(2) {\n  tries += 1\n  within timeout(20ms) {\n    while tries < 2 {\n      time.sleep_ms(5)\n    }\n    exhale \"finished on try ${tries}\"\n  }\n}\n",
    );
    let r = cigar(d.path(), &["run", "main.cigar"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert_eq!(r.out, "finished on try 2\n");
}

#[test]
fn timeout_layers_say_what_it_reduces_to() {
    let d = folder("strength full\n\nwithin timeout(2s) {\n  exhale 1\n}\n");
    let r = cigar(d.path(), &["explain", "--layers", "main.cigar:3:1"]);
    assert_eq!(r.code, 0, "{}", r.err);
    assert!(r
        .out
        .contains("`within timeout` means *give the block 2s to finish*"));
    assert!(text_of(&steps(&r.out), "mild").contains("time.now_ms() + 2000"));
}
