// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `derive` (#38, section 2) and field defaults.
//!
//! `record Point derive(Eq, Ord, Hash, Show, Debug, Default, Clone) { ... }`
//! writes an `impl` of each interface, field by field in declaration order:
//! `Eq` compares each field, `Ord` is lexicographic by field (an enum by
//! variant order, then payload), `Hash` joins each field's key with its
//! length in front, `Show` and `Debug` are the display `exhale` gives,
//! `Default` takes each field's default (written, or its type's), and
//! `Clone` clones each field. Each field's method is picked by its value
//! through the interface's dispatcher. The checker requires every field to
//! implement the interface (`E1232`). `derive(Copy)` (#22 D3 (i)) writes
//! nothing: it makes the type a copy type, which the checker allows only
//! when every field is one. `derive(Json)` waits on its package (`E1102`).
//!
//! What is written is not hidden: `cigar explain --derive` prints it.

use super::{impl_fn_name, L, T};
use crate::types::{At, ImplDecl, Ty};

/// What `derive` takes: the standard interfaces it writes, and `Copy`.
pub const DERIVABLE: [&str; 8] = [
    "Eq", "Ord", "Hash", "Show", "Debug", "Default", "Clone", "Copy",
];

impl L<'_> {
    /// `derive(A, B)` at `j`, if written: each name with where it is, and
    /// the index after the list.
    pub(super) fn derive_list(&mut self, j: usize) -> (Vec<(String, At)>, usize) {
        if !(self.is_word(j, "derive") && matches!(self.k(j + 1), T::LParen)) {
            return (Vec::new(), j);
        }
        let close = self.matching(j + 1);
        let mut out = Vec::new();
        let mut k = j + 2;
        while k < close {
            if let Some(n) = self.ident(k) {
                let at = self.at_of(k, k);
                // `Json(lenient)`: the name, and its options.
                let end = if matches!(self.k(k + 1), T::LParen) {
                    self.matching(k + 1)
                } else {
                    k
                };
                if n == "Json" {
                    self.err_hint(
                        "E1102",
                        "`derive(Json)` is not built yet (otmof-ops/cigar-script#38)",
                        at.start,
                        self.sp(end).end,
                        "write `to_json` and `from_json` by hand until it is",
                    );
                } else if !DERIVABLE.contains(&n.as_str()) {
                    self.err_hint(
                        "E1202",
                        format!("`{n}` cannot be derived"),
                        at.start,
                        at.end,
                        format!("`derive` takes {}", DERIVABLE.join(", ")),
                    );
                } else if out.iter().all(|(x, _): &(String, At)| *x != n) {
                    out.push((n, at));
                }
                k = end + 1;
                continue;
            }
            k += 1;
        }
        (out, close + 1)
    }

    /// The index after a `derive(...)` at `j`, or `j`.
    pub(super) fn past_derive(&self, j: usize) -> usize {
        if self.is_word(j, "derive") && matches!(self.k(j + 1), T::LParen) {
            self.matching(j + 1) + 1
        } else {
            j
        }
    }

    /// A field default after `=` at `j`: a value written out (a number, a
    /// text with no `${...}`, `true`, `false`, `null`, `[]` or `{}`).
    /// Returns it and the index after it.
    pub(super) fn field_default(
        &mut self,
        field: &str,
        j: usize,
        close: usize,
    ) -> (Option<(String, String, Ty)>, usize) {
        let mut end = j;
        while end < close && !matches!(self.k(end), T::Newline | T::Comma) {
            end += 1;
        }
        let text = if end > j {
            self.src[self.sp(j).start..self.sp(end - 1).end]
                .trim()
                .to_string()
        } else {
            String::new()
        };
        let neg = matches!(self.k(j), T::Minus);
        let first = if neg { j + 1 } else { j };
        let ty = match (self.k(first), end - first) {
            (T::Int(_), 1) => Some(Ty::Int),
            (T::Float(_), 1) => Some(Ty::Float),
            (T::Str(parts), 1)
                if !neg && parts.iter().all(|p| matches!(p, super::StrPart::Lit(_))) =>
            {
                Some(Ty::Text)
            }
            (T::True | T::False, 1) if !neg => Some(Ty::Bool),
            (T::Null, 1) if !neg => Some(Ty::Null),
            (T::LBracket, 2) if !neg && matches!(self.k(first + 1), T::RBracket) => {
                Some(Ty::List(Box::new(Ty::Any)))
            }
            (T::LBrace, 2) if !neg && matches!(self.k(first + 1), T::RBrace) => {
                Some(Ty::Map(Box::new(Ty::Any)))
            }
            _ => None,
        };
        match ty {
            Some(t) => (Some((field.to_string(), text, t)), end),
            None => {
                let sp = self.sp(j);
                self.err_hint(
                    "E1202",
                    format!("the default of `{field}` is not a value written out"),
                    sp.start,
                    self.sp(end.saturating_sub(1).max(j)).end,
                    "a default is a number, a text, `true`, `false`, `null`, `[]` or `{}`",
                );
                (None, end)
            }
        }
    }

    /// `Point.default()`: the `Default` impl of the record or enum `name`
    /// (#38), written in place of `Point.default`.
    pub(super) fn type_default(&mut self, j: usize, name: &str) -> bool {
        if !(matches!(self.k(j + 1), T::Dot)
            && self.is_word(j + 2, "default")
            && matches!(self.k(j + 3), T::LParen)
            && self.prog.interfaces.contains_key("Default"))
        {
            return false;
        }
        let f = impl_fn_name("Default", &self.pattern_of(name), "default");
        self.e.replace(self.sp(j).start, self.sp(j + 2).end, f);
        true
    }

    /// The type an `impl` for the record or enum `name` is written for: its
    /// own type parameters as they are declared.
    fn pattern_of(&self, name: &str) -> Ty {
        if let Some(r) = self.prog.records.get(name) {
            return Ty::Record(
                name.to_string(),
                r.generics.iter().map(|g| Ty::Param(g.clone())).collect(),
            );
        }
        if let Some(e) = self.prog.enums.get(name) {
            return Ty::Enum(
                name.to_string(),
                e.generics.iter().map(|g| Ty::Param(g.clone())).collect(),
            );
        }
        Ty::Record(name.to_string(), Vec::new())
    }

    /// The call that gives a default value of type `t`, for `Default`.
    fn default_of(&self, t: &Ty) -> String {
        let d = |t: &Ty| format!("{}()", impl_fn_name("Default", t, "default"));
        match t {
            Ty::Int | Ty::Float | Ty::Bool | Ty::Text => d(t),
            Ty::Path => d(&Ty::Text),
            Ty::Sized(_) => "0".into(),
            Ty::List(_) | Ty::ListView(_) => "[]".into(),
            Ty::Map(_) => "{}".into(),
            Ty::Record(n, _) | Ty::Enum(n, _) => d(&self.pattern_of(n)),
            _ => "null".into(),
        }
    }

    /// Every `derive` of this file's records and enums: the impls' functions,
    /// appended, and their `ImplDecl`s.
    pub(super) fn derive_impls(&mut self) {
        let mut made: Vec<(ImplDecl, String)> = Vec::new();
        let mut copies = false;
        for r in self.prog.records.values() {
            let me = self.pattern_of(&r.name);
            for (iface, at) in &r.derives {
                if iface == "Copy" {
                    copies = true;
                    continue;
                }
                let fields: Vec<&str> = r.fields.iter().map(|(f, _)| f.as_str()).collect();
                let body = match iface.as_str() {
                    "Eq" => {
                        let mut b = String::new();
                        for f in &fields {
                            b.push_str(&format!(
                                "if not _cg_d_eq(self.{f}, other.{f}) {{ snuff false }}; "
                            ));
                        }
                        format!("{{ {b}snuff true }}")
                    }
                    "Ord" => {
                        let mut b = String::from("roll c = 0; ");
                        for f in &fields {
                            b.push_str(&format!(
                                "c = _cg_d_cmp(self.{f}, other.{f}); if c != 0 {{ snuff c }}; "
                            ));
                        }
                        format!("{{ {b}snuff 0 }}")
                    }
                    "Hash" => {
                        let mut b = format!("roll out = \"{}\"; ", r.name);
                        for (k, f) in fields.iter().enumerate() {
                            b.push_str(&format!(
                                "stick h{k} = _cg_d_hash(self.{f}); out = out + \":\" + str(h{k}.len()) + \":\" + h{k}; "
                            ));
                        }
                        format!("{{ {b}snuff out }}")
                    }
                    "Show" | "Debug" => "{ snuff _cg_show(self) }".to_string(),
                    "Default" => {
                        let vals: Vec<String> = r
                            .fields
                            .iter()
                            .map(|(f, t)| {
                                let v = r
                                    .defaults
                                    .iter()
                                    .find(|d| &d.0 == f)
                                    .map(|d| d.1.clone())
                                    .unwrap_or_else(|| self.default_of(t));
                                format!("{f}: {v}")
                            })
                            .collect();
                        format!("{{ snuff _cg_new_{}({{{}}}) }}", r.name, vals.join(", "))
                    }
                    "Clone" => {
                        let vals: Vec<String> = fields
                            .iter()
                            .map(|f| format!("{f}: _cg_d_clone(self.{f})"))
                            .collect();
                        format!("{{ snuff _cg_new_{}({{{}}}) }}", r.name, vals.join(", "))
                    }
                    _ => continue,
                };
                made.push(self.derived_impl(iface, &me, &r.generics, *at, &body));
            }
        }
        for e in self.prog.enums.values() {
            let me = self.pattern_of(&e.name);
            for (iface, at) in &e.derives {
                let per_variant = |line: &dyn Fn(&str) -> String| -> String {
                    let mut b = String::new();
                    for v in e.variants.iter().filter(|v| !v.fields.is_empty()) {
                        let inner: String = v.fields.iter().map(|(f, _)| line(f)).collect();
                        b.push_str(&format!(
                            "if self.__variant == \"{}\" {{ {inner}}}; ",
                            v.name
                        ));
                    }
                    b
                };
                let body = match iface.as_str() {
                    "Copy" => {
                        copies = true;
                        continue;
                    }
                    "Eq" => format!(
                        "{{ if self.__variant != other.__variant {{ snuff false }}; {}snuff true }}",
                        per_variant(&|f| format!(
                            "if not _cg_d_eq(self.{f}, other.{f}) {{ snuff false }}; "
                        ))
                    ),
                    "Ord" => {
                        let order: Vec<String> =
                            e.variants.iter().map(|v| format!("\"{}\"", v.name)).collect();
                        format!(
                            "{{ stick order = [{}]; stick a = order.index_of(self.__variant); stick b = order.index_of(other.__variant); if a != b {{ snuff a - b }}; roll c = 0; {}snuff 0 }}",
                            order.join(", "),
                            per_variant(&|f| format!(
                                "c = _cg_d_cmp(self.{f}, other.{f}); if c != 0 {{ snuff c }}; "
                            ))
                        )
                    }
                    "Hash" => format!(
                        "{{ roll out = \"{}.\" + self.__variant; {}snuff out }}",
                        e.name,
                        per_variant(&|f| format!(
                            "out = out + \":\" + str(_cg_d_hash(self.{f}).len()) + \":\" + _cg_d_hash(self.{f}); "
                        ))
                    ),
                    "Show" | "Debug" => "{ snuff _cg_show(self) }".to_string(),
                    "Clone" => "{ snuff self.copy() }".to_string(),
                    // `Default` on an enum: the checker says why not (E1232).
                    _ => continue,
                };
                made.push(self.derived_impl(iface, &me, &e.generics, *at, &body));
            }
        }
        // A field may be optional: `null` is equal only to `null`, orders
        // first, has its own key and is its own clone.
        let used: std::collections::BTreeSet<String> =
            made.iter().map(|(im, _)| im.interface.clone()).collect();
        for (iface, helper) in FIELD_HELPERS {
            if used.contains(iface) {
                self.appended.push_str(helper);
            }
        }
        for (im, text) in made {
            self.prog
                .derived
                .push((im.for_ty.to_string(), im.interface.clone(), text.clone()));
            self.appended.push_str(&text);
            self.prog.impls.push(im);
        }
        if copies {
            // A copy type's value is copied where it is bound or passed: its
            // fields are copies, so one level per record or enum suffices.
            self.appended.push_str(COPY_VALUE);
        }
    }

    /// The `impl` `derive(iface)` writes for `me`, and its function.
    fn derived_impl(
        &self,
        iface: &str,
        me: &Ty,
        generics: &[String],
        at: At,
        body: &str,
    ) -> (ImplDecl, String) {
        let Some(i) = self.prog.interfaces.get(iface) else {
            return (
                ImplDecl {
                    generics: vec![],
                    interface: iface.into(),
                    for_ty: me.clone(),
                    methods: vec![],
                    at,
                    builtin: false,
                },
                String::new(),
            );
        };
        let mut m = i.methods[0].clone();
        let params: Vec<String> = m.params.iter().map(|p| p.name.clone()).collect();
        for p in m.params.iter_mut() {
            p.ty = Some(match &p.ty {
                None => me.clone(),
                Some(t) => crate::types::subst_self(t, me),
            });
        }
        m.ret = m.ret.as_ref().map(|t| crate::types::subst_self(t, me));
        let gens: Vec<(String, Vec<String>)> = generics
            .iter()
            .map(|g| (g.clone(), vec![iface.to_string()]))
            .collect();
        m.generics = gens.clone();
        let f = impl_fn_name(iface, me, &m.name);
        let text = format!("pull {f}({}) {body}\n", params.join(", "));
        (
            ImplDecl {
                generics: gens,
                interface: iface.into(),
                for_ty: me.clone(),
                methods: vec![m],
                at,
                builtin: false,
            },
            text,
        )
    }
}

/// Copy a value of a copy type: a record or an enum is a map whose fields
/// are copies; anything else is its own copy.
pub const COPY_VALUE: &str = "pull _cg_copy_value(v) { if type_of(v) != \"map\" { snuff v }; roll out = {}; for k in v.keys() { out[k] = _cg_copy_value(v[k]) }; snuff out }\n";

/// What a derived method calls on each field: the interface's dispatcher,
/// with `null` handled first.
const FIELD_HELPERS: [(&str, &str); 4] = [
    ("Eq", "pull _cg_d_eq(a, b) { if a == null or b == null { snuff a == b }; snuff _cg_dispatch_Eq_eq(a, b) }\n"),
    ("Ord", "pull _cg_d_cmp(a, b) { if a == null and b == null { snuff 0 }; if a == null { snuff -1 }; if b == null { snuff 1 }; snuff _cg_dispatch_Ord_cmp(a, b) }\n"),
    ("Hash", "pull _cg_d_hash(v) { if v == null { snuff \"null\" }; snuff _cg_dispatch_Hash_hash(v) }\n"),
    ("Clone", "pull _cg_d_clone(v) { if v == null { snuff null }; snuff _cg_dispatch_Clone_clone(v) }\n"),
];
