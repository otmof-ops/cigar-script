// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: Apache-2.0
//! Reading a `.cigar` file's `strength` declaration.
//!
//! The declaration is the first statement of the file: the identifier
//! `strength`, then one of `mild`, `medium` or `full`, both on the same
//! line. Comments and blank lines may come before it (the lexer skips
//! comments on its own; we skip leading newline tokens).
//!
//! Return the strength (mild, when there is none), plus the source with
//! the declaration replaced by spaces of the same length so cigscript's
//! spans and line numbers stay exact.

use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::lexer::tokenize;
use cigscript::syntax::span::Span;
use cigscript::syntax::token::{Token, TokenKind};

use crate::features::Strength;

/// The result of reading a `.cigar` file's head.
pub struct Declaration {
    pub strength: Strength,
    /// The source with the declaration blanked. For a file with no
    /// declaration this is the same bytes as the input.
    pub blanked: String,
    /// Any E1103 diagnostics raised by the scan (a declaration anywhere
    /// but at the top, a bad word after `strength`, or `strength` used as
    /// an ordinary name elsewhere in the file).
    pub errors: Vec<Diagnostic>,
}

/// Read a `.cigar` file's declaration.
#[allow(clippy::result_large_err)]
pub fn read(source: &str) -> Result<Declaration, Diagnostic> {
    let tokens = tokenize(source)?;
    Ok(scan(source, &tokens))
}

fn scan(source: &str, tokens: &[Token]) -> Declaration {
    let mut errors = Vec::new();

    // Find the first non-trivia token position. Newlines are the only
    // trivia the lexer still produces (comments it drops). A statement
    // separator is a Newline or a Semicolon; those between the start
    // and the first real token do not open another statement.
    let first = tokens
        .iter()
        .position(|t| !matches!(t.kind, TokenKind::Newline | TokenKind::Semicolon));

    let (strength, decl_span): (Strength, Option<Span>) = match first {
        None => (Strength::Mild, None),
        Some(i) => match &tokens[i].kind {
            TokenKind::Ident(name) if name == "strength" => {
                // Expect an identifier on the SAME line next.
                let next = tokens.get(i + 1);
                let word = match next {
                    Some(t) if t.span.line == tokens[i].span.line => match &t.kind {
                        TokenKind::Ident(w) => Some((w.clone(), t.span)),
                        _ => None,
                    },
                    _ => None,
                };
                match word {
                    Some((w, wspan)) => match Strength::parse(&w) {
                        Some(s) => {
                            let decl = tokens[i].span.to(wspan);
                            (s, Some(decl))
                        }
                        None => {
                            errors.push(e1103(
                                format!(
                                    "`strength` names no strength: `{w}` is not `mild`, `medium` or `full`",
                                ),
                                wspan,
                            ));
                            // Blank the `strength` token so a later pass
                            // does not flag it as a reserved-name use.
                            (Strength::Mild, Some(tokens[i].span))
                        }
                    },
                    None => {
                        errors.push(e1103(
                            "`strength` must be followed by `mild`, `medium` or `full` on the same line"
                                .to_string(),
                            tokens[i].span,
                        ));
                        (Strength::Mild, Some(tokens[i].span))
                    }
                }
            }
            _ => (Strength::Mild, None),
        },
    };

    // Any later use of `strength` is reserved: either as another
    // declaration (which can only be the first statement) or as an
    // ordinary name. Both are E1103.
    for (n, t) in tokens.iter().enumerate() {
        if let TokenKind::Ident(name) = &t.kind {
            if name == "strength" {
                let is_declaration = decl_span.is_some_and(|s| t.span.start == s.start);
                if is_declaration {
                    continue;
                }
                // Decide the message by whether this looks like a late
                // declaration (`strength` at a statement start) or a
                // reserved name used elsewhere.
                let at_statement_start = at_statement_start(tokens, n);
                let msg = if at_statement_start {
                    "`strength` declarations must be the first statement of the file".to_string()
                } else {
                    "`strength` is reserved in .cigar files; use another name".to_string()
                };
                errors.push(e1103(msg, t.span));
            }
        }
    }

    let blanked = match decl_span {
        Some(sp) => blank_range(source, sp),
        None => source.to_string(),
    };

    Declaration {
        strength,
        blanked,
        errors,
    }
}

/// Is the token at `i` the start of a statement? A statement starts at
/// the beginning of the file, or after a Newline or Semicolon token.
fn at_statement_start(tokens: &[Token], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    matches!(
        tokens[i - 1].kind,
        TokenKind::Newline | TokenKind::Semicolon
    )
}

/// Replace the bytes of `span` with spaces, so line counts and byte
/// positions stay exact for everything else cigscript reads.
fn blank_range(source: &str, span: Span) -> String {
    let bytes = source.as_bytes();
    let start = span.start.min(bytes.len());
    let end = span.end.min(bytes.len()).max(start);
    let mut out = Vec::with_capacity(bytes.len());
    out.extend_from_slice(&bytes[..start]);
    for &b in &bytes[start..end] {
        // Keep newlines so lines do not collapse. Any other byte is
        // replaced by a space (UTF-8 continuation bytes included: this
        // runs over ASCII keywords only, so continuation bytes do not
        // appear here).
        if b == b'\n' {
            out.push(b'\n');
        } else {
            out.push(b' ');
        }
    }
    out.extend_from_slice(&bytes[end..]);
    // The input was valid UTF-8 and we only replaced ASCII bytes with
    // another ASCII byte, so this stays valid.
    String::from_utf8(out).expect("blanking kept UTF-8 valid")
}

fn e1103(msg: String, span: Span) -> Diagnostic {
    let mut d = Diagnostic::new(Kind::Check, msg).at(span);
    // Direct set bypasses the registry debug_assert: the CigarScript
    // codes E1101-E1103 live in cigscript's reserved E1100-E1199 range
    // but have no row in the catalogue yet (they land there when the
    // first strength ships).
    d.code = "E1103";
    d
}
