// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Maps with keys that are not text, and sets (#38, section 2).
//!
//! cig's maps are keyed by text. A `Map<K, V>` whose `K` is not text, and a
//! `Set<T>`, are held in a cig map keyed by the key's `hash()` (an injective
//! text key for the built-in types and for `derive(Hash)`), each entry a list
//! of `{k, v}` so two keys with one hash stay apart (compared with `Eq`).
//! Reading keys back gives the keys themselves, in the order they were
//! first added, as cig's maps keep theirs; iterating takes the keys as they
//! are when the loop starts, as iterating a cig map does.
//!
//! The checker writes each operation as a call to one of these functions;
//! they are appended to a file that writes a `Set` or a `Map<K, V>` with a
//! key that is not text.

use super::{L, T};

/// The run-time functions of keyed maps and sets.
pub const KEYED: &str = r#"pull _cg_km_find(m, k) { stick b = m.get(_cg_dispatch_Hash_hash(k)); if b == null { snuff null }; for e in b { if _cg_dispatch_Eq_eq(e.k, k) { snuff e } }; snuff null }
pull _cg_km_get(m, k) { stick e = _cg_km_find(m, k); if e == null { snuff null }; snuff e.v }
pull _cg_km_index(m, k) { stick e = _cg_km_find(m, k); if e == null { cough "E505 map has no key " + _cg_inner(k) }; snuff e.v }
pull _cg_km_has(m, k) { snuff _cg_km_find(m, k) != null }
pull _cg_km_set(m, k, v) { stick e = _cg_km_find(m, k); if e != null { e.v = v; snuff null }; stick h = _cg_dispatch_Hash_hash(k); if not m.has(h) { m[h] = [] }; m[h].push({k: k, v: v}); snuff null }
pull _cg_km_remove(m, k) { stick h = _cg_dispatch_Hash_hash(k); stick b = m.get(h); if b == null { snuff null }; roll keep = []; roll out = null; for e in b { if _cg_dispatch_Eq_eq(e.k, k) { out = e.v } else { keep.push(e) } }; if keep.len() == 0 { m.remove(h) } else { m[h] = keep }; snuff out }
pull _cg_km_keys(m) { roll out = []; for h in m { for e in m[h] { out.push(e.k) } }; snuff out }
pull _cg_km_values(m) { roll out = []; for h in m { for e in m[h] { out.push(e.v) } }; snuff out }
pull _cg_km_len(m) { roll n = 0; for h in m { n = n + m[h].len() }; snuff n }
pull _cg_km_copy(m) { roll out = {}; for h in m { roll b = []; for e in m[h] { b.push({k: e.k, v: e.v}) }; out[h] = b }; snuff out }
pull _cg_km_show(m) { roll parts = []; for h in m { for e in m[h] { parts.push(_cg_inner(e.k) + ": " + _cg_inner(e.v)) } }; snuff "{" + parts.join(", ") + "}" }
pull _cg_set_new() { snuff {} }
pull _cg_set_of(xs) { roll s = {}; for x in xs { _cg_km_set(s, x, true) }; snuff s }
pull _cg_set_add(s, x) { _cg_km_set(s, x, true); snuff null }
pull _cg_set_remove(s, x) { snuff _cg_km_remove(s, x) != null }
pull _cg_set_union(a, b) { roll s = _cg_km_copy(a); for x in _cg_km_keys(b) { _cg_km_set(s, x, true) }; snuff s }
pull _cg_set_intersection(a, b) { roll s = {}; for x in _cg_km_keys(a) { if _cg_km_has(b, x) { _cg_km_set(s, x, true) } }; snuff s }
pull _cg_set_difference(a, b) { roll s = {}; for x in _cg_km_keys(a) { if not _cg_km_has(b, x) { _cg_km_set(s, x, true) } }; snuff s }
pull _cg_set_show(s) { roll parts = []; for x in _cg_km_keys(s) { parts.push(_cg_inner(x)) }; snuff "Set {" + parts.join(", ") + "}" }
"#;

impl L<'_> {
    /// Does the file write a `Set`, or a `Map<K, V>` whose `K` is not text?
    pub(super) fn writes_keyed(&self) -> bool {
        (0..self.toks.len().saturating_sub(2)).any(|i| {
            (self.is_word(i, "Set") && !self.record_names.contains("Set"))
                || (self.is_word(i, "Map")
                    && matches!(self.k(i + 1), T::Lt)
                    && !self.is_word(i + 2, "Text")
                    && !self.is_word(i + 2, "Path")
                    && self.two_args(i + 1))
        })
    }

    /// Does the `<` at `lt` hold two arguments (a `,` at its own depth)?
    fn two_args(&self, lt: usize) -> bool {
        let mut depth = 0i32;
        for j in lt..self.toks.len() {
            match self.k(j) {
                T::Lt => depth += 1,
                T::Gt => {
                    depth -= 1;
                    if depth == 0 {
                        return false;
                    }
                }
                T::Comma if depth == 1 => return true,
                T::Newline | T::LBrace | T::Assign => return false,
                _ => {}
            }
        }
        false
    }

    /// `Set.new()` and `Set.of(xs)`, written in place of `Set.new` and
    /// `Set.of`.
    pub(super) fn set_constructor(&mut self, j: usize) -> bool {
        if !(self.is_word(j, "Set")
            && !self.record_names.contains("Set")
            && matches!(self.k(j + 1), T::Dot)
            && matches!(self.k(j + 3), T::LParen))
        {
            return false;
        }
        let f = match self.ident(j + 2).as_deref() {
            Some("new") => "_cg_set_new",
            Some("of") => "_cg_set_of",
            _ => return false,
        };
        self.e.replace(self.sp(j).start, self.sp(j + 2).end, f);
        true
    }
}
