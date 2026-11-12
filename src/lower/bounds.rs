// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Bounds on type parameters (#38, section 1.1): `<T: A + B>` after a
//! function's name, and a `where` clause after its result type,
//! `where K: Hash + Eq, V: Show`.
//!
//! The bounds are read into the signature (each type parameter with the
//! interfaces it needs) for the checker, and erased from the text, like the
//! rest of a signature's types: CigScript never sees them.

use super::{L, T};

impl L<'_> {
    /// One bound list at token `j`, after the `:`: `A + B`. Returns the
    /// interface names and the index after the list. A bound written with
    /// type arguments (`Into<Text>`) keeps its name; the arguments wait on
    /// generic interfaces.
    pub(super) fn bound_list(&mut self, mut j: usize) -> (Vec<String>, usize) {
        let mut out = Vec::new();
        while let Some(b) = self.ident(j) {
            out.push(b);
            j += 1;
            if matches!(self.k(j), T::Lt) {
                j = self.close_angle(j, self.toks.len() - 1) + 1;
            }
            if matches!(self.k(j), T::Plus) {
                j += 1;
            } else {
                break;
            }
        }
        (out, j)
    }

    /// A `where` clause at token `j`, if there is one: each `Name: A + B`
    /// adds its bounds to the type parameter of that name (`E1202` for a
    /// name the function does not declare). Returns the index after it, and
    /// blanks it when `edit` is set.
    pub(super) fn where_clause(
        &mut self,
        j: usize,
        edit: bool,
        gens: &mut [(String, Vec<String>)],
    ) -> usize {
        if !self.is_word(j, "where") {
            return j;
        }
        let kw = j;
        let mut k = j + 1;
        let mut last = self.sp(kw).end;
        while let Some(n) = self.ident(k) {
            if !matches!(self.k(k + 1), T::Colon) {
                break;
            }
            let at = k;
            let (bs, next) = self.bound_list(k + 2);
            if bs.is_empty() {
                let sp = self.sp(k + 2);
                self.err(
                    "E1202",
                    format!("`where {n}:` needs an interface: `where {n}: Show`"),
                    sp.start,
                    sp.end,
                );
                break;
            }
            match gens.iter_mut().find(|g| g.0 == n) {
                Some(g) => {
                    for b in bs {
                        if !g.1.contains(&b) {
                            g.1.push(b);
                        }
                    }
                }
                None => {
                    let sp = self.sp(at);
                    self.err_hint(
                        "E1202",
                        format!(
                            "`{n}` in this `where` clause is not a type parameter of the function"
                        ),
                        sp.start,
                        sp.end,
                        format!("declare it in the function's `<...>`, as `<{n}>`"),
                    );
                }
            }
            last = self.sp(next - 1).end;
            k = next;
            if matches!(self.k(k), T::Comma) {
                k += 1;
            } else {
                break;
            }
        }
        if edit {
            self.e.blank(self.src, self.sp(kw).start, last);
        }
        k
    }

    /// A bound on a record's or an enum's type parameter: refused, since
    /// nothing checks it there yet (a bound goes on the functions).
    pub(super) fn no_bounds_on(&mut self, what: &str, gens: &[(String, Vec<String>)], at: usize) {
        if let Some((n, bs)) = gens.iter().find(|g| !g.1.is_empty()) {
            let sp = self.sp(at);
            self.err_hint(
                "E1202",
                format!(
                    "a bound on a {what}'s type parameter (`{n}: {}`) is not checked; bounds go on functions",
                    bs.join(" + ")
                ),
                sp.start,
                sp.end,
                format!("write `<{n}>` here, and `<{n}: {}>` on the functions that need it", bs.join(" + ")),
            );
        }
    }
}
