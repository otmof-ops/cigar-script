// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The feature gate.
//!
//! Given the blanked source of a `.cigar` file and the effective strength,
//! find every use of a feature (a reserved word or a type annotation) and
//! raise E1101 for a feature above the strength, E1102 for a feature at
//! or below the strength whose row says it is not built yet. Findings
//! are sorted by position.

use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::span::Span;
use cigscript::syntax::token::{Token, TokenKind};

use crate::features::{Detect, Feature, Strength, FEATURES};

/// What the gate found: the errors that stop the check, and the warnings
/// that ride along with a file that goes on.
pub struct Scan {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// Scan a blanked `.cigar` source at the given effective strength.
#[allow(clippy::result_large_err)]
pub fn scan(source: &str, effective: Strength) -> Result<Vec<Diagnostic>, Diagnostic> {
    scan_all(source, effective).map(|s| s.errors)
}

/// Scan, keeping the warnings (W1234) as well as the errors.
#[allow(clippy::result_large_err)]
pub fn scan_all(source: &str, effective: Strength) -> Result<Scan, Diagnostic> {
    let (_, tokens, _, raw) = crate::lower::mask_raw(source)?;
    let names = name_only(&tokens);
    let mut findings: Vec<(Span, Diagnostic)> = Vec::new();
    // Mild is CigScript as it is, where `#` starts a comment: no raw names.
    if effective < Strength::Medium {
        for &at in &raw {
            let line = source[..at].matches('\n').count() + 1;
            let col = source[..at]
                .rsplit('\n')
                .next()
                .map_or(0, |l| l.chars().count())
                + 1;
            let span = Span::new(at, at + 2, line as u32, col as u32);
            let mut d = Diagnostic::new(
                Kind::Check,
                format!("using raw identifiers (`r#name`) needs strength medium; this file is {effective}"),
            )
            .at(span)
            .with_hint("a name that is not a keyword in a mild file needs no escape".to_string());
            d.code = "E1101";
            findings.push((span, d));
        }
    }
    let mut notes: Vec<(Span, Diagnostic)> = Vec::new();
    // `r#word` is read as the name `r_word`: a file that also writes `r_word`
    // would have the two be one variable, so it is refused.
    let mangled: Vec<&str> = tokens
        .iter()
        .filter(|t| raw.binary_search(&t.span.start).is_ok())
        .filter_map(|t| match &t.kind {
            TokenKind::Ident(n) if n.starts_with("r_") => Some(n.as_str()),
            _ => None,
        })
        .collect();
    for t in &tokens {
        if let TokenKind::Ident(n) = &t.kind {
            if raw.binary_search(&t.span.start).is_err() && mangled.contains(&n.as_str()) {
                let word = &n[2..];
                let mut d = Diagnostic::new(
                    Kind::Check,
                    format!(
                        "`r#{word}` and `{n}` are the same name in a `.cigar` file; rename one"
                    ),
                )
                .at(t.span);
                d.code = "E1105";
                findings.push((t.span, d));
            }
        }
    }
    for f in FEATURES {
        match f.detect {
            Detect::Word(w) => {
                collect_word(&tokens, &names, w, f, effective, &mut findings, &mut notes)
            }
            Detect::Annotation => collect_annotations(&tokens, f, effective, &mut findings),
        }
    }
    findings.sort_by_key(|(s, _)| (s.start, s.end));
    notes.sort_by_key(|(s, _)| (s.start, s.end));
    Ok(Scan {
        errors: findings.into_iter().map(|(_, d)| d).collect(),
        warnings: notes.into_iter().map(|(_, d)| d).collect(),
    })
}

/// For each token, is it a name by where it stands, whatever it is spelled:
/// the word after a `.` or `?.` (a field or a method), or a key in a map or
/// record literal (`{ word: value }`)? A reserved word there is data, such
/// as a field of parsed JSON, and no file can rename that (#22 D1).
pub fn name_only(tokens: &[Token]) -> Vec<bool> {
    let mut out = vec![false; tokens.len()];
    // The innermost open bracket at each point: only a brace holds keys.
    let mut open: Vec<&TokenKind> = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        match &t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => open.push(&t.kind),
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                open.pop();
            }
            TokenKind::Ident(_) => {
                let after_dot =
                    i > 0 && matches!(tokens[i - 1].kind, TokenKind::Dot | TokenKind::SafeDot);
                let key = matches!(open.last(), Some(TokenKind::LBrace))
                    && i > 0
                    && matches!(
                        tokens[i - 1].kind,
                        TokenKind::LBrace | TokenKind::Comma | TokenKind::Newline
                    )
                    && matches!(tokens.get(i + 1).map(|n| &n.kind), Some(TokenKind::Colon));
                out[i] = after_dot || key;
            }
            _ => {}
        }
    }
    out
}

fn kind_at(tokens: &[Token], i: usize) -> Option<&TokenKind> {
    tokens.get(i).map(|t| &t.kind)
}

fn is_operand_start(k: Option<&TokenKind>) -> bool {
    matches!(
        k,
        Some(
            TokenKind::Ident(_)
                | TokenKind::Int(_)
                | TokenKind::Float(_)
                | TokenKind::Str(_)
                | TokenKind::True
                | TokenKind::False
                | TokenKind::Null
        )
    )
}

fn is_operand_end(k: Option<&TokenKind>) -> bool {
    is_operand_start(k)
        || matches!(
            k,
            Some(TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace)
        )
}

/// The index of the bracket that closes the one at `open`.
fn closing(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (j, t) in tokens.iter().enumerate().skip(open) {
        match t.kind {
            TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace => depth += 1,
            TokenKind::RParen | TokenKind::RBracket | TokenKind::RBrace => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(j);
                }
            }
            _ => {}
        }
    }
    None
}

/// Is the word at `i` written the way its feature's syntax begins? A word
/// is a keyword only there (#22 D1); anywhere else it is a name.
///
/// `lenient` settles the shapes that read both ways (`measure(x)`: the word
/// then a call, or the word then an operand in parentheses): in a file whose
/// strength reserves the word they are the keyword, below it a name.
fn keyword_shape(tokens: &[Token], i: usize, word: &str, lenient: bool) -> bool {
    let prev = i.checked_sub(1).and_then(|p| kind_at(tokens, p));
    let next = kind_at(tokens, i + 1);
    let next_ident = matches!(next, Some(TokenKind::Ident(_)));
    // The word starts a statement or a value: after a separator, `=`, `snuff`.
    let at_value_start = matches!(
        prev,
        None | Some(
            TokenKind::Newline
                | TokenKind::Semicolon
                | TokenKind::LBrace
                | TokenKind::RBrace
                | TokenKind::Assign
                | TokenKind::Snuff
                | TokenKind::Exhale
                | TokenKind::FatArrow
        )
    );
    let opener = matches!(
        next,
        Some(TokenKind::LParen | TokenKind::LBracket | TokenKind::LBrace | TokenKind::Minus)
    );
    match word {
        // Two names side by side are never CigScript: the word is the keyword.
        "record" | "enum" | "interface" | "use" | "layer" | "within" | "coughs" | "owned"
        | "borrow" => next_ident,
        // `impl Show for T`, or `impl<T: Show> Show for List<T>` (#38).
        "impl" => next_ident || matches!(next, Some(TokenKind::Lt)),
        // `move x` gives `x` away; `move pack ...` is a function value that
        // owns what it captures (#22 S6).
        "move" => next_ident || matches!(next, Some(TokenKind::Pack)),
        "pub" => {
            next_ident
                || matches!(
                    next,
                    Some(TokenKind::Stick | TokenKind::Roll | TokenKind::Pull)
                )
        }
        "foreign" => next_ident || matches!(next, Some(TokenKind::Str(_))),
        "unchecked" => matches!(next, Some(TokenKind::LBrace)),
        "parallel" => {
            matches!(next, Some(TokenKind::LParen))
                && closing(tokens, i + 1).is_some_and(|c| {
                    matches!(
                        kind_at(tokens, c + 1),
                        Some(TokenKind::For | TokenKind::LBrace)
                    )
                })
        }
        "channel" => {
            if !matches!(next, Some(TokenKind::Lt)) {
                return false;
            }
            // `channel<T>(`: the type arguments, then a call.
            let mut depth = 0usize;
            for (j, t) in tokens.iter().enumerate().skip(i + 1).take(40) {
                match t.kind {
                    TokenKind::Lt => depth += 1,
                    TokenKind::Gt => {
                        depth -= 1;
                        if depth == 0 {
                            return matches!(kind_at(tokens, j + 1), Some(TokenKind::LParen));
                        }
                    }
                    TokenKind::Ident(_) | TokenKind::Comma => {}
                    _ => return false,
                }
            }
            false
        }
        "match" => {
            is_operand_start(next)
                || matches!(next, Some(TokenKind::Not))
                || (opener && lenient && at_value_start)
        }
        "measure" => {
            matches!(next, Some(TokenKind::For))
                || is_operand_start(next)
                || (opener && lenient && !matches!(next, Some(TokenKind::LBrace)))
        }
        "into" => {
            matches!(next, Some(TokenKind::For))
                || (is_operand_end(prev) && (is_operand_start(next) || opener))
        }
        _ => true,
    }
}

fn collect_word(
    tokens: &[Token],
    names: &[bool],
    word: &str,
    f: &Feature,
    effective: Strength,
    out: &mut Vec<(Span, Diagnostic)>,
    notes: &mut Vec<(Span, Diagnostic)>,
) {
    let lenient = f.strength <= effective;
    for (i, t) in tokens.iter().enumerate() {
        let TokenKind::Ident(name) = &t.kind else {
            continue;
        };
        if name != word || names[i] {
            continue;
        }
        if keyword_shape(tokens, i, word, lenient) {
            if word == "move" && effective == Strength::Medium && is_lent_argument(tokens, i) {
                // `f(move x)`: a medium file gives a value to a full
                // function's `owned` parameter (#22 D3). The ownership
                // check says whether `f` takes it `owned`.
                continue;
            }
            push_finding(out, f, effective, t.span);
        } else if lenient {
            out.push((t.span, e1105(word, t.span)));
        } else {
            notes.push((t.span, w1234(word, f.strength, t.span)));
        }
    }
}

/// Is the `move` at `i` a call's argument, `(move x` or `, move x` followed
/// by `)` or `,`?
fn is_lent_argument(tokens: &[Token], i: usize) -> bool {
    let prev = i.checked_sub(1).and_then(|p| kind_at(tokens, p));
    matches!(prev, Some(TokenKind::LParen | TokenKind::Comma))
        && matches!(kind_at(tokens, i + 1), Some(TokenKind::Ident(_)))
        && matches!(
            kind_at(tokens, i + 2),
            Some(TokenKind::RParen | TokenKind::Comma)
        )
}

/// L9: a reserved word where a name goes says so, and how to write it.
pub fn reserved_message(word: &str) -> String {
    format!("`{word}` is a reserved word in `.cigar` files; rename it, or write `r#{word}`")
}

fn e1105(word: &str, span: Span) -> Diagnostic {
    let mut d = Diagnostic::new(Kind::Check, reserved_message(word))
        .at(span)
        .with_hint(format!(
            "a field or a map key may use it as it is (`x.{word}`, `{{{word}: 1}}`)"
        ));
    // Direct set, as E1101 to E1103 do: the row joins the catalogue with them.
    d.code = "E1105";
    d
}

fn w1234(word: &str, strength: Strength, span: Span) -> Diagnostic {
    let mut d = Diagnostic::new(
        Kind::Check,
        format!("`{word}` will be a keyword if this file goes to {strength}"),
    )
    .at(span)
    .with_hint(format!(
        "rename it now, or write `r#{word}` once the file is at {strength}"
    ));
    d.code = "W1234";
    d
}

fn collect_annotations(
    tokens: &[Token],
    f: &Feature,
    effective: Strength,
    out: &mut Vec<(Span, Diagnostic)>,
) {
    let mut i = 0;
    while i < tokens.len() {
        match &tokens[i].kind {
            // `stick NAME :` and `roll NAME :`
            TokenKind::Stick | TokenKind::Roll => {
                if let (Some(a), Some(b)) = (tokens.get(i + 1), tokens.get(i + 2)) {
                    if matches!(a.kind, TokenKind::Ident(_)) && matches!(b.kind, TokenKind::Colon) {
                        push_finding(out, f, effective, b.span);
                    }
                }
                i += 1;
            }
            // `pull NAME ( ... ) [ -> ... ]`
            TokenKind::Pull => {
                // Find the `(` after `pull NAME`.
                if let (Some(a), Some(b)) = (tokens.get(i + 1), tokens.get(i + 2)) {
                    if matches!(a.kind, TokenKind::Ident(_)) && matches!(b.kind, TokenKind::LParen)
                    {
                        // Walk the parameter list, flag `Ident Colon` at
                        // the parameter level only: not inside a nested
                        // paren, or a map or list a default value writes
                        // (`pull f(a = {x: 1})` annotates nothing).
                        let mut depth = 1usize;
                        let mut nested = 0usize;
                        let mut j = i + 3;
                        while j < tokens.len() && depth > 0 {
                            match &tokens[j].kind {
                                TokenKind::LParen => depth += 1,
                                TokenKind::RParen => depth -= 1,
                                TokenKind::LBrace | TokenKind::LBracket => nested += 1,
                                TokenKind::RBrace | TokenKind::RBracket => {
                                    nested = nested.saturating_sub(1)
                                }
                                TokenKind::Ident(_) if depth == 1 && nested == 0 => {
                                    if let Some(next) = tokens.get(j + 1) {
                                        if matches!(next.kind, TokenKind::Colon) {
                                            push_finding(out, f, effective, next.span);
                                        }
                                    }
                                }
                                _ => {}
                            }
                            if depth == 0 {
                                break;
                            }
                            j += 1;
                        }
                        // `j` points at the closing `)`. Check for `->`
                        // right after it (optionally with newlines being
                        // stripped: cigscript's lexer does not drop them,
                        // so we only check contiguous tokens).
                        if depth == 0 {
                            if let (Some(a), Some(b)) = (tokens.get(j + 1), tokens.get(j + 2)) {
                                if matches!(a.kind, TokenKind::Minus)
                                    && matches!(b.kind, TokenKind::Gt)
                                {
                                    let arrow = a.span.to(b.span);
                                    push_finding(out, f, effective, arrow);
                                }
                            }
                            i = j + 1;
                            continue;
                        }
                    }
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
}

fn push_finding(out: &mut Vec<(Span, Diagnostic)>, f: &Feature, effective: Strength, span: Span) {
    if f.strength > effective {
        out.push((span, e1101(f, effective, span)));
    } else if !f.built {
        out.push((span, e1102(f, span)));
    }
}

fn e1101(f: &Feature, effective: Strength, span: Span) -> Diagnostic {
    let msg = format!(
        "using {} needs strength {}; this file is {}",
        f.name, f.strength, effective
    );
    let hint = format!(
        "to use it, put `strength {}` at the top of the file",
        f.strength
    );
    let mut d = Diagnostic::new(Kind::Check, msg).at(span).with_hint(hint);
    d.code = "E1101";
    d
}

fn e1102(f: &Feature, span: Span) -> Diagnostic {
    let msg = format!(
        "using {} is {}-strength CigarScript, not built yet in this version (otmof-ops/cigar-script{})",
        f.name, f.strength, f.issue
    );
    let mut d = Diagnostic::new(Kind::Check, msg).at(span);
    d.code = "E1102";
    d
}
