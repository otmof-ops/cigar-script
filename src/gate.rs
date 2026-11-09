//! The feature gate.
//!
//! Given the blanked source of a `.cigar` file and the effective strength,
//! find every use of a feature (a reserved word or a type annotation) and
//! raise E1101 for a feature above the strength, E1102 for a feature at
//! or below the strength whose row says it is not built yet. Findings
//! are sorted by position.

use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::lexer::tokenize;
use cigscript::syntax::span::Span;
use cigscript::syntax::token::{Token, TokenKind};

use crate::features::{Detect, Feature, Strength, FEATURES};

/// Scan a blanked `.cigar` source at the given effective strength.
#[allow(clippy::result_large_err)]
pub fn scan(source: &str, effective: Strength) -> Result<Vec<Diagnostic>, Diagnostic> {
    let tokens = tokenize(source)?;
    let mut findings: Vec<(Span, Diagnostic)> = Vec::new();
    for f in FEATURES {
        match f.detect {
            Detect::Word(w) => collect_word(&tokens, w, f, effective, &mut findings),
            Detect::Annotation => collect_annotations(&tokens, f, effective, &mut findings),
        }
    }
    findings.sort_by_key(|(s, _)| (s.start, s.end));
    Ok(findings.into_iter().map(|(_, d)| d).collect())
}

fn collect_word(
    tokens: &[Token],
    word: &str,
    f: &Feature,
    effective: Strength,
    out: &mut Vec<(Span, Diagnostic)>,
) {
    for t in tokens {
        if let TokenKind::Ident(name) = &t.kind {
            if name == word {
                push_finding(out, f, effective, t.span);
            }
        }
    }
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
