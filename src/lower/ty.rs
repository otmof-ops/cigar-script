// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Type expressions (`List<Map<Int>>?`, `(Int, Text) -> Bool`) read from
//! cig's tokens, and the run-time descriptors a boundary check uses.

use crate::types::{SizedInt, Ty};
use cigscript::syntax::token::{Token, TokenKind};
use std::collections::BTreeSet;

/// What a type name can be beyond the built-ins.
pub trait Names {
    fn is_record(&self, name: &str) -> bool;
    fn is_enum(&self, name: &str) -> bool;
}

/// Read one type starting at token `i`. `opt` holds the byte offsets of the
/// lone `?` characters masked out of the source (cig's lexer has no token
/// for them): a `?` right after a type makes it optional. `params` are the
/// type parameters in scope. Returns the type and the index after it, or
/// a message.
pub fn parse(
    toks: &[Token],
    mut i: usize,
    src: &str,
    opt: &BTreeSet<usize>,
    params: &[String],
    names: &dyn Names,
) -> Result<(Ty, usize), (usize, String)> {
    let start = i;
    let base = match &toks[i].kind {
        TokenKind::LParen => {
            // (A, B) -> R
            i += 1;
            let mut ps = Vec::new();
            if !matches!(toks[i].kind, TokenKind::RParen) {
                loop {
                    let (t, n) = parse(toks, i, src, opt, params, names)?;
                    ps.push(t);
                    i = n;
                    if matches!(toks[i].kind, TokenKind::Comma) {
                        i += 1;
                        continue;
                    }
                    break;
                }
            }
            if !matches!(toks[i].kind, TokenKind::RParen) {
                return Err((i, "`)` to close a function type".into()));
            }
            i += 1;
            if !(matches!(toks[i].kind, TokenKind::Minus)
                && matches!(toks[i + 1].kind, TokenKind::Gt))
            {
                return Err((i, "`->` and a result type".into()));
            }
            i += 2;
            let (r, n) = parse(toks, i, src, opt, params, names)?;
            i = n;
            Ty::Fn(ps, Box::new(r))
        }
        TokenKind::Null => {
            i += 1;
            Ty::Null
        }
        TokenKind::Ident(name) => {
            let name = name.clone();
            i += 1;
            let mut args = Vec::new();
            if matches!(toks[i].kind, TokenKind::Lt) {
                i += 1;
                loop {
                    let (t, n) = parse(toks, i, src, opt, params, names)?;
                    args.push(t);
                    i = n;
                    if matches!(toks[i].kind, TokenKind::Comma) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                if !matches!(toks[i].kind, TokenKind::Gt) {
                    return Err((i, "`>` to close the type arguments".into()));
                }
                i += 1;
            }
            named(&name, args, params, names).map_err(|m| (start, m))?
        }
        _ => return Err((i, "a type".into())),
    };
    // A masked `?` right after the type (spaces only between).
    let end = toks[i - 1].span.end;
    let mut t = base;
    let mut at = end;
    while let Some(&q) = opt.range(at..).next() {
        if src[at..q].chars().all(|c| c == ' ' || c == '\t') {
            t = match t {
                Ty::Opt(_) => t,
                other => Ty::Opt(Box::new(other)),
            };
            at = q + 1;
        } else {
            break;
        }
    }
    Ok((t, i))
}

fn named(name: &str, args: Vec<Ty>, params: &[String], names: &dyn Names) -> Result<Ty, String> {
    let arity = |n: usize| -> Result<(), String> {
        if args.len() == n {
            Ok(())
        } else {
            Err(format!(
                "E1203 `{name}` takes {n} type argument{}, and {} {} given",
                if n == 1 { "" } else { "s" },
                args.len(),
                if args.len() == 1 { "is" } else { "are" }
            ))
        }
    };
    if params.iter().any(|p| p == name) {
        arity(0)?;
        return Ok(Ty::Param(name.to_string()));
    }
    if let Some((_, s)) = SizedInt::ALL.iter().find(|(n, _)| *n == name) {
        arity(0)?;
        return Ok(Ty::Sized(*s));
    }
    Ok(match name {
        "Any" => {
            arity(0)?;
            Ty::Any
        }
        "Null" => {
            arity(0)?;
            Ty::Null
        }
        "Bool" => {
            arity(0)?;
            Ty::Bool
        }
        "Int" => {
            arity(0)?;
            Ty::Int
        }
        "Float" => {
            arity(0)?;
            Ty::Float
        }
        "Text" => {
            arity(0)?;
            Ty::Text
        }
        "Path" => {
            arity(0)?;
            Ty::Path
        }
        "Bytes" => {
            arity(0)?;
            Ty::Bytes
        }
        // `List` and `Map` alone hold anything: `List<Any>`, `Map<Any>`.
        "List" => match args.len() {
            0 => Ty::List(Box::new(Ty::Any)),
            _ => {
                arity(1)?;
                Ty::List(Box::new(args.into_iter().next().unwrap()))
            }
        },
        "ListView" => {
            arity(1)?;
            Ty::ListView(Box::new(args.into_iter().next().unwrap()))
        }
        "Map" => match args.len() {
            0 => Ty::Map(Box::new(Ty::Any)),
            1 => Ty::Map(Box::new(args.into_iter().next().unwrap())),
            2 => {
                let mut it = args.into_iter();
                let k = it.next().unwrap();
                let v = it.next().unwrap();
                // Text keys are cig's own; any other key goes through its
                // `hash()` (#38).
                if matches!(k, Ty::Text | Ty::Path | Ty::Any) {
                    Ty::Map(Box::new(v))
                } else {
                    Ty::KeyMap(Box::new(k), Box::new(v))
                }
            }
            _ => return Err("E1203 `Map` takes `Map<V>` or `Map<K, V>`".into()),
        },
        "Set" => {
            arity(1)?;
            Ty::Set(Box::new(args.into_iter().next().unwrap()))
        }
        "Channel" => {
            arity(1)?;
            Ty::Channel(Box::new(args.into_iter().next().unwrap()))
        }
        n if names.is_record(n) => Ty::Record(n.to_string(), args),
        n if names.is_enum(n) => Ty::Enum(n.to_string(), args),
        n => {
            return Err(format!(
                "E1202 `{n}` is not a type: no record, enum or type parameter has that name"
            ))
        }
    })
}

/// The run-time descriptor of a type, as CigScript map literal text.
pub fn descriptor(t: &Ty) -> String {
    match t {
        // A map with keys that are not text, and a set, are held in cig maps
        // keyed by `hash()`: not checked by shape.
        Ty::Any
        | Ty::Param(_)
        | Ty::Never
        | Ty::Bytes
        | Ty::Channel(_)
        | Ty::KeyMap(..)
        | Ty::Set(_) => "{k: \"any\"}".into(),
        Ty::Null => "{k: \"null\"}".into(),
        Ty::Bool => "{k: \"bool\"}".into(),
        Ty::Int => "{k: \"int\"}".into(),
        Ty::Float => "{k: \"float\"}".into(),
        Ty::Text | Ty::Path => "{k: \"text\"}".into(),
        Ty::Sized(s) => {
            let (lo, hi) = s.range();
            format!(
                "{{k: \"sized\", lo: {lo}, hi: {hi}, name: \"{}\"}}",
                s.name()
            )
        }
        Ty::List(e) | Ty::ListView(e) => format!("{{k: \"list\", of: {}}}", descriptor(e)),
        Ty::Map(v) => format!("{{k: \"map\", of: {}}}", descriptor(v)),
        Ty::Opt(v) => format!("{{k: \"opt\", of: {}}}", descriptor(v)),
        Ty::Fn(..) => "{k: \"fn\"}".into(),
        Ty::Record(n, _) => format!("{{k: \"record\", name: \"{n}\"}}"),
        Ty::Enum(n, _) => format!("{{k: \"enum\", name: \"{n}\"}}"),
    }
}

/// Does a descriptor check anything at all (`Any` needs no call)?
pub fn checks(t: &Ty) -> bool {
    !matches!(
        t,
        Ty::Any
            | Ty::Param(_)
            | Ty::Never
            | Ty::Bytes
            | Ty::Channel(_)
            | Ty::KeyMap(..)
            | Ty::Set(_)
    )
}
