// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar fmt` (#16): one style, idempotent, comments kept, and a formatted
//! file checks exactly as before.

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use cigarscript::check::{check_file_source, Kind};
use cigarscript::fmt::format;
use cigarscript::lower::mask_raw;
use cigscript::syntax::token::{StrPart, TokenKind};

/// Every `.cigar` and `.cig` file under a folder, sorted.
fn files_under(root: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(root) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if p.is_dir() {
            if name != "target" && name != ".git" && name != "node_modules" {
                files_under(&p, out);
            }
        } else if matches!(
            p.extension().and_then(|x| x.to_str()),
            Some("cigar") | Some("cig")
        ) {
            out.push(p);
        }
    }
}

fn corpus() -> Vec<PathBuf> {
    let mut out = Vec::new();
    files_under(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("tests"),
        &mut out,
    );
    files_under(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("bench"),
        &mut out,
    );
    if let Some(src) = common::cigscript_src() {
        for sub in ["examples", "tests"] {
            files_under(&src.join(sub), &mut out);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn kind_of(p: &Path) -> Kind {
    Kind::from_path(p).expect("a .cig or .cigar file")
}

/// The tokens of a source, positions ignored.
fn tokens(src: &str) -> Vec<String> {
    let Ok((_, toks, _, _)) = mask_raw(src) else {
        return vec!["lex error".into()];
    };
    let mut out: Vec<String> = Vec::new();
    for t in toks {
        let s = match t.kind {
            TokenKind::Newline if out.last().is_some_and(|l| l == "nl") => continue,
            TokenKind::Newline => "nl".to_string(),
            TokenKind::Str(parts) => parts
                .iter()
                .map(|p| match p {
                    StrPart::Lit(l) => format!("L{l:?}"),
                    StrPart::Interp { src, .. } => format!("I{src:?}"),
                })
                .collect::<String>(),
            other => format!("{other:?}"),
        };
        out.push(s);
    }
    out
}

fn comments(src: &str) -> Vec<String> {
    // Comments in a gap-only sense: a `#` outside a string. Good enough for
    // the test inputs, which hold no `#` in strings.
    src.lines()
        .filter_map(|l| l.find('#').map(|i| l[i..].trim_end().to_string()))
        .collect()
}

#[test]
fn formatting_twice_is_formatting_once_over_the_whole_corpus() {
    let files = corpus();
    assert!(files.len() > 200, "the corpus is {} files", files.len());
    let (mut formatted, mut unlexable) = (0, 0);
    for p in &files {
        let src = std::fs::read_to_string(p).unwrap();
        if mask_raw(&src).is_err() {
            // A file that does not lex has nothing to lay out.
            assert!(format(&src).is_err(), "{}", p.display());
            unlexable += 1;
            continue;
        }
        let once = format(&src).unwrap_or_else(|d| panic!("{}: {}", p.display(), d.message));
        let twice =
            format(&once).unwrap_or_else(|d| panic!("{} (2nd): {}", p.display(), d.message));
        assert_eq!(once, twice, "not idempotent: {}", p.display());
        assert!(once.ends_with('\n') || once.is_empty(), "{}", p.display());
        assert!(!once.contains("\r"), "{}", p.display());
        assert!(
            once.lines().all(|l| l == l.trim_end()),
            "trailing space in {}",
            p.display()
        );
        formatted += 1;
    }
    assert!(
        formatted > 200,
        "formatted {formatted}, unlexable {unlexable}"
    );
}

#[test]
fn a_formatted_file_checks_exactly_as_before() {
    for p in corpus() {
        let src = std::fs::read_to_string(&p).unwrap();
        let Ok(out) = format(&src) else { continue };
        let kind = kind_of(&p);
        let a = check_file_source(kind, &src, None, Some(&p));
        let b = check_file_source(kind, &out, None, Some(&p));
        let codes = |c: &cigarscript::check::Checked| {
            c.errors
                .iter()
                .chain(&c.warnings)
                .chain(&c.hints)
                .map(|d| (d.code.to_string(), d.kind.as_str().to_string()))
                .collect::<Vec<_>>()
        };
        assert_eq!(codes(&a), codes(&b), "diagnostics changed: {}", p.display());
        assert_eq!(a.ok(), b.ok(), "{}", p.display());
        // The lowered program is the same tokens.
        assert_eq!(
            tokens(&a.source_as_cigscript_saw_it),
            tokens(&b.source_as_cigscript_saw_it),
            "lowered program changed: {}",
            p.display()
        );
        assert_eq!(tokens(&src), tokens(&out), "{}", p.display());
    }
}

#[test]
fn every_comment_is_kept_in_order() {
    let src = "# one\nstrength medium  # two\n\n\n# three\npull f(a) {   # four\n  # five\n  snuff a # six\n    # seven\n}\n# eight";
    let out = format(src).unwrap();
    assert_eq!(comments(src), comments(&out));
    assert!(out.contains("strength medium # two\n"), "{out}");
    assert!(out.ends_with("# eight\n"), "{out}");
}

#[test]
fn the_house_style() {
    let src = "strength medium\n\n\n\npull  area( s:Shape , borrow xs :List<Int>)->Float{\n\tsnuff -1+2*3\n}\nrecord Job{id:Int,step:Move?}\nstick m={a:1,b:[1,2]}\nexhale measure ({a: 1}), measure(x)\nfor n in 1 .. 4{exhale n}\nstick z = a\n>> f\n>> g\nstick w: Map<Text,List<Int>> = {}\nif (x<3 and not y) { exhale ann ?. nick ?? \"x\" } else { exhale r#match }\n";
    let want = "strength medium\n\npull area(s: Shape, borrow xs: List<Int>) -> Float {\n  snuff -1 + 2 * 3\n}\nrecord Job { id: Int, step: Move? }\nstick m = {a: 1, b: [1, 2]}\nexhale measure ({a: 1}), measure(x)\nfor n in 1..4 { exhale n }\nstick z = a\n  >> f\n  >> g\nstick w: Map<Text, List<Int>> = {}\nif (x<3 and not y) { exhale ann?.nick ?? \"x\" } else { exhale r#match }\n";
    assert_eq!(format(src).unwrap(), want);
}

#[test]
fn a_duration_keeps_its_unit_against_its_number() {
    let out = format("within  timeout( 2s ){\nexhale 1\n}\n").unwrap();
    assert_eq!(out, "within timeout(2s) {\n  exhale 1\n}\n");
}

#[test]
fn nested_blocks_and_multiline_literals_indent_by_their_open_lines() {
    let src =
        "pull f() {\nstick xs = [\n1,\n2,\n]\nfoo(a,\nb)\nif x {\nwhile y {\nbreak\n}\n}\n}\n";
    let want = "pull f() {\n  stick xs = [\n    1,\n    2,\n  ]\n  foo(a,\n    b)\n  if x {\n    while y {\n      break\n    }\n  }\n}\n";
    assert_eq!(format(src).unwrap(), want);
}

#[test]
fn a_file_that_does_not_lex_is_refused_not_mangled() {
    assert!(format("stick x = \"never closed\n").is_err());
}

fn cigar(dir: &Path, args: &[&str]) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cigar"))
        .args(args)
        .current_dir(dir)
        .env("HOME", dir.join("home"))
        .env("CIGSCRIPT_HOME", dir.join("cig"))
        .env("XDG_CONFIG_HOME", dir.join("xdg/config"))
        .env("XDG_CACHE_HOME", dir.join("xdg/cache"))
        .env("XDG_DATA_HOME", dir.join("xdg/data"))
        .env("XDG_STATE_HOME", dir.join("xdg/state"))
        .env("NO_COLOR", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        if start.elapsed() > Duration::from_secs(60) {
            let _ = child.kill();
            panic!("cigar {args:?} ran past 60 s");
        }
        std::thread::yield_now();
    }
    let out = child.wait_with_output().unwrap();
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn check_names_what_would_change_and_exits_nonzero_then_fmt_fixes_it() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("bad.cigar");
    let good = dir.path().join("good.cigar");
    std::fs::write(&bad, "stick   x=1\nexhale x\n").unwrap();
    std::fs::write(&good, "stick x = 1\nexhale x\n").unwrap();

    let (code, out, _) = cigar(dir.path(), &["fmt", "--check", "bad.cigar", "good.cigar"]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("bad.cigar"), "{out}");
    assert!(!out.contains("good.cigar"), "{out}");
    // --check changes nothing.
    assert_eq!(
        std::fs::read_to_string(&bad).unwrap(),
        "stick   x=1\nexhale x\n"
    );

    let (code, out, _) = cigar(dir.path(), &["fmt", "bad.cigar", "good.cigar"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read_to_string(&bad).unwrap(),
        "stick x = 1\nexhale x\n"
    );

    let (code, out, _) = cigar(dir.path(), &["fmt", "--check", "."]);
    assert_eq!(code, 0, "{out}");
    let (code, _, err) = cigar(dir.path(), &["fmt"]);
    assert_eq!(code, 3, "{err}");
}

#[test]
fn json_check_prints_one_object_per_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.cig"), "exhale  1\n").unwrap();
    let (code, out, _) = cigar(dir.path(), &["--json", "fmt", "--check", "a.cig"]);
    assert_eq!(code, 2);
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(v["would_change"], true);
    assert_eq!(v["file"], "a.cig");
}
