// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! Every CigarScript sample in the docs is checked (#6). A ```cigar block in
//! the README or under docs/ must pass `cigar check`, unless it says which
//! error it gives first: `# expect: E1101` as its first line, or
//! `<!-- expect: E1101 -->` on the line before the fence (unseen when the
//! page is read, so the sample stays the very file its output came from).
//! The design document is left out: its samples are the language as
//! designed, most of it not built yet.

use cigarscript::check::{check_source, Kind};
use std::path::{Path, PathBuf};

/// The design, not the docs of what is built.
const NOT_CHECKED: &[&str] = &["docs/CIGARSCRIPT-AND-CIGCOMPILE.md"];

fn docs() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "md") {
                out.push(p);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut out = vec![root.join("README.md")];
    walk(&root.join("docs"), &mut out);
    out.retain(|p| !NOT_CHECKED.iter().any(|n| p.ends_with(n)));
    out.sort();
    out
}

/// A sample: the line its fence opens on, its text, and the error it says
/// it gives first.
#[derive(Debug, PartialEq)]
struct Sample {
    line: usize,
    text: String,
    expect: Option<String>,
}

/// Each ```cigar block, with its expectation from either place.
fn samples(text: &str) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut open: Option<Sample> = None;
    let mut before: Option<String> = None;
    for (i, line) in text.lines().enumerate() {
        let fence = line.trim_start();
        match &mut open {
            None if fence.starts_with("```cigar") => {
                open = Some(Sample {
                    line: i + 1,
                    text: String::new(),
                    expect: before.take(),
                })
            }
            Some(s) if fence.starts_with("```") => {
                if s.expect.is_none() {
                    s.expect = s
                        .text
                        .lines()
                        .next()
                        .and_then(|l| l.trim().strip_prefix("# expect:"))
                        .map(|c| c.trim().to_string());
                }
                out.push(open.take().unwrap());
            }
            Some(s) => {
                s.text.push_str(line);
                s.text.push('\n');
            }
            None => {
                before = fence
                    .strip_prefix("<!-- expect:")
                    .and_then(|r| r.strip_suffix("-->"))
                    .map(|c| c.trim().to_string());
            }
        }
    }
    out
}

#[test]
fn every_cigar_sample_in_the_docs_checks_as_it_says() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut seen = 0;
    let mut wrong = Vec::new();
    for f in docs() {
        let text = std::fs::read_to_string(&f).unwrap();
        let shown = f.strip_prefix(root).unwrap_or(&f).display().to_string();
        for Sample { line, text, expect } in samples(&text) {
            seen += 1;
            let checked = check_source(Kind::Cigar, &text, None);
            let first = checked.errors.first();
            match (&expect, first) {
                (None, None) => {}
                (Some(code), Some(e)) if e.code == code => {}
                (None, Some(e)) => wrong.push(format!(
                    "{shown}:{line}: should pass, and gives {} {}",
                    e.code, e.message
                )),
                (Some(code), Some(e)) => wrong.push(format!(
                    "{shown}:{line}: should give {code} first, and gives {} {}",
                    e.code, e.message
                )),
                (Some(code), None) => {
                    wrong.push(format!("{shown}:{line}: should give {code}, and passes"))
                }
            }
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
    // The docs carry samples; a docs folder without any would pass silently.
    assert!(seen > 0, "no ```cigar sample found in the docs");
}

#[test]
fn a_fence_reader_finds_each_block_its_line_and_what_it_expects() {
    let text = "intro\n```cigar\nstrength mild\n```\n\n```sh\ncigar check .\n```\n```cigar\n# expect: E1101\nrecord A {}\n```\n<!-- expect: E1103 -->\n```cigar\nstrength spicy\n```\n";
    let s = samples(text);
    assert_eq!(s.len(), 3);
    assert_eq!(
        s[0],
        Sample {
            line: 2,
            text: "strength mild\n".to_string(),
            expect: None
        }
    );
    assert_eq!(s[1].line, 9);
    assert_eq!(s[1].expect.as_deref(), Some("E1101"));
    assert_eq!(s[2].expect.as_deref(), Some("E1103"));
    assert_eq!(s[2].text, "strength spicy\n");
}
