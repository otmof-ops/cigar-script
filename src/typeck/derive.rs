// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! What `derive` asks of a type (#38, `E1232`), and copy types (#22 D3 (i)).
//!
//! - `derive(X)` requires every field (of every variant, for an enum) to
//!   implement `X`; a field that does not is `E1232`, named. A field of the
//!   type's own type parameter is the derived `impl`'s bound instead, and an
//!   optional field needs its inner type to implement `X`.
//! - `derive(Copy)` requires every field to be a copy type: a scalar,
//!   `Text`, or a record or enum that carries `derive(Copy)`.
//! - A value of a copy type is copied where a place (a name, a field, an
//!   element) holding one is bound, assigned, passed, stored or returned:
//!   the two names never share it.

use super::{Ck, Resolution};
use crate::types::{At, Ty};
use cigscript::syntax::ast::{Expr, ExprKind};

impl Ck<'_> {
    /// `E1232` for every derive a field cannot have.
    pub(super) fn check_derives(&mut self) {
        let prog = self.prog;
        let mut found: Vec<(String, At, String)> = Vec::new();
        let mut each = |what: &str,
                        name: &str,
                        generics: &[String],
                        derives: &[(String, At)],
                        fields: Vec<(String, &Ty)>,
                        ck: &Ck| {
            for (iface, at) in derives {
                if what == "enum" && iface == "Default" {
                    found.push((
                        iface.clone(),
                        *at,
                        format!("`{name}` can't derive `Default`: an enum has no default variant to pick; write `impl Default for {name}`"),
                    ));
                    continue;
                }
                for (f, t) in &fields {
                    let inner = match t {
                        Ty::Opt(i) => i.as_ref(),
                        t => t,
                    };
                    let ok = if iface == "Copy" {
                        prog.copy_type(t)
                    } else if matches!(inner, Ty::Param(p) if generics.contains(p)) {
                        true
                    } else {
                        !prog.interfaces.contains_key(iface) || ck.implements(inner, iface)
                    };
                    if !ok {
                        let why = if iface == "Copy" {
                            "which is not a copy type".to_string()
                        } else {
                            format!("which doesn't implement `{iface}`")
                        };
                        found.push((
                            iface.clone(),
                            *at,
                            format!(
                                "`{name}` can't derive `{iface}`: field `{f}` is a `{t}`, {why}"
                            ),
                        ));
                        break;
                    }
                }
            }
        };
        for r in prog
            .records
            .values()
            .filter(|r| !prog.foreign.contains(&r.name))
        {
            let fields = r.fields.iter().map(|(f, t)| (f.clone(), t)).collect();
            each("record", &r.name, &r.generics, &r.derives, fields, self);
        }
        for e in prog
            .enums
            .values()
            .filter(|e| !prog.foreign.contains(&e.name))
        {
            let fields = e
                .variants
                .iter()
                .flat_map(|v| {
                    v.fields
                        .iter()
                        .map(move |(f, t)| (format!("{}.{f}", v.name), t))
                })
                .collect();
            each("enum", &e.name, &e.generics, &e.derives, fields, self);
        }
        for (iface, at, msg) in found {
            let hint = if iface == "Copy" {
                "a copy type's fields are scalars, `Text`, or records that `derive(Copy)`; leave `Copy` out and the record moves".to_string()
            } else {
                format!("derive `{iface}` on the field's type, write its `impl {iface}`, or leave `{iface}` out")
            };
            self.push("E1232", msg, None, Some(at), Some(hint));
        }
        // A field default must fit its field.
        for r in prog
            .records
            .values()
            .filter(|r| !prog.foreign.contains(&r.name))
        {
            for (f, text, lt) in &r.defaults {
                let Some((_, ft)) = r.fields.iter().find(|(x, _)| x == f) else {
                    continue;
                };
                if !super::assignable(lt, ft) {
                    self.push(
                        "E1201",
                        format!("the default of `{f}` is `{text}`, which is not a `{ft}`"),
                        None,
                        Some(r.at),
                        Some(format!(
                            "expected `{ft}` here, because `{f}` is declared as `{ft}`"
                        )),
                    );
                }
            }
        }
    }

    /// `e` gives a value of type `t` to a new holder: when `t` is a copy
    /// record or enum and `e` is a place, the holder gets a copy.
    pub(super) fn copy_out(&mut self, e: &Expr, t: &Ty) {
        let copy = matches!(t, Ty::Record(..) | Ty::Enum(..)) && self.prog.copy_type(t);
        let place = matches!(
            e.kind,
            ExprKind::Ident(_) | ExprKind::Member { .. } | ExprKind::Index { .. }
        );
        if copy && place && self.interpolating == 0 {
            self.resolutions.push(Resolution::Wrap {
                start: e.span.start,
                end: e.span.end,
                pre: "_cg_copy_value(".into(),
                post: ")".into(),
            });
        }
    }
}
