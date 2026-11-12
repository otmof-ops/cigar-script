// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Generic implementations and default methods (#38, sections 1.2 and 1.3).
//!
//! - `impl<T: Show> Show for List<T> where ... { pull ... }`: the header goes,
//!   the methods stay where they are, renamed by [`impl_fn_name`] after the
//!   pattern type (`List<T>`), and carry the impl's type parameters and bounds
//!   so the checker sees a bounded `T` in their bodies.
//! - A method written with a body in an `interface` is its default. It is
//!   lowered once, as the method of a `Self` bounded by the interface
//!   (`_cg_impl_Show_Self_m`), and an `impl` that leaves it out gets a
//!   function under its own name that calls the default.

use super::{impl_fn_name, ty, Program, L, T};
use crate::types::Ty;

/// The name of the type parameter a default method's `self` has.
pub const SELF: &str = "Self";

impl L<'_> {
    /// `interface Name { ... }` in the statement pass: a signature with no
    /// body is blanked; one with a body is lowered as the default.
    pub(super) fn interface_stmt(&mut self, i: usize) -> usize {
        let name = self.ident(i + 1).unwrap_or_default();
        let open = i + 2;
        let close = self.matching(open);
        self.e.blank(self.src, self.sp(i).start, self.sp(open).end);
        self.e
            .blank(self.src, self.sp(close).start, self.sp(close).end);
        let mut k = open + 1;
        while k < close {
            if !matches!(self.k(k), T::Pull) {
                k += 1;
                continue;
            }
            match self.sig_shape(k, close) {
                (true, _) => {
                    let saved = std::mem::replace(
                        &mut self.impl_generics,
                        vec![(SELF.to_string(), vec![name.clone()])],
                    );
                    let tp = self.tparams.clone();
                    self.tparams.push(SELF.to_string());
                    k = self.pull_stmt(k, close, Some((&name, &Ty::Param(SELF.to_string()))));
                    self.tparams = tp;
                    self.impl_generics = saved;
                }
                (false, end) => {
                    self.e.blank(self.src, self.sp(k).start, self.sp(end).start);
                    k = end;
                }
            }
        }
        close + 1
    }

    /// From `pull` at `k`: does the signature open a body (`{` outside its
    /// parentheses, before the line ends), and where does the line end?
    fn sig_shape(&self, k: usize, close: usize) -> (bool, usize) {
        let mut depth = 0i32;
        let mut j = k + 1;
        while j < close {
            match self.k(j) {
                T::LParen => depth += 1,
                T::RParen => depth -= 1,
                T::LBrace if depth == 0 => return (true, j),
                T::Newline | T::Pull if depth == 0 => return (false, j),
                _ => {}
            }
            j += 1;
        }
        (false, close)
    }

    /// The `<A: B>` after `impl`, if any: the parameters and the index after.
    pub(super) fn impl_params(&mut self, i: usize) -> (Vec<(String, Vec<String>)>, usize) {
        if matches!(self.k(i + 1), T::Lt) {
            self.generic_params(i + 1)
        } else {
            (Vec::new(), i + 1)
        }
    }

    /// For each `impl` of this file, each default method it leaves out: a
    /// function under the impl's name for it that calls the default, and the
    /// method's signature on the impl so calls and dispatch find it.
    pub(super) fn fill_defaults(&mut self) {
        let prog: &mut Program = &mut self.prog;
        let mut text = String::new();
        for im in prog.impls.iter_mut() {
            let Some(iface) = prog.interfaces.get(&im.interface) else {
                continue;
            };
            for d in &iface.defaults {
                if im.methods.iter().any(|m| &m.name == d) {
                    continue;
                }
                let Some(sig) = iface.methods.iter().find(|m| &m.name == d) else {
                    continue;
                };
                let params: Vec<String> = sig
                    .params
                    .iter()
                    .filter(|p| p.name != "self")
                    .map(|p| p.name.clone())
                    .collect();
                let args = if params.is_empty() {
                    String::new()
                } else {
                    format!(", {}", params.join(", "))
                };
                let own = impl_fn_name(&im.interface, &im.for_ty, d);
                let default = impl_fn_name(&im.interface, &Ty::Param(SELF.to_string()), d);
                text.push_str(&format!(
                    "pull {own}(self{args}) {{ snuff {default}(self{args}) }}\n"
                ));
                let mut m = sig.clone();
                // The impl's type parameters and bounds hold in it too.
                let mut gens = im.generics.clone();
                gens.append(&mut m.generics);
                m.generics = gens;
                for p in m.params.iter_mut() {
                    if p.name == "self" {
                        p.ty = Some(im.for_ty.clone());
                    }
                }
                im.methods.push(m);
            }
        }
        self.appended.push_str(&text);
    }
}

/// A run-time test that the value named `v` is an `impl`'s type: by kind, as
/// [`super::type_test`] says, and for a list or map of a known element type,
/// by its elements too (dispatch by element type).
pub fn impl_test(t: &Ty, v: &str) -> String {
    let kind = super::type_test(t, v);
    match t {
        Ty::List(el) | Ty::Map(el) if is_concrete(el) => {
            format!("{kind} and _cg_fits({v}, {})", ty::descriptor(t))
        }
        _ => kind,
    }
}

/// A type with no type parameter and no `Any` in it.
fn is_concrete(t: &Ty) -> bool {
    match t {
        Ty::Any | Ty::Param(_) => false,
        Ty::List(a) | Ty::ListView(a) | Ty::Map(a) | Ty::Opt(a) | Ty::Channel(a) => is_concrete(a),
        Ty::Record(_, a) | Ty::Enum(_, a) => a.iter().all(is_concrete),
        Ty::Fn(ps, r) => ps.iter().all(is_concrete) && is_concrete(r),
        _ => true,
    }
}
