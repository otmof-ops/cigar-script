// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar explain --layers <file>:<line>:<col>` (#13): peel the `into`,
//! `measure` or `within` at a position back to CigScript's core.
//!
//! The checker already chooses every layer when it checks the file (which
//! meaning, for which type); this reads those choices back, so what is
//! printed is what the lowering did and never a second opinion. Each line
//! of the reduction is one level down: full (as written), medium (the
//! chosen meaning as a call), mild (the CigScript it becomes), core (the
//! operation the burn rule watches).

use std::fs;
use std::path::PathBuf;

use cigscript::cli::Ctx;
use cigscript::diagnostics::usage;

use crate::check::{check_file_source, Checked, Kind};
use crate::cli::{exit, report_checked};
use crate::lower::WithinNote;
use crate::typeck::{Choice, LayerNote};

/// One level of a reduction.
#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub level: &'static str,
    pub text: String,
}

/// What stands at a position, reduced.
#[derive(Clone, Debug, PartialEq)]
pub struct Explained {
    /// `into`, `measure`, a declared layer's name, or `within`.
    pub what: String,
    /// The expression or block head as written.
    pub written: String,
    /// The layer chosen, for which type, and where the meaning came from.
    pub layer: Option<String>,
    pub ty: Option<String>,
    /// `built in`, `declared` or `chosen at run time`.
    pub source: Option<&'static str>,
    /// Does it touch the world? `None` when only the run can say.
    pub effect: Option<bool>,
    /// The `within` blocks around it, outermost first.
    pub context: Vec<String>,
    /// The sentence the editor shows on hover.
    pub headline: String,
    pub steps: Vec<Step>,
    /// Line and column of the start, in the file as written.
    pub line: usize,
    pub col: usize,
}

/// A position `file:line:col`.
#[allow(clippy::result_large_err)]
fn parse_position(s: &str) -> Result<(PathBuf, usize, usize), String> {
    let bad = || format!("`{s}` is not `file:line:col`");
    let (rest, col) = s.rsplit_once(':').ok_or_else(bad)?;
    let (file, line) = rest.rsplit_once(':').ok_or_else(bad)?;
    let line: usize = line.parse().map_err(|_| bad())?;
    let col: usize = col.parse().map_err(|_| bad())?;
    if file.is_empty() || line == 0 || col == 0 {
        return Err(bad());
    }
    Ok((PathBuf::from(file), line, col))
}

/// The byte offset of a 1-based line and column (in characters).
fn offset_of(source: &str, line: usize, col: usize) -> Option<usize> {
    let mut start = 0;
    for _ in 1..line {
        start += source[start..].find('\n')? + 1;
    }
    let text = &source[start..];
    let line_end = text.find('\n').unwrap_or(text.len());
    let mut chars = text[..line_end].char_indices();
    match chars.nth(col - 1) {
        Some((i, _)) => Some(start + i),
        None if col - 1 == text[..line_end].chars().count() => Some(start + line_end),
        None => None,
    }
}

/// The 1-based line and column (in characters) of a byte offset.
fn position_of(source: &str, off: usize) -> (usize, usize) {
    let before = &source[..off.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let from = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    (line, before[from..].chars().count() + 1)
}

/// `cigar explain --layers file:line:col`.
pub fn run_layers(ctx: &Ctx, position: &str) -> i32 {
    let fail = |d: cigscript::diagnostics::Diagnostic| {
        cigscript::cli::report(ctx, &d, None, None);
        exit::USAGE
    };
    let (path, line, col) = match parse_position(position) {
        Ok(p) => p,
        Err(m) => {
            return fail(usage(m).code("E800").with_hint(
                "name the place as file:line:col, as an error shows it: cigar explain --layers sales.cigar:12:1",
            ))
        }
    };
    if Kind::from_path(&path) != Some(Kind::Cigar) {
        return fail(
            usage(format!(
                "{}: layers belong to a .cigar file",
                path.display()
            ))
            .code("E800")
            .with_hint("a .cig script is CigScript and has no layers to peel"),
        );
    }
    let source = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return fail(
                usage(format!("cannot read {}: {e}", path.display()))
                    .code("E801")
                    .with_hint("name a file you can read"),
            )
        }
    };
    let Some(off) = offset_of(&source, line, col) else {
        return fail(
            usage(format!(
                "{} has no line {line}, column {col}",
                path.display()
            ))
            .code("E800")
            .with_hint("count lines and columns from 1, as an error shows them"),
        );
    };
    let checked = check_file_source(Kind::Cigar, &source, None, Some(&path));
    let name = path.to_string_lossy().to_string();
    let Some(map) = checked.map.as_ref() else {
        return not_explainable(ctx, &name, &checked);
    };
    if !checked.ok() {
        // What is wrong comes first: a layer that does not resolve has
        // nothing to show, and the error says why.
        report_checked(ctx, &name, &checked);
        return exit::CHECK;
    }
    let all = collect(&source, map.layers.as_slice(), map.withins());
    let Some(found) = at(&all, off) else {
        let places: Vec<String> = all
            .iter()
            .take(8)
            .map(|e| format!("{}:{} {}", e.line, e.col, e.what))
            .collect();
        let hint = if places.is_empty() {
            "this file has no `into`, `measure` or `within`".to_string()
        } else {
            format!("layers in this file: {}", places.join(", "))
        };
        return fail(
            usage(format!(
                "{name}:{line}:{col}: no `into`, `measure` or `within` here"
            ))
            .code("E800")
            .with_hint(hint),
        );
    };
    print(ctx, &name, found);
    exit::OK
}

/// A file the checker did not take as far as choosing layers.
fn not_explainable(ctx: &Ctx, name: &str, checked: &Checked) -> i32 {
    if !checked.errors.is_empty() {
        report_checked(ctx, name, checked);
        return exit::CHECK;
    }
    let d = usage(format!(
        "{name} is `strength {}`: it has no layers to peel",
        checked.strength.word()
    ))
    .code("E800")
    .with_hint("`into`, `measure` and `within` need `strength medium` or `strength full`");
    cigscript::cli::report(ctx, &d, None, None);
    exit::USAGE
}

/// Everything the file has to explain, each with its extent.
struct Entry {
    start: usize,
    end: usize,
    /// For a `within`, the whole block (so the blocks around a place are known).
    block_end: usize,
    explained: Explained,
}

impl std::ops::Deref for Entry {
    type Target = Explained;
    fn deref(&self) -> &Explained {
        &self.explained
    }
}

fn collect(source: &str, layers: &[LayerNote], withins: &[WithinNote]) -> Vec<Entry> {
    let mut out: Vec<Entry> = Vec::new();
    for l in layers {
        let (line, col) = position_of(source, l.start);
        let written = source.get(l.start..l.end).unwrap_or("").trim().to_string();
        out.push(Entry {
            start: l.start,
            end: l.end,
            block_end: l.end,
            explained: explain_layer(l, written, line, col),
        });
    }
    for w in withins {
        let (line, col) = position_of(source, w.start);
        out.push(Entry {
            start: w.start,
            end: w.head_end,
            block_end: w.end,
            explained: explain_within(w, line, col),
        });
    }
    out.sort_by_key(|e| (e.start, e.end));
    // The blocks around each place, outermost first (nesting order is
    // significant, so it is shown).
    let blocks: Vec<(usize, usize, String)> = withins
        .iter()
        .map(|w| (w.start, w.end, format!("within {}", w.head)))
        .collect();
    for e in out.iter_mut() {
        let mut around: Vec<&(usize, usize, String)> = blocks
            .iter()
            .filter(|(s, en, _)| *s < e.start && e.start < *en)
            .collect();
        around.sort_by_key(|(s, _, _)| *s);
        e.explained.context = around.into_iter().map(|(_, _, t)| t.clone()).collect();
    }
    out
}

/// The narrowest thing at an offset: a layer expression or a `within`
/// head first, and failing those the innermost `within` block around it.
fn at(all: &[Entry], off: usize) -> Option<&Entry> {
    let on = |e: &&Entry| e.start <= off && off < e.end.max(e.start + 1);
    all.iter()
        .filter(on)
        .min_by_key(|e| e.end.saturating_sub(e.start))
        .or_else(|| {
            all.iter()
                .filter(|e| e.what == "within" && e.start <= off && off < e.block_end)
                .max_by_key(|e| e.start)
        })
}

fn meaning(l: &LayerNote) -> (String, &'static str, Option<bool>) {
    match &l.choice {
        Choice::Builtin { function, effect } => {
            let m = match (l.layer.as_str(), function.as_str()) {
                ("into", "_cg_into_list") => "append to a list".to_string(),
                ("into", "_cg_into_channel") => "send to a channel".to_string(),
                ("into", _) => "write file".to_string(),
                ("measure", "fs.size") => "the size of the file".to_string(),
                ("measure", _) if l.ty == "Text" => "count the characters".to_string(),
                _ => "count the items".to_string(),
            };
            (m, "built in", Some(*effect))
        }
        Choice::Declared { function } => (
            format!("`{function}`, declared in this program"),
            "declared",
            None,
        ),
        Choice::Runtime => (
            "whichever layer the value's type has when it runs".to_string(),
            "chosen at run time",
            None,
        ),
    }
}

fn explain_layer(l: &LayerNote, written: String, line: usize, col: usize) -> Explained {
    let (meaning, source, effect) = meaning(l);
    // A text written where a path goes is the path layer's: say so.
    let ty = match &l.choice {
        Choice::Builtin { effect: true, .. } => "Path".to_string(),
        _ => l.ty.clone(),
    };
    let subject = if l.subject.is_empty() {
        "the value".to_string()
    } else {
        format!("`{}`", l.subject)
    };
    let headline = match &l.choice {
        Choice::Runtime => format!(
            "here, `{}` is chosen when the program runs: the type of {subject} is not known to the checker",
            l.layer
        ),
        _ => format!(
            "here, `{}` means *{meaning}*, because {subject} is a {ty}",
            l.layer
        ),
    };
    let mut steps = vec![Step {
        level: "full",
        text: written.clone(),
    }];
    let (v, s) = (&l.value, &l.subject);
    match &l.choice {
        Choice::Builtin { function, effect } if l.layer == "into" => {
            steps.push(Step {
                level: "medium",
                text: format!("{function}({v}, {s})"),
            });
            if *effect {
                let mild = if l.value_ty == "Text" {
                    format!("fs.write_text({s}, {v})")
                } else {
                    format!("fs.write_text({s}, json.stringify({v}))")
                };
                steps.push(Step {
                    level: "mild",
                    text: mild,
                });
                let burn = if l.in_burn {
                    "journaled, reversible"
                } else {
                    "refused outside a burn: E303"
                };
                steps.push(Step {
                    level: "core",
                    text: format!("burn op: write {s}   ({burn})"),
                });
            } else if function == "_cg_into_channel" {
                steps.push(Step {
                    level: "mild",
                    text: format!("{s}.send({v})"),
                });
                steps.push(Step {
                    level: "core",
                    text: "channel send   (in memory, no effect on the world; E1520 if closed)"
                        .to_string(),
                });
            } else {
                steps.push(Step {
                    level: "mild",
                    text: format!("{s}.push({v})"),
                });
                steps.push(Step {
                    level: "core",
                    text: "list append   (in memory, no effect on the world)".to_string(),
                });
            }
        }
        Choice::Builtin { function, effect } => {
            steps.push(Step {
                level: "medium",
                text: format!("{function}({s})"),
            });
            steps.push(Step {
                level: "mild",
                text: format!("{function}({s})"),
            });
            let core = if *effect {
                format!("burn op: {function}")
            } else if function == "fs.size" {
                "read op: the size of the file   (a read, so no burn needed)".to_string()
            } else {
                "pure: no effect on the world".to_string()
            };
            steps.push(Step {
                level: "core",
                text: core,
            });
        }
        Choice::Declared { function } => {
            let call = if l.layer == "into" {
                format!("{function}({s}, {v})")
            } else {
                format!("{function}({s})")
            };
            steps.push(Step {
                level: "medium",
                text: call.clone(),
            });
            steps.push(Step {
                level: "mild",
                text: format!("{call}   (the `pull {function}` you wrote)"),
            });
            steps.push(Step {
                level: "core",
                text: format!(
                    "whatever `{function}` does; a burn in it is journaled like any other"
                ),
            });
        }
        Choice::Runtime => {
            steps.push(Step {
                level: "mild",
                text: format!(
                    "a helper on this line picks the layer by the value's type, and coughs E1402 if none fits: `{written}`"
                ),
            });
            steps.push(Step {
                level: "core",
                text: "burn op: write file, when the target turns out to be a path (needs a burn)"
                    .to_string(),
            });
        }
    }
    Explained {
        what: l.layer.clone(),
        written,
        layer: Some(l.layer.clone()),
        ty: Some(ty),
        source: Some(source),
        effect,
        context: Vec::new(),
        headline,
        steps,
        line,
        col,
    }
}

fn explain_within(w: &WithinNote, line: usize, col: usize) -> Explained {
    let written = format!("within {}", w.head);
    let (headline, mild, core) = match w.context.as_str() {
        "retry" => {
            let gap = if w.ms > 0 {
                format!(", sleeping {} ms between tries", w.ms)
            } else {
                String::new()
            };
            (
                format!(
                    "here, `within retry` means *try the block up to {} times*; a cough retries the whole block",
                    w.arg
                ),
                format!(
                    "a loop the block runs inside: up to {} tries{gap}; the last failure is coughed again",
                    w.arg
                ),
                "try, ashtray and time.sleep_ms: nothing the core does not already have".to_string(),
            )
        }
        "parallel" => (
            "here, `within parallel` means *run the block in loop order*, which keeps the journal the sequential run's".to_string(),
            format!(
                "`if true {{ ... }}`: the block runs in loop order on one thread today; threads: {} is accepted, not yet used",
                if w.arg.is_empty() { "?" } else { &w.arg }
            ),
            "no new operation: the burns inside are journaled in order".to_string(),
        ),
        _ => (
            format!(
                "here, `within timeout` means *give the block {} to finish*, or cough E1404",
                if w.arg.is_empty() { "its limit" } else { &w.arg }
            ),
            format!(
                "roll deadline = time.now_ms() + {}; a check at the top of each for and while pass in the block, and one at its end; past the deadline: cough E1404",
                if w.ms > 0 { w.ms.to_string() } else { w.arg.clone() }
            ),
            "time.now_ms (a read) and a cough: no effect on the world".to_string(),
        ),
    };
    Explained {
        what: "within".to_string(),
        written: written.clone(),
        layer: None,
        ty: None,
        source: None,
        effect: Some(false),
        context: Vec::new(),
        headline,
        steps: vec![
            Step {
                level: "full",
                text: written,
            },
            Step {
                level: "mild",
                text: mild,
            },
            Step {
                level: "core",
                text: core,
            },
        ],
        line,
        col,
    }
}

fn print(ctx: &Ctx, name: &str, e: &Explained) {
    if ctx.json {
        let steps: Vec<serde_json::Value> = e
            .steps
            .iter()
            .map(|s| serde_json::json!({"level": s.level, "text": s.text}))
            .collect();
        let obj = serde_json::json!({
            "file": name,
            "line": e.line,
            "col": e.col,
            "what": e.what,
            "written": e.written,
            "layer": e.layer,
            "type": e.ty,
            "source": e.source,
            "effect": e.effect,
            "within": e.context,
            "headline": e.headline,
            "reduction": steps,
        });
        println!("{obj}");
        return;
    }
    println!("{}", ctx.bold(&format!("{name}:{}:{}", e.line, e.col)));
    println!("{}", capitalize(&e.headline));
    if !e.context.is_empty() {
        println!(
            "{}",
            ctx.dim(&format!(
                "inside {} (outermost first; the order matters)",
                e.context.join(" > ")
            ))
        );
    }
    println!();
    for s in &e.steps {
        println!("{:<8}{}", s.level, s.text);
    }
    if !ctx.plain {
        println!();
        println!(
            "{}",
            ctx.dim("every layer reduces to the core; context never hides an effect")
        );
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str() + ".",
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_file_line_col() {
        assert_eq!(
            parse_position("a/b.cigar:12:3").unwrap(),
            (PathBuf::from("a/b.cigar"), 12, 3)
        );
        assert!(parse_position("a.cigar:12").is_err());
        assert!(parse_position("a.cigar:0:1").is_err());
        assert!(parse_position("a.cigar:x:1").is_err());
    }

    #[test]
    fn offsets_and_positions_agree_in_characters() {
        let s = "héllo\nwörld into x\n";
        let off = offset_of(s, 2, 7).unwrap();
        assert_eq!(&s[off..off + 4], "into");
        assert_eq!(position_of(s, off), (2, 7));
        assert!(offset_of(s, 9, 1).is_none());
    }
}
