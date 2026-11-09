// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! Every CigarScript sample in the docs is checked (#6). A ```cigar block in
//! the README or under docs/ must pass `cigar check`, unless it says which
//! error it gives first: `# expect: E1101` as its first line, or
//! `<!-- expect: E1101 -->` on the line before the fence (unseen when the
//! page is read, so the sample stays the very file its output came from).
//! The design document is left out: its samples are the language as
//! designed, most of it not built yet.
//!
//! A sample in several files writes each module as its own fence, with its
//! path after the language (```cigar tools/plan.cigar), before the file
//! that uses it: the modules are written to a folder and the file is
//! checked there.

use cigarscript::check::{check_file_source, check_source, Kind};
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
    /// The modules written before it: (path, text).
    modules: Vec<(String, String)>,
}

/// Each ```cigar block, with its expectation from either place, and the
/// modules written before it.
fn samples(text: &str) -> Vec<Sample> {
    let mut out = Vec::new();
    let mut open: Option<(Sample, Option<String>)> = None;
    let mut before: Option<String> = None;
    let mut modules: Vec<(String, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let fence = line.trim_start();
        match &mut open {
            None if fence.starts_with("```cigar") => {
                let path = fence["```cigar".len()..].trim();
                let path = (!path.is_empty()).then(|| path.to_string());
                open = Some((
                    Sample {
                        line: i + 1,
                        text: String::new(),
                        expect: before.take(),
                        modules: Vec::new(),
                    },
                    path,
                ))
            }
            Some((s, path)) if fence.starts_with("```") => {
                if let Some(p) = path.take() {
                    modules.push((p, std::mem::take(&mut s.text)));
                    open = None;
                    continue;
                }
                if s.expect.is_none() {
                    s.expect = s
                        .text
                        .lines()
                        .next()
                        .and_then(|l| l.trim().strip_prefix("# expect:"))
                        .map(|c| c.trim().to_string());
                }
                let (mut s, _) = open.take().unwrap();
                s.modules = std::mem::take(&mut modules);
                out.push(s);
            }
            Some((s, _)) => {
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
        for Sample {
            line,
            text,
            expect,
            modules,
        } in samples(&text)
        {
            seen += 1;
            let checked = if modules.is_empty() {
                check_source(Kind::Cigar, &text, None)
            } else {
                let dir = tempfile::tempdir().unwrap();
                for (p, t) in &modules {
                    let at = dir.path().join(p);
                    std::fs::create_dir_all(at.parent().unwrap()).unwrap();
                    std::fs::write(at, t).unwrap();
                }
                let main = dir.path().join("main.cigar");
                std::fs::write(&main, &text).unwrap();
                check_file_source(Kind::Cigar, &text, None, Some(&main))
            };
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
            expect: None,
            modules: Vec::new(),
        }
    );
    assert_eq!(s[1].line, 9);
    assert_eq!(s[1].expect.as_deref(), Some("E1101"));
    assert_eq!(s[2].expect.as_deref(), Some("E1103"));
    assert_eq!(s[2].text, "strength spicy\n");
}

#[test]
fn a_module_fence_goes_with_the_sample_after_it() {
    let text = "```cigar tools/plan.cigar\nstrength medium\npub stick limit = 3\n```\n\n```cigar\nstrength medium\nuse tools.plan\nexhale plan.limit\n```\n```cigar\nstrength mild\n```\n";
    let s = samples(text);
    assert_eq!(s.len(), 2);
    assert_eq!(
        s[0].modules,
        vec![(
            "tools/plan.cigar".to_string(),
            "strength medium\npub stick limit = 3\n".to_string()
        )]
    );
    assert!(s[1].modules.is_empty());
}
