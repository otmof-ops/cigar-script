// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Bounds on type parameters (#38, section 1.1).
//!
//! - Each bound names an interface (`E1202` otherwise).
//! - Inside the function, a value of a bounded type parameter has the
//!   methods of its bounds, with their signatures; a method none of them
//!   declares is `E1313`. An unbounded type parameter is as before: any
//!   interface's method, picked by the value when it runs.
//! - At each call, the type the call gives a bounded parameter must have an
//!   `impl` of every bound (`E1230`). A type parameter of the caller has
//!   only the bounds the caller declares for it. A type the checker cannot
//!   see (`Any`) is not judged.
//!
//! The bounds are erased: a method on a `T` dispatches by the value when the
//! program runs, as a method on `Any` does.

use super::Ck;
use crate::types::{self, Ty};
use cigscript::syntax::ast::Expr;
use cigscript::syntax::span::Span;
use std::collections::HashMap;

impl Ck<'_> {
    /// The bounds of the type parameter `p` where the checker is, innermost
    /// function first. `None` when no enclosing function bounds it.
    pub(super) fn bounds_of(&self, p: &str) -> Option<&Vec<String>> {
        self.bounds
            .iter()
            .rev()
            .find_map(|m| m.get(p))
            .filter(|b| !b.is_empty())
    }

    /// Entering a function: its type parameters and their bounds.
    pub(super) fn enter_bounds(&mut self, sig: Option<&types::FnSig>) {
        let m: HashMap<String, Vec<String>> = sig
            .map(|s| s.generics.iter().cloned().collect())
            .unwrap_or_default();
        self.bounds.push(m);
    }

    /// Every bound of every function names an interface.
    pub(super) fn validate_bounds(&mut self) {
        let prog = self.prog;
        for s in prog.fns.values() {
            for (g, bs) in &s.generics {
                for b in bs {
                    if !prog.interfaces.contains_key(b) {
                        self.push(
                            "E1202",
                            format!(
                                "`{b}` in `{}<{g}: {b}>` is not an interface: a bound names one",
                                super::shown_name(&s.name)
                            ),
                            None,
                            Some(s.at),
                            Some(format!(
                                "declare `interface {b} {{ ... }}`, or check the spelling"
                            )),
                        );
                    }
                }
            }
        }
    }

    /// The line a signature is written on, in the file as written.
    fn line_of(&self, at: types::At) -> Option<usize> {
        let src = self.source;
        (!src.is_empty() && at.start <= src.len())
            .then(|| src[..at.start].matches('\n').count() + 1)
    }

    /// Does `t` have an `impl` of the interface `b`?
    pub(super) fn implements(&self, t: &Ty, b: &str) -> bool {
        match t {
            Ty::Param(q) => self
                .bounds_of(q)
                .is_some_and(|bs| bs.iter().any(|x| x == b)),
            // A generic `impl` fits when its own bounds hold (#38).
            _ => self.select(b, t).is_ok(),
        }
    }

    /// After a call to `sig` (shown as `what`): every bounded type
    /// parameter, as this call binds it, has an `impl` of each bound.
    pub(super) fn check_bounds(&mut self, what: &str, sig: &types::FnSig, span: Span) {
        let binds = std::mem::take(&mut self.last_binds);
        let line = self.line_of(sig.at);
        for (g, bs) in &sig.generics {
            let Some(t) = binds.get(g) else { continue };
            if matches!(t, Ty::Any | Ty::Never | Ty::Null) {
                continue;
            }
            for b in bs {
                // A bound that names no interface is already `E1202`.
                if self.implements(t, b) || !self.prog.interfaces.contains_key(b) {
                    continue;
                }
                let where_ = match line {
                    Some(l) => format!("`{g}: {b}` on line {l}"),
                    None => format!("`{g}: {b}`"),
                };
                let hint = match t {
                    Ty::Param(q) => {
                        let mut have = self.bounds_of(q).cloned().unwrap_or_default();
                        have.push(b.clone());
                        format!(
                            "`{what}` asks for it in {where_}; add the bound here too: `<{q}: {}>`",
                            have.join(" + ")
                        )
                    }
                    _ => format!(
                        "`{what}` asks for it in {where_}; write `impl {b} for {t} {{ ... }}` (or `derive({b})` once `derive` is built)"
                    ),
                };
                self.err(
                    "E1230",
                    format!("`{t}` doesn't implement `{b}`, which `{what}` needs for `{g}`"),
                    span,
                    Some(hint),
                );
            }
        }
    }

    /// `x.name(args)` where `x` is a `p` bounded by `bs`: the method of the
    /// bound that declares it, checked with its signature, and dispatched by
    /// the value when the program runs.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn bounded_method(
        &mut self,
        receiver: &Expr,
        p: &str,
        bs: &[String],
        name: &str,
        args: &[Expr],
        safe: bool,
        span: Span,
    ) -> Ty {
        let prog = self.prog;
        let found = bs.iter().find_map(|b| {
            prog.interfaces
                .get(b)
                .and_then(|i| i.methods.iter().find(|m| m.name == name))
                .map(|m| (b.clone(), m.clone()))
        });
        let Some((iface, m)) = found else {
            for a in args {
                self.infer(a);
            }
            self.err(
                "E1313",
                format!(
                    "`{p}` has no method `{name}`: its bounds ({}) do not declare one",
                    bs.iter()
                        .map(|b| format!("`{b}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                span,
                Some(format!(
                    "add a bound whose interface declares `{name}`: `{p}: {} + ...`",
                    bs.join(" + ")
                )),
            );
            return Ty::Any;
        };
        self.dispatch_by_value(&iface, receiver, name, args.len(), safe, span);
        let params: Vec<Ty> = m
            .params
            .iter()
            .filter(|x| x.name != "self")
            .map(|x| x.ty.clone().unwrap_or(Ty::Any))
            .collect();
        let ret = m.ret.clone().unwrap_or(Ty::Any);
        self.apply(
            name,
            &[],
            &params,
            0,
            None,
            &ret,
            args,
            span,
            HashMap::new(),
        )
    }
}
