// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Operators on user types (#38, section 2): an operator whose left side is
//! a record or an enum, or a bounded type parameter, is the method of its
//! standard interface.
//!
//! | operator | interface | becomes |
//! |---|---|---|
//! | `a + b` (`-`, `*`, `/`, `%`) | `Add` (`Sub`, `Mul`, `Div`, `Rem`) | `add(a, b)` |
//! | `-a` | `Neg` | `neg(a)` |
//! | `a < b` (`<=`, `>`, `>=`) | `Ord` | `cmp(a, b) < 0` |
//! | `a == b`, `a != b` | `Eq` | `eq(a, b)`, `(not eq(a, b))` |
//! | `a[k]` | `Index` | `index(a, k)` |
//!
//! A record with no `impl` of the interface is `E1230` at the operator,
//! except `==` and `!=`, which compare a record field by field as they always
//! have when it has no `impl Eq`. On the built-in types the operators stay
//! CigScript's, so a `.cig` file means what it meant. A bounded `T` uses its
//! bound, picked by the value when the program runs; an unbounded `T` is as
//! before.

use super::{Ck, Resolution};
use crate::types::{self, Ty};
use cigscript::syntax::ast::{BinOp, Expr};
use cigscript::syntax::span::Span;

/// The interface and method an operator stands for, and what follows the
/// call (`< 0` for `<`).
fn of(op: BinOp) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    Some(match op {
        BinOp::Add => ("Add", "add", "", ""),
        BinOp::Sub => ("Sub", "sub", "", ""),
        BinOp::Mul => ("Mul", "mul", "", ""),
        BinOp::Div => ("Div", "div", "", ""),
        BinOp::Rem => ("Rem", "rem", "", ""),
        BinOp::Lt => ("Ord", "cmp", "", " < 0"),
        BinOp::Le => ("Ord", "cmp", "", " <= 0"),
        BinOp::Gt => ("Ord", "cmp", "", " > 0"),
        BinOp::Ge => ("Ord", "cmp", "", " >= 0"),
        BinOp::Eq => ("Eq", "eq", "", ""),
        BinOp::Ne => ("Eq", "eq", "(not ", ")"),
        _ => return None,
    })
}

/// Is `t` a type whose operators are its interfaces' methods?
fn user_type(t: &Ty) -> bool {
    matches!(t, Ty::Record(..) | Ty::Enum(..) | Ty::Param(_))
}

impl Ck<'_> {
    /// The function an operator on a value of type `t` calls for `method`
    /// of `iface`: the `impl`'s, or for a bounded `T` the dispatcher. `Err`
    /// with what to say when `t` has none (`None`: say nothing, the operator
    /// keeps its CigScript meaning).
    fn operator_fn(&self, iface: &str, method: &str, t: &Ty) -> Result<String, Option<String>> {
        match t {
            Ty::Param(p) => match self.bounds_of(p) {
                Some(bs) if bs.iter().any(|b| b == iface) => {
                    Ok(format!("_cg_dispatch_{iface}_{method}"))
                }
                Some(bs) => Err(Some(format!(
                    "add the bound: `{p}: {} + {iface}`",
                    bs.join(" + ")
                ))),
                None => Err(None),
            },
            _ => match self.select(iface, t) {
                Ok((im, _)) => Ok(types::impl_fn_name(iface, &im.for_ty, method)),
                Err(_) if iface == "Eq" => Err(None),
                Err(_) => Err(Some(format!(
                    "write `impl {iface} for {t} {{ ... }}`{}",
                    if matches!(iface, "Eq" | "Ord") {
                        format!(" or `derive({iface})` on `{t}`")
                    } else {
                        String::new()
                    }
                ))),
            },
        }
    }

    /// The text between two operands in the file as written, with the
    /// operator turned into `,` and its line breaks kept.
    fn gap(&self, from: usize, to: usize) -> String {
        let (a, b) = ((self.original)(from), (self.original)(to));
        let lines = self
            .source
            .get(a..b.max(a))
            .unwrap_or("")
            .matches('\n')
            .count();
        format!(", {}", "\n".repeat(lines))
    }

    /// `lhs op rhs`, where `l` is a record, an enum or a type parameter: the
    /// type of the result when the operator is the interface's method (or
    /// refused), `None` when it keeps its CigScript meaning.
    pub(super) fn user_operator(
        &mut self,
        op: BinOp,
        lhs: &Expr,
        rhs: &Expr,
        l: &Ty,
        r: &Ty,
    ) -> Option<Ty> {
        let (iface, method, pre, post) = of(op)?;
        if !user_type(l) || (iface == "Eq" && matches!(r, Ty::Null | Ty::Opt(_))) {
            return None;
        }
        let span = Span::new(lhs.span.start, rhs.span.end, lhs.span.line, lhs.span.col);
        let symbol = op.symbol();
        let f = match self.operator_fn(iface, method, l) {
            Ok(f) => f,
            Err(None) => return None,
            Err(Some(hint)) => {
                self.err(
                    "E1230",
                    format!("`{l}` doesn't implement `{iface}`, which `{symbol}` needs"),
                    span,
                    Some(hint),
                );
                return Some(Ty::Any);
            }
        };
        if self.interpolating > 0 {
            self.not_in_interpolation(symbol, l, span);
            return Some(Ty::Any);
        }
        self.expect(
            rhs,
            r,
            l,
            "E1201",
            format!("`{symbol}` on `{l}` takes another `{l}`"),
        );
        self.resolutions.push(super::Resolution::Wrap {
            start: lhs.span.start,
            end: rhs.span.end,
            pre: format!("{pre}{f}("),
            post: format!("){post}"),
        });
        let gap = self.gap(lhs.span.end, rhs.span.start);
        self.resolutions.push(Resolution::Rename {
            start: lhs.span.end,
            end: rhs.span.start,
            text: gap,
        });
        Some(if iface == "Ord" || iface == "Eq" {
            Ty::Bool
        } else {
            l.clone()
        })
    }

    /// `-x` on a record, an enum or a bounded `T`: `Neg`.
    pub(super) fn user_negation(&mut self, e: &Expr, x: &Expr, t: &Ty) -> Option<Ty> {
        if !user_type(t) {
            return None;
        }
        let f = match self.operator_fn("Neg", "neg", t) {
            Ok(f) => f,
            Err(None) => return None,
            Err(Some(hint)) => {
                self.err(
                    "E1230",
                    format!("`{t}` doesn't implement `Neg`, which `-` needs"),
                    e.span,
                    Some(hint),
                );
                return Some(Ty::Any);
            }
        };
        if self.interpolating > 0 {
            self.not_in_interpolation("-", t, e.span);
            return Some(Ty::Any);
        }
        self.resolutions.push(Resolution::Rename {
            start: e.span.start,
            end: x.span.start,
            text: format!("{f}("),
        });
        self.resolutions.push(Resolution::Wrap {
            start: x.span.start,
            end: x.span.end,
            pre: String::new(),
            post: ")".into(),
        });
        Some(t.clone())
    }

    /// `x[k]` on a record, an enum or a bounded `T`: `Index`.
    pub(super) fn user_index(
        &mut self,
        e: &Expr,
        object: &Expr,
        index: &Expr,
        t: &Ty,
    ) -> Option<Ty> {
        if !user_type(t) {
            return None;
        }
        let f = match self.operator_fn("Index", "index", t) {
            Ok(f) => f,
            Err(None) => return None,
            Err(Some(hint)) => {
                self.err(
                    "E1230",
                    format!("`{t}` doesn't implement `Index`, which `[...]` needs"),
                    e.span,
                    Some(hint),
                );
                return Some(Ty::Any);
            }
        };
        if self.interpolating > 0 {
            self.not_in_interpolation("[...]", t, e.span);
            return Some(Ty::Any);
        }
        self.resolutions.push(Resolution::Wrap {
            start: object.span.start,
            end: object.span.end,
            pre: format!("{f}("),
            post: String::new(),
        });
        let open = self.gap(object.span.end, index.span.start);
        self.resolutions.push(Resolution::Rename {
            start: object.span.end,
            end: index.span.start,
            text: open,
        });
        self.resolutions.push(Resolution::Rename {
            start: index.span.end,
            end: e.span.end,
            text: ")".into(),
        });
        Some(Ty::Any)
    }

    /// An operator that would be a method call, inside `${...}`: not there
    /// yet, since the interpolation is read on its own.
    fn not_in_interpolation(&mut self, symbol: &str, t: &Ty, span: Span) {
        self.err(
            "E1102",
            format!(
                "`{symbol}` on `{t}` inside `${{...}}` is not built yet (otmof-ops/cigar-script#38)"
            ),
            span,
            Some("work it out in a `stick` first, and put the name in the text".into()),
        );
    }
}
