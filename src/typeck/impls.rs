// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Generic implementations and their coherence (#38, section 1.2).
//!
//! - **Selection.** An `impl` is a pattern: `impl<T: Show> Show for List<T>`
//!   gives `List<Point>` the methods of `Show` when `T = Point` implements
//!   `Show`. A pattern that fits but whose bounds do not hold is `E1230`.
//! - **Overlap.** Two `impl`s of one interface whose types can be the same
//!   type (`List<Int>` and `List<T>`) are `E1231`, naming both: there is no
//!   specialization, so a value never has two.
//! - **Orphans.** An `impl` of a module's interface for a type this file does
//!   not declare (a built-in type, or another module's record) is `E1231`: it
//!   belongs in the module that declares the interface, so two files can never
//!   both give one type the same interface.

use super::{same_ty, subst, unify, Ck};
use crate::types::{ImplDecl, Ty};
use std::collections::HashMap;

/// What an `impl` pattern bound its type parameters to, for a type.
pub(super) type Binds = HashMap<String, Ty>;

/// A generic `impl` whose pattern fits, and the bound it misses: `param`
/// was bound to `ty`, which has no `impl` of `bound`.
pub(super) struct Unmet {
    pub im: ImplDecl,
    pub param: String,
    pub bound: String,
    pub ty: Ty,
}

/// Can `a` (with type parameters `ga`) and `b` (with `gb`) be the same type?
fn overlap(a: &Ty, ga: &[String], b: &Ty, gb: &[String]) -> bool {
    let var = |t: &Ty, g: &[String]| matches!(t, Ty::Param(p) if g.contains(p));
    if var(a, ga) || var(b, gb) {
        return true;
    }
    match (a, b) {
        (Ty::List(x), Ty::List(y))
        | (Ty::ListView(x), Ty::ListView(y))
        | (Ty::Map(x), Ty::Map(y))
        | (Ty::Opt(x), Ty::Opt(y))
        | (Ty::Channel(x), Ty::Channel(y)) => overlap(x, ga, y, gb),
        (Ty::Record(n, xs), Ty::Record(m, ys)) | (Ty::Enum(n, xs), Ty::Enum(m, ys)) => {
            n == m
                && (xs.is_empty()
                    || ys.is_empty()
                    || (xs.len() == ys.len()
                        && xs.iter().zip(ys).all(|(x, y)| overlap(x, ga, y, gb))))
        }
        (Ty::Fn(px, rx), Ty::Fn(py, ry)) => {
            px.len() == py.len()
                && px.iter().zip(py).all(|(x, y)| overlap(x, ga, y, gb))
                && overlap(rx, ga, ry, gb)
        }
        (Ty::Path, Ty::Text) | (Ty::Text, Ty::Path) => true,
        _ => a == b,
    }
}

fn names(im: &ImplDecl) -> Vec<String> {
    im.generics.iter().map(|g| g.0.clone()).collect()
}

impl Ck<'_> {
    /// Does `im`'s pattern fit `t`? Its bindings when it does.
    pub(super) fn impl_binds(&self, im: &ImplDecl, t: &Ty) -> Option<Binds> {
        let gens = names(im);
        if gens.is_empty() {
            return same_ty(&im.for_ty, t).then(Binds::new);
        }
        let mut b = Binds::new();
        unify(&im.for_ty, t, &gens, &mut b);
        let whole = subst(&im.for_ty, &gens, &b);
        same_ty(&whole, t).then_some(b)
    }

    /// The first bound of `im` its bindings do not meet: (parameter, bound,
    /// the type bound to it).
    pub(super) fn unmet(&self, im: &ImplDecl, b: &Binds) -> Option<(String, String, Ty)> {
        for (g, bs) in &im.generics {
            let Some(t) = b.get(g) else { continue };
            if matches!(t, Ty::Any | Ty::Never | Ty::Null) {
                continue;
            }
            for x in bs {
                if self.prog.interfaces.contains_key(x) && !self.implements(t, x) {
                    return Some((g.clone(), x.clone(), t.clone()));
                }
            }
        }
        None
    }

    /// The `impl` of `iface` that gives `t` its methods, and its bindings;
    /// or, when a pattern fits and its bounds do not hold, what is missing.
    pub(super) fn select(
        &self,
        iface: &str,
        t: &Ty,
    ) -> Result<(ImplDecl, Binds), Option<Box<Unmet>>> {
        let mut missing = None;
        if self.selecting.get() > 32 {
            return Err(None);
        }
        self.selecting.set(self.selecting.get() + 1);
        let mut out = None;
        for im in self.prog.impls.iter().filter(|im| im.interface == iface) {
            if let Some(b) = self.impl_binds(im, t) {
                match self.unmet(im, &b) {
                    None => {
                        out = Some((im.clone(), b));
                        break;
                    }
                    Some((param, bound, ty)) => {
                        missing.get_or_insert(Box::new(Unmet {
                            im: im.clone(),
                            param,
                            bound,
                            ty,
                        }));
                    }
                }
            }
        }
        self.selecting.set(self.selecting.get() - 1);
        out.ok_or(missing)
    }

    /// `E1231`: overlapping `impl`s, and an `impl` of a module's interface
    /// for a type this file does not declare.
    pub(super) fn check_coherence(&mut self) {
        let prog = self.prog;
        for (k, b) in prog.impls.iter().enumerate() {
            for a in &prog.impls[..k] {
                if a.interface == b.interface && overlap(&a.for_ty, &names(a), &b.for_ty, &names(b))
                {
                    let line = self.line_at(a.at.start);
                    let where_ = line.map(|l| format!(" on line {l}")).unwrap_or_default();
                    self.push(
                        "E1231",
                        format!(
                            "`impl {} for {}` overlaps `impl {} for {}`{where_}: a value of both types would have two",
                            b.interface, b.for_ty, a.interface, a.for_ty
                        ),
                        None,
                        Some(b.at),
                        Some(
                            "keep one: an `impl` is never chosen over another by being more specific"
                                .into(),
                        ),
                    );
                    break;
                }
            }
            if prog.foreign.contains(&b.interface) && !self.declared_here(&b.for_ty) {
                self.push(
                    "E1231",
                    format!(
                        "`impl {} for {}` belongs in the module that declares `{}`: this file declares neither `{}` nor `{}`",
                        b.interface, b.for_ty, b.interface, b.interface, b.for_ty
                    ),
                    None,
                    Some(b.at),
                    Some(format!(
                        "write it in the module that declares `{}`, or implement `{}` for a type this file declares",
                        b.interface, b.interface
                    )),
                );
            }
        }
    }

    /// Is `t`'s outermost type a record or enum this file declares?
    fn declared_here(&self, t: &Ty) -> bool {
        match t {
            Ty::Record(n, _) | Ty::Enum(n, _) => !self.prog.foreign.contains(n),
            _ => false,
        }
    }

    /// The line of an offset in the file as written.
    pub(super) fn line_at(&self, at: usize) -> Option<usize> {
        let src = self.source;
        (!src.is_empty() && at <= src.len()).then(|| src[..at].matches('\n').count() + 1)
    }
}
