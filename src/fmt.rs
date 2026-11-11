// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar fmt`: one style, no options (#16).
//!
//! The formatter reads a file with the lexer `cigar check` reads it with
//! (`lower::mask_raw`, cigscript's own tokens); it has no parser of its own.
//! It lays the tokens out again: indentation, spacing and blank lines are
//! rewritten, every token and every comment is kept as written. Comments
//! are not tokens, so they are found in the gaps between tokens.
//!
//! Two guards keep a format from changing a program. The layout never moves
//! a token across a line break (a newline ends a statement, so it is kept),
//! and the result is read again: if its tokens are not the input's, the
//! file is refused and left as it was. The rules are in `docs/commands.md`.

// cigscript's `Diagnostic` is large; every lexer-facing function here returns it.
#![allow(clippy::result_large_err)]

use cigscript::diagnostics::{Diagnostic, Kind as DK};
use cigscript::syntax::token::{StrPart, TokenKind as T};

use crate::lower::mask_raw;

/// Words that read differently with a space before `(` or `[`
/// (`measure(x)` or `measure (x)`): the author's spacing is kept.
const CONTEXTUAL: &[&str] = &[
    "strength",
    "coughs",
    "record",
    "enum",
    "interface",
    "impl",
    "match",
    "use",
    "pub",
    "into",
    "measure",
    "owned",
    "borrow",
    "move",
    "parallel",
    "channel",
    "within",
    "layer",
    "foreign",
    "unchecked",
];

const INDENT: usize = 2;

struct Item {
    kind: T,
    text: String,
    /// Newlines in the gap before it (after the last comment, if any).
    nl: usize,
    /// Comments before it: the text, and the newlines before each.
    comments: Vec<(String, usize)>,
    /// Written right against the token before (no space, no newline).
    adjacent: bool,
    /// Followed in the source by a lone `?` (an optional type), now part of `text`.
    opt: bool,
    /// `-` and `>` written together.
    arrow: bool,
}

struct Gap {
    comments: Vec<(String, usize)>,
    nl_last: usize,
    suffix: String,
    prefix: String,
    spaces: usize,
}

fn scan_gap(src: &[u8], masked: &[u8], a: usize, b: usize) -> Gap {
    let mut g = Gap {
        comments: Vec::new(),
        nl_last: 0,
        suffix: String::new(),
        prefix: String::new(),
        spaces: 0,
    };
    let mut nl = 0;
    let mut i = a;
    while i < b {
        let c = masked[i];
        if c == b'#' {
            let mut j = i;
            while j < b && masked[j] != b'\n' {
                j += 1;
            }
            let text = String::from_utf8_lossy(&src[i..j]).trim_end().to_string();
            g.comments.push((text, nl));
            nl = 0;
            i = j;
            continue;
        }
        if c == b'\n' {
            nl += 1;
        } else if src[i] != c {
            // A `?` or `r#` the lexer was never shown.
            if src[i] == b'?' && g.prefix.is_empty() {
                g.suffix.push('?');
            } else {
                g.prefix.push(src[i] as char);
            }
        } else if c.is_ascii_whitespace() {
            g.spaces += 1;
        }
        i += 1;
    }
    g.nl_last = nl;
    g
}

fn value_end(k: &T) -> bool {
    matches!(
        k,
        T::Int(_)
            | T::IntMinMagnitude
            | T::Float(_)
            | T::Str(_)
            | T::Ident(_)
            | T::RParen
            | T::RBracket
            | T::RBrace
            | T::True
            | T::False
            | T::Null
    )
}

fn binary_op(k: &T) -> bool {
    matches!(
        k,
        T::Assign
            | T::PlusAssign
            | T::MinusAssign
            | T::StarAssign
            | T::SlashAssign
            | T::Plus
            | T::Star
            | T::Slash
            | T::Percent
            | T::EqEq
            | T::NotEq
            | T::Lt
            | T::LtEq
            | T::Gt
            | T::GtEq
            | T::Coalesce
            | T::Pipe
            | T::And
            | T::Or
    )
}

fn build(src: &str) -> Result<Vec<Item>, Diagnostic> {
    let (masked, tokens, _, _) = mask_raw(src)?;
    let (s, m) = (src.as_bytes(), masked.as_bytes());
    let mut items: Vec<Item> = Vec::new();
    let mut prev_end = 0usize;
    let mut pending_prefix = String::new();
    for t in &tokens {
        if matches!(t.kind, T::Newline) {
            continue;
        }
        let eof = matches!(t.kind, T::Eof);
        let start = if eof { src.len() } else { t.span.start };
        let end = if eof { src.len() } else { t.span.end };
        if start < prev_end || end > src.len() {
            return Err(Diagnostic::new(DK::Syntax, "fmt: tokens out of order"));
        }
        let g = scan_gap(s, m, prev_end, start);
        if !g.suffix.is_empty() {
            if let Some(p) = items.last_mut() {
                p.text.push_str(&g.suffix);
                p.opt = true;
            }
        }
        let prefix = std::mem::take(&mut pending_prefix) + &g.prefix;
        let mut text = prefix;
        if !eof {
            text.push_str(&src[start..end]);
        }
        let mut item = Item {
            kind: t.kind.clone(),
            text,
            nl: g.nl_last,
            comments: g.comments,
            adjacent: g.spaces == 0 && g.nl_last == 0,
            opt: false,
            arrow: false,
        };
        item.adjacent = item.adjacent && item.comments.is_empty();
        // `->` is a `-` and a `>` the lexer reads apart.
        if matches!(item.kind, T::Gt) && item.adjacent {
            if let Some(p) = items.last_mut() {
                if matches!(p.kind, T::Minus) && p.text == "-" && !p.arrow {
                    p.text = "->".into();
                    p.arrow = true;
                    prev_end = end;
                    continue;
                }
            }
        }
        // `>>` reaches us as two `>`: the pipe, unless a generic closes
        // twice (layout decides).
        if matches!(item.kind, T::Gt) && item.adjacent {
            if let Some(p) = items.last_mut() {
                if matches!(p.kind, T::Gt) && p.text == ">" {
                    p.text = ">>".into();
                    p.kind = T::Pipe;
                    prev_end = end;
                    continue;
                }
            }
        }
        // A `.` + `r#word` gap holds the prefix; nothing else carries over.
        pending_prefix.clear();
        items.push(item);
        prev_end = end;
    }
    Ok(items)
}

#[derive(Clone, Copy, PartialEq)]
enum Open {
    Paren,
    Bracket,
    Brace,
    Angle,
}

struct Entry {
    open: Open,
    line: usize,
    counts: bool,
}

/// Format one file's text. The error is why the file cannot be formatted
/// (it does not lex); nothing is ever half-formatted.
pub fn format(src: &str) -> Result<String, Diagnostic> {
    let items = build(src)?;
    let out = layout(&items);
    verify(src, &out)?;
    Ok(out)
}

fn layout(items: &[Item]) -> String {
    // Braces that open and close on one line, and are not empty, get a space inside.
    let n = items.len();
    let mut inline = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let mut tight: Vec<(usize, bool)> = Vec::new();
    for (i, it) in items.iter().enumerate() {
        match it.kind {
            T::LBrace => {
                // A map literal is written tight (`{k: v}`), a block and a
                // record literal spaced (`{ snuff 1 }`, `Box { a: 1 }`): a
                // brace that follows an operator, an opener, a comma or a
                // word that takes a value opens a map.
                let map = i > 0
                    && matches!(
                        items[i - 1].kind,
                        T::Assign
                            | T::PlusAssign
                            | T::MinusAssign
                            | T::StarAssign
                            | T::SlashAssign
                            | T::LParen
                            | T::LBracket
                            | T::Comma
                            | T::Colon
                            | T::Snuff
                            | T::Exhale
                            | T::Cough
                            | T::Coalesce
                            | T::Plus
                            | T::Not
                            | T::In
                    );
                tight.push((i, map));
                stack.push(i)
            }
            T::RBrace => {
                if let Some(o) = stack.pop() {
                    let flat = items[o + 1..=i]
                        .iter()
                        .all(|x| x.nl == 0 && x.comments.is_empty());
                    let map = tight.iter().any(|&(k, m)| k == o && m);
                    if flat && i > o + 1 && !map {
                        inline[o] = true;
                        inline[i] = true;
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = String::new();
    let mut st: Vec<Entry> = Vec::new();
    let mut line = 0usize;
    let mut at_start = true;
    // The kind of the last code token, and whether it closed a generic.
    let mut prev: Option<usize> = None;
    let mut prev_generic_close = false;
    let mut prev_unary = false;
    let mut prev_generic_open = false;

    let levels = |st: &[Entry]| st.iter().filter(|e| e.counts).count();
    fn end_line(out: &mut String, line: &mut usize, blank: bool) {
        while out.ends_with(' ') {
            out.pop();
        }
        out.push('\n');
        if blank {
            out.push('\n');
        }
        *line += 1;
    }

    for (k, it) in items.iter().enumerate() {
        let closer = matches!(it.kind, T::RParen | T::RBracket | T::RBrace);
        let prev_open =
            prev.is_some_and(|p| matches!(items[p].kind, T::LBrace | T::LParen | T::LBracket));
        // Comments first.
        for (ci, (text, nl_before)) in it.comments.iter().enumerate() {
            let first = ci == 0;
            if first && *nl_before == 0 && !at_start && prev.is_some() {
                out.push(' ');
                out.push_str(text);
                end_line(&mut out, &mut line, false);
                at_start = true;
            } else {
                if !at_start {
                    end_line(&mut out, &mut line, false);
                }
                if *nl_before >= 2 && !out.is_empty() && !prev_open {
                    out.push('\n');
                    line += 1;
                }
                out.push_str(&" ".repeat(levels(&st) * INDENT));
                out.push_str(text);
                end_line(&mut out, &mut line, false);
                at_start = true;
            }
        }
        if matches!(it.kind, T::Eof) {
            break;
        }
        let had_comments = !it.comments.is_empty();
        let breaks = it.nl > 0 || had_comments;
        // Where this one goes.
        let mut generic_open = false;
        let mut generic_close = false;
        let mut double_close = false;
        let mut unary = false;
        match it.kind {
            T::Lt => {
                generic_open = !breaks
                    && it.adjacent
                    && prev.is_some_and(|p| matches!(items[p].kind, T::Ident(_)));
            }
            T::Gt => {
                generic_close = !it.arrow && st.last().is_some_and(|e| e.open == Open::Angle);
            }
            T::Pipe if it.text == ">>" => {
                let n = st.len();
                generic_close =
                    n >= 2 && st[n - 1].open == Open::Angle && st[n - 2].open == Open::Angle;
                double_close = generic_close;
            }
            T::Minus if !it.arrow => {
                unary = breaks
                    || prev.is_none()
                    || !prev.is_some_and(|p| value_end(&items[p].kind) || items[p].opt);
            }
            _ => {}
        }
        if breaks && !at_start {
            // A generic never spans a line.
            while st.last().is_some_and(|e| e.open == Open::Angle) {
                st.pop();
            }
            let blank = it.nl >= 2 && !prev_open && !closer;
            end_line(&mut out, &mut line, blank);
            at_start = true;
        } else if breaks && at_start && it.nl >= 2 && had_comments && !prev_open && !closer {
            out.push('\n');
            line += 1;
        }
        if closer {
            let want = match it.kind {
                T::RParen => Open::Paren,
                T::RBracket => Open::Bracket,
                _ => Open::Brace,
            };
            if st.iter().any(|e| e.open == want) {
                while let Some(e) = st.pop() {
                    if e.open == want {
                        break;
                    }
                }
            }
        }
        if at_start {
            let mut lv = levels(&st);
            let cont = !closer
                && (matches!(
                    it.kind,
                    T::Pipe | T::Dot | T::SafeDot | T::And | T::Or | T::Coalesce
                ) || prev.is_some_and(|p| binary_op(&items[p].kind) && !had_comments));
            if cont {
                lv += 1;
            }
            out.push_str(&" ".repeat(lv * INDENT));
        } else if let Some(p) = prev {
            let pi = &items[p];
            if space(
                pi,
                it,
                SpaceCtx {
                    prev_generic_close,
                    prev_unary,
                    prev_generic_open,
                    generic_open,
                    generic_close,
                    inline_before: matches!(pi.kind, T::LBrace) && inline[p],
                    inline_here: matches!(it.kind, T::RBrace) && inline[k],
                },
            ) {
                out.push(' ');
            }
        }
        out.push_str(&it.text);
        at_start = false;
        match it.kind {
            T::LParen | T::LBracket | T::LBrace => {
                let counts = !st.iter().any(|e| e.line == line && e.open != Open::Angle);
                st.push(Entry {
                    open: match it.kind {
                        T::LParen => Open::Paren,
                        T::LBracket => Open::Bracket,
                        _ => Open::Brace,
                    },
                    line,
                    counts,
                });
            }
            _ => {}
        }
        if generic_open {
            st.push(Entry {
                open: Open::Angle,
                line,
                counts: false,
            });
        }
        if generic_close {
            st.pop();
            if double_close {
                st.pop();
            }
        }
        prev = Some(k);
        prev_generic_close = generic_close;
        prev_unary = unary;
        prev_generic_open = generic_open;
    }
    while out.ends_with(' ') || out.ends_with('\n') {
        out.pop();
    }
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

struct SpaceCtx {
    prev_generic_close: bool,
    prev_unary: bool,
    prev_generic_open: bool,
    generic_open: bool,
    generic_close: bool,
    inline_before: bool,
    inline_here: bool,
}

fn space(prev: &Item, cur: &Item, c: SpaceCtx) -> bool {
    use T::*;
    match cur.kind {
        RParen | RBracket | Comma | Semicolon | Colon | Dot | SafeDot | DotDot => return false,
        RBrace => return c.inline_here,
        _ => {}
    }
    if c.generic_close || c.generic_open || c.prev_unary || c.prev_generic_open {
        return false;
    }
    // A unit written against its number (`2s`, `500ms`) stays against it.
    if matches!(prev.kind, Int(_) | Float(_)) && matches!(cur.kind, Ident(_)) && cur.adjacent {
        return false;
    }
    match prev.kind {
        LParen | LBracket | Dot | SafeDot | DotDot => return false,
        LBrace => return c.inline_before,
        _ => {}
    }
    match cur.kind {
        LBrace => !matches!(prev.kind, LParen | LBracket),
        LParen | LBracket => {
            let callable = matches!(prev.kind, Pack | RParen | RBracket)
                || c.prev_generic_close
                || (matches!(prev.kind, Ident(_)) && !prev.arrow);
            if !callable || prev.opt {
                return true;
            }
            if let Ident(w) = &prev.kind {
                if CONTEXTUAL.contains(&w.as_str()) {
                    return !cur.adjacent;
                }
            }
            false
        }
        _ => true,
    }
}

/// The tokens of a file, as a list of strings that ignores positions and
/// counts a run of newlines once.
fn signature(src: &str) -> Result<Vec<String>, Diagnostic> {
    let (_, tokens, opt, raw) = mask_raw(src)?;
    let mut out = Vec::with_capacity(tokens.len() + 2);
    for t in &tokens {
        let s = match &t.kind {
            T::Newline => {
                if out.last().is_some_and(|l: &String| l == "\n") {
                    continue;
                }
                "\n".to_string()
            }
            T::Str(parts) => {
                let mut s = String::from("str:");
                for p in parts {
                    match p {
                        StrPart::Lit(l) => s.push_str(&format!("L{l:?}")),
                        StrPart::Interp { src, .. } => s.push_str(&format!("I{src:?}")),
                    }
                }
                s
            }
            other => format!("{other:?}"),
        };
        out.push(s);
    }
    // Leading and trailing newlines are layout.
    while out.first().is_some_and(|f| f == "\n") {
        out.remove(0);
    }
    while out.len() > 1 && out[out.len() - 2] == "\n" {
        let l = out.len() - 2;
        out.remove(l);
    }
    out.push(format!("opt:{}", opt.len()));
    out.push(format!("raw:{}", raw.len()));
    Ok(out)
}

fn verify(src: &str, out: &str) -> Result<(), Diagnostic> {
    let a = signature(src)?;
    let b = signature(out).map_err(|_| {
        Diagnostic::new(
            DK::Syntax,
            "fmt: the result does not lex; the file is left as it is",
        )
    })?;
    if a != b {
        return Err(Diagnostic::new(
            DK::Syntax,
            "fmt: formatting would change the program's tokens; the file is left as it is",
        ));
    }
    Ok(())
}

/// The lines of `a` and `b` that differ, as `line: text`, for `--check`.
pub fn first_difference(a: &str, b: &str) -> Option<usize> {
    let mut la = a.split('\n');
    let mut lb = b.split('\n');
    let mut n = 0;
    loop {
        n += 1;
        match (la.next(), lb.next()) {
            (None, None) => return None,
            (x, y) if x == y => {}
            _ => return Some(n),
        }
    }
}

/// `cigar fmt [--check] PATH...`: the exit code. Files are read and written
/// whole; a file is only written when its text changes, and a file that
/// cannot be formatted is reported and left alone.
pub fn run(ctx: &cigscript::cli::Ctx, paths: &[std::path::PathBuf], check: bool) -> i32 {
    use crate::cli::{exit, files_of};
    use cigscript::diagnostics::usage;
    if paths.is_empty() {
        let d = usage("cigar fmt: name a file or a folder").code("E800");
        cigscript::cli::report(ctx, &d, None, None);
        return exit::USAGE;
    }
    let (mut changed, mut failed, mut bad_usage) = (false, false, false);
    for path in paths {
        let files = match files_of(path) {
            Ok(f) => f,
            Err(d) => {
                cigscript::cli::report(ctx, &d, None, None);
                bad_usage = true;
                continue;
            }
        };
        for file in files {
            if crate::check::Kind::from_path(&file).is_none() {
                let d =
                    usage(format!("{}: not a .cig or .cigar file", file.display())).code("E800");
                cigscript::cli::report(ctx, &d, None, None);
                bad_usage = true;
                continue;
            }
            let name = file.to_string_lossy().to_string();
            let text = match std::fs::read_to_string(&file) {
                Ok(t) => t,
                Err(e) => {
                    let d = usage(format!("cannot read {name}: {e}")).code("E801");
                    cigscript::cli::report(ctx, &d, None, None);
                    bad_usage = true;
                    continue;
                }
            };
            let out = match format(&text) {
                Ok(o) => o,
                Err(d) => {
                    cigscript::cli::report(ctx, &d, Some(&name), Some(&text));
                    failed = true;
                    continue;
                }
            };
            if out == text {
                continue;
            }
            changed = true;
            let line = first_difference(&text, &out).unwrap_or(1);
            if check {
                if ctx.json {
                    println!(
                        "{}",
                        serde_json::json!({"file": name, "would_change": true, "line": line})
                    );
                } else {
                    println!("would reformat {name} (first change at line {line})");
                }
            } else if let Err(e) = std::fs::write(&file, &out) {
                let d = usage(format!("cannot write {name}: {e}")).code("E801");
                cigscript::cli::report(ctx, &d, None, None);
                bad_usage = true;
            } else if !ctx.json {
                println!("formatted {name}");
            }
        }
    }
    if bad_usage {
        exit::USAGE
    } else if failed || (check && changed) {
        exit::CHECK
    } else {
        exit::OK
    }
}
