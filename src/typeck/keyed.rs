// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `Map<K, V>` with keys that are not text, and `Set<T>` (#38, section 2).
//!
//! A key or element type implements `Hash` and `Eq` (`E1230` otherwise). The
//! checker writes each operation as a call to its run-time function
//! (`src/lower/keyed.rs`): `m[k]`, `m[k] = v`, the methods, `for k in m` and
//! `exhale m`. A `Map<K, V>` starts as `{}` and a set as `Set.new()` or
//! `Set.of(list)`.

use super::{Ck, Resolution, STICK_MUTATORS};
use crate::types::{At, Ty};
use cigscript::syntax::ast::{AssignOp, Expr};
use cigscript::syntax::span::Span;
use std::collections::HashMap;

/// The methods of a `Map<K, V>`: name, run-time function, parameters, result.
fn map_method(name: &str, k: &Ty, v: &Ty) -> Option<(&'static str, Vec<Ty>, Ty)> {
    let opt = |t: &Ty| Ty::Opt(Box::new(t.clone()));
    Some(match name {
        "get" => ("_cg_km_get", vec![k.clone()], opt(v)),
        "has" => ("_cg_km_has", vec![k.clone()], Ty::Bool),
        "set" => ("_cg_km_set", vec![k.clone(), v.clone()], Ty::Null),
        "remove" => ("_cg_km_remove", vec![k.clone()], opt(v)),
        "keys" => ("_cg_km_keys", vec![], Ty::List(Box::new(k.clone()))),
        "values" => ("_cg_km_values", vec![], Ty::List(Box::new(v.clone()))),
        "len" => ("_cg_km_len", vec![], Ty::Int),
        "copy" => (
            "_cg_km_copy",
            vec![],
            Ty::KeyMap(Box::new(k.clone()), Box::new(v.clone())),
        ),
        _ => return None,
    })
}

/// The methods of a `Set<T>`.
fn set_method(name: &str, t: &Ty) -> Option<(&'static str, Vec<Ty>, Ty)> {
    let set = || Ty::Set(Box::new(t.clone()));
    Some(match name {
        "add" => ("_cg_set_add", vec![t.clone()], Ty::Null),
        "remove" => ("_cg_set_remove", vec![t.clone()], Ty::Bool),
        "has" => ("_cg_km_has", vec![t.clone()], Ty::Bool),
        "len" => ("_cg_km_len", vec![], Ty::Int),
        "items" => ("_cg_km_keys", vec![], Ty::List(Box::new(t.clone()))),
        "union" => ("_cg_set_union", vec![set()], set()),
        "intersection" => ("_cg_set_intersection", vec![set()], set()),
        "difference" => ("_cg_set_difference", vec![set()], set()),
        "copy" => ("_cg_km_copy", vec![], set()),
        _ => return None,
    })
}

const MAP_METHODS: &str = "get, has, set, remove, keys, values, len, copy";
const SET_METHODS: &str = "add, remove, has, len, items, union, intersection, difference, copy";

impl Ck<'_> {
    /// A key or element type of a map or set implements `Hash` and `Eq`.
    pub(super) fn key_type_ok(&mut self, k: &Ty, what: &str, at: At, span: Option<Span>) {
        if matches!(k, Ty::Any | Ty::Param(_)) {
            return;
        }
        for i in ["Hash", "Eq"] {
            if self.prog.interfaces.contains_key(i) && !self.implements(k, i) {
                self.push(
                    "E1230",
                    format!("`{k}` doesn't implement `{i}`, which {what} needs"),
                    span,
                    Some(at),
                    Some(format!(
                        "`derive(Hash, Eq)` on `{k}`, or write `impl {i} for {k} {{ ... }}`"
                    )),
                );
                return;
            }
        }
    }

    /// `x.name(args)` on a `Map<K, V>` or a `Set<T>`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn keyed_method(
        &mut self,
        receiver: &Expr,
        name: &str,
        args: &[Expr],
        safe: bool,
        span: Span,
        recv: &Ty,
    ) -> Option<Ty> {
        let (found, list) = match recv {
            Ty::KeyMap(k, v) => (map_method(name, k, v), MAP_METHODS),
            Ty::Set(t) => (set_method(name, t), SET_METHODS),
            _ => return None,
        };
        let Some((func, params, ret)) = found else {
            for a in args {
                self.infer(a);
            }
            self.err(
                "E1313",
                format!("a `{recv}` has no method `{name}`"),
                span,
                Some(format!("its methods are: {list}")),
            );
            return Some(Ty::Any);
        };
        if self.d2_applies() && (STICK_MUTATORS.contains(&name) || name == "add") {
            if let Some(root) = self.stick_root(receiver) {
                self.err(
                    "E1215",
                    format!("`{root}` is a `stick` and `.{name}` would write through it"),
                    span,
                    Some(format!(
                        "declare it with `roll {root} = ...` if it has to change, or call `.copy()` for a fresh one"
                    )),
                );
            }
        }
        self.resolutions.push(Resolution::Method {
            recv_start: receiver.span.start,
            recv_end: receiver.span.end,
            call_end: span.end,
            func: func.to_string(),
            has_args: !args.is_empty(),
            safe,
        });
        Some(self.apply(
            name,
            &[],
            &params,
            0,
            None,
            &ret,
            args,
            span,
            HashMap::new(),
        ))
    }

    /// `m[k]` on a `Map<K, V>`.
    pub(super) fn keyed_index(
        &mut self,
        e: &Expr,
        object: &Expr,
        index: &Expr,
        t: &Ty,
    ) -> Option<Ty> {
        let Ty::KeyMap(k, v) = t else {
            if let Ty::Set(_) = t {
                self.err(
                    "E1313",
                    format!("a `{t}` is not indexed"),
                    e.span,
                    Some("ask `s.has(x)`, or take `s.items()` for a list".into()),
                );
                return Some(Ty::Any);
            }
            return None;
        };
        let it = self.infer(index);
        self.expect(
            index,
            &it,
            k,
            "E1201",
            format!("a `{t}` is indexed by `{k}`"),
        );
        self.call_shape("_cg_km_index", object, &[index], e.span.end);
        Some((**v).clone())
    }

    /// `m[k] = v` on a `Map<K, V>`: `true` when handled.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn keyed_assign(
        &mut self,
        object: &Expr,
        index: &Expr,
        value: &Expr,
        op: AssignOp,
        span: Span,
        t: &Ty,
        vt: &Ty,
    ) -> bool {
        let Ty::KeyMap(k, v) = t else { return false };
        if op != AssignOp::Set {
            self.err(
                "E1102",
                format!("a compound assignment to an entry of a `{t}` is not built yet (otmof-ops/cigar-script#38)"),
                span,
                Some("write it out: `m[k] = m[k] + x`".into()),
            );
            return true;
        }
        if self.d2_applies() {
            if let Some(root) = self.stick_root(object) {
                self.stick_mutation(&root, "entries", span);
            }
        }
        let it = self.infer(index);
        self.expect(
            index,
            &it,
            k,
            "E1201",
            format!("a `{t}` is indexed by `{k}`"),
        );
        self.expect(value, vt, v, "E1201", format!("a `{t}` holds `{v}`"));
        self.call_shape("_cg_km_set", object, &[index, value], value.span.end);
        true
    }

    /// `f(object, a, b)` written over `object[a] = b` or `object[a]`: the
    /// call opens at the object, and the text between the parts becomes
    /// `, `, with its line breaks kept.
    fn call_shape(&mut self, f: &str, object: &Expr, parts: &[&Expr], end: usize) {
        self.resolutions.push(Resolution::Wrap {
            start: object.span.start,
            end: object.span.end,
            pre: format!("{f}("),
            post: String::new(),
        });
        let mut from = object.span.end;
        for p in parts {
            let gap = self.gap_text(from, p.span.start);
            self.resolutions.push(Resolution::Rename {
                start: from,
                end: p.span.start,
                text: gap,
            });
            from = p.span.end;
        }
        let tail = self.gap_tail(from, end);
        self.resolutions.push(Resolution::Rename {
            start: from,
            end,
            text: tail,
        });
    }

    /// The text between two parts in the file as written, as `, ` with its
    /// line breaks kept.
    fn gap_text(&self, from: usize, to: usize) -> String {
        let (a, b) = ((self.original)(from), (self.original)(to));
        let lines = self
            .source
            .get(a..b.max(a))
            .unwrap_or("")
            .matches('\n')
            .count();
        format!(", {}", "\n".repeat(lines))
    }

    /// What follows the last part up to `end` (a `]`), as `)`.
    fn gap_tail(&self, from: usize, end: usize) -> String {
        let (a, b) = ((self.original)(from), (self.original)(end));
        let lines = self
            .source
            .get(a..b.max(a))
            .unwrap_or("")
            .matches('\n')
            .count();
        format!("){}", "\n".repeat(lines))
    }

    /// `for x in m` over a `Map<K, V>` (its keys) or a `Set<T>` (its
    /// elements): the element type, the loop reading a list of them.
    pub(super) fn keyed_iter(&mut self, iter: &Expr, t: &Ty) -> Option<Ty> {
        let el = match t {
            Ty::KeyMap(k, _) => (**k).clone(),
            Ty::Set(x) => (**x).clone(),
            _ => return None,
        };
        self.resolutions.push(Resolution::Wrap {
            start: iter.span.start,
            end: iter.span.end,
            pre: "_cg_km_keys(".into(),
            post: ")".into(),
        });
        Some(el)
    }

    /// `exhale m`: a keyed map or a set shown as its keys and values.
    pub(super) fn keyed_show(&mut self, v: &Expr, t: &Ty) {
        let f = match t {
            Ty::KeyMap(..) => "_cg_km_show",
            Ty::Set(_) => "_cg_set_show",
            _ => return,
        };
        if self.interpolating == 0 {
            self.resolutions.push(Resolution::Wrap {
                start: v.span.start,
                end: v.span.end,
                pre: format!("{f}("),
                post: ")".into(),
            });
        }
    }

    /// `_cg_set_of(xs)` (`Set.of`) and `_cg_set_new()` (`Set.new`).
    pub(super) fn set_constructor(&mut self, name: &str, args: &[Expr], span: Span) -> Option<Ty> {
        match name {
            "_cg_set_new" => {
                for a in args {
                    self.infer(a);
                }
                Some(Ty::Set(Box::new(Ty::Any)))
            }
            "_cg_set_of" => {
                let ts: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
                let el = match ts.first() {
                    Some(Ty::List(t) | Ty::ListView(t)) => (**t).clone(),
                    _ => Ty::Any,
                };
                self.key_type_ok(&el, "a `Set` element", At::default(), Some(span));
                Some(Ty::Set(Box::new(el)))
            }
            _ => None,
        }
    }
}
