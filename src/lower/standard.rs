// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The standard interfaces of the prelude (#38, section 2): `Eq`, `Ord`,
//! `Hash`, `Show`, `Debug`, `Default`, `Clone`, `Add`, `Sub`, `Mul`, `Div`,
//! `Rem`, `Neg` and `Index`, with the built-in types' implementations as
//! ordinary `impl`s of the prelude.
//!
//! A file has a standard interface when it names it (in a bound, an `impl`,
//! a `derive`) and does not declare an interface of that name itself, so a
//! file that never names one lowers exactly as before. The built-in types'
//! methods are CigScript functions appended to the file, named as any
//! `impl`'s are ([`impl_fn_name`]); the dispatchers include them, so a
//! bounded `T` that is an `Int` when the program runs finds its `cmp`.
//!
//! The associated `Out` of the arithmetic interfaces and of `Index` waits on
//! associated types: `add` takes and gives `Self`, and `index` takes and
//! gives `Any`.

use super::{impl_fn_name, L, T};
use crate::types::{At, FnSig, ImplDecl, InterfaceDecl, Mode, Param, Ty};

/// The name of the type parameter `Self`.
const SELF: &str = super::impls::SELF;

fn this() -> Ty {
    Ty::Param(SELF.to_string())
}

fn sig(name: &str, with_self: bool, params: &[(&str, Ty)], ret: Ty) -> FnSig {
    let mut ps = Vec::new();
    if with_self {
        ps.push(Param {
            name: "self".into(),
            ty: None,
            mode: Mode::Shared,
        });
    }
    for (n, t) in params {
        ps.push(Param {
            name: (*n).into(),
            ty: Some(t.clone()),
            mode: Mode::Shared,
        });
    }
    FnSig {
        name: name.into(),
        params: ps,
        ret: Some(ret),
        ..Default::default()
    }
}

/// Each standard interface: its one method, and that method's signature.
pub fn interfaces() -> Vec<InterfaceDecl> {
    let other = || vec![("other", this())];
    let mut out = vec![
        ("Eq", sig("eq", true, &other(), Ty::Bool)),
        ("Ord", sig("cmp", true, &other(), Ty::Int)),
        ("Hash", sig("hash", true, &[], Ty::Text)),
        ("Show", sig("show", true, &[], Ty::Text)),
        ("Debug", sig("debug", true, &[], Ty::Text)),
        ("Default", sig("default", false, &[], this())),
        ("Clone", sig("clone", true, &[], this())),
        ("Neg", sig("neg", true, &[], this())),
        ("Index", sig("index", true, &[("key", Ty::Any)], Ty::Any)),
        // What `derive(Json)` writes (#38); the reader is `Type.from_json`.
        ("Json", sig("to_json", true, &[], Ty::Text)),
    ];
    for (i, m) in ARITH {
        out.push((i, sig(m, true, &other(), this())));
    }
    out.into_iter()
        .map(|(name, m)| InterfaceDecl {
            name: name.into(),
            methods: vec![m],
            defaults: Vec::new(),
            public: true,
            at: At::default(),
        })
        .collect()
}

/// The arithmetic interfaces, their methods and their operators.
pub const ARITH: [(&str, &str); 5] = [
    ("Add", "add"),
    ("Sub", "sub"),
    ("Mul", "mul"),
    ("Div", "div"),
    ("Rem", "rem"),
];

fn list_t() -> Ty {
    Ty::List(Box::new(Ty::Param("T".into())))
}

fn map_v() -> Ty {
    Ty::Map(Box::new(Ty::Param("V".into())))
}

/// A built-in impl: its type parameters with their bounds, its type, and
/// its method's body.
type Builtin = (Vec<(String, Vec<String>)>, Ty, String);

/// The built-in implementations of `iface`: (type parameters with bounds,
/// the type, the body of the method as CigScript after the parameters).
fn builtins(iface: &str) -> Vec<Builtin> {
    // `Path` is text: the `Text` impls are its.
    let scalars = [Ty::Int, Ty::Float, Ty::Bool, Ty::Text];
    let numbers = [Ty::Int, Ty::Float];
    let bounded = |p: &str| vec![(p.to_string(), vec![iface.to_string()])];
    let plain = |p: &str| vec![(p.to_string(), Vec::new())];
    let mut out = Vec::new();
    match iface {
        "Eq" => {
            for t in scalars {
                out.push((vec![], t, "{ snuff self == other }".into()));
            }
            out.push((
                bounded("T"),
                list_t(),
                "{ if self.len() != other.len() { snuff false }; roll i = 0; while i < self.len() { if not _cg_dispatch_Eq_eq(self[i], other[i]) { snuff false }; i = i + 1 }; snuff true }".into(),
            ));
            out.push((
                bounded("V"),
                map_v(),
                "{ if self.len() != other.len() { snuff false }; for k in self.keys() { if not other.has(k) { snuff false }; if not _cg_dispatch_Eq_eq(self[k], other[k]) { snuff false } }; snuff true }".into(),
            ));
        }
        "Ord" => {
            for t in [Ty::Int, Ty::Float, Ty::Text] {
                out.push((
                    vec![],
                    t,
                    "{ if self < other { snuff -1 }; if self > other { snuff 1 }; snuff 0 }".into(),
                ));
            }
            out.push((
                bounded("T"),
                list_t(),
                "{ roll i = 0; while i < self.len() and i < other.len() { stick c = _cg_dispatch_Ord_cmp(self[i], other[i]); if c != 0 { snuff c }; i = i + 1 }; if self.len() < other.len() { snuff -1 }; if self.len() > other.len() { snuff 1 }; snuff 0 }".into(),
            ));
        }
        "Hash" => {
            // A text key no two different values share: the kind, then the
            // value; a list's items each with their length in front.
            for t in scalars.clone() {
                out.push((
                    vec![],
                    t,
                    "{ snuff type_of(self) + \":\" + str(self) }".into(),
                ));
            }
            out.push((
                bounded("T"),
                list_t(),
                "{ roll parts = []; for x in self { stick h = _cg_dispatch_Hash_hash(x); parts.push(str(h.len()) + \":\" + h) }; snuff \"list:\" + parts.join(\"\") }".into(),
            ));
        }
        "Show" | "Debug" => {
            for t in scalars {
                out.push((vec![], t, "{ snuff _cg_show(self) }".into()));
            }
            out.push((plain("T"), list_t(), "{ snuff _cg_show(self) }".into()));
            out.push((plain("V"), map_v(), "{ snuff _cg_show(self) }".into()));
        }
        "Default" => {
            for (t, v) in [
                (Ty::Int, "0"),
                (Ty::Float, "0.0"),
                (Ty::Bool, "false"),
                (Ty::Text, "\"\""),
            ] {
                out.push((vec![], t, format!("{{ snuff {v} }}")));
            }
            out.push((plain("T"), list_t(), "{ snuff [] }".into()));
            out.push((plain("V"), map_v(), "{ snuff {} }".into()));
        }
        "Clone" => {
            for t in scalars {
                out.push((vec![], t, "{ snuff self }".into()));
            }
            out.push((plain("T"), list_t(), "{ snuff self.copy() }".into()));
            out.push((plain("V"), map_v(), "{ snuff self.copy() }".into()));
        }
        "Neg" => {
            for t in numbers {
                out.push((vec![], t, "{ snuff 0 - self }".into()));
            }
        }
        "Index" => {
            out.push((plain("T"), list_t(), "{ snuff self[key] }".into()));
            out.push((plain("V"), map_v(), "{ snuff self[key] }".into()));
        }
        _ => {
            if let Some(op) = arith_op(iface) {
                for t in numbers {
                    out.push((vec![], t, format!("{{ snuff self {op} other }}")));
                }
                if iface == "Add" {
                    out.push((vec![], Ty::Text, "{ snuff self + other }".into()));
                    out.push((plain("T"), list_t(), "{ snuff self + other }".into()));
                }
            }
        }
    }
    out
}

/// The CigScript operator of an arithmetic interface.
pub fn arith_op(iface: &str) -> Option<&'static str> {
    Some(match iface {
        "Add" => "+",
        "Sub" => "-",
        "Mul" => "*",
        "Div" => "/",
        "Rem" => "%",
        _ => return None,
    })
}

impl L<'_> {
    /// The standard interfaces this file names and does not declare itself,
    /// with the built-in types' impls and their functions.
    pub(super) fn standard_interfaces(&mut self) {
        let mut named: std::collections::BTreeSet<String> = self
            .toks
            .iter()
            .filter_map(|t| match &t.kind {
                T::Ident(n) => Some(n.clone()),
                _ => None,
            })
            .collect();
        // A `Set` or a map keyed by something other than text needs its keys'
        // `Hash` and `Eq`, and the functions it is lowered to.
        if self.writes_keyed() {
            named.insert("Hash".into());
            named.insert("Eq".into());
            self.appended.push_str(super::keyed::KEYED);
        }
        for iface in interfaces() {
            if !named.contains(&iface.name) || self.prog.interfaces.contains_key(&iface.name) {
                continue;
            }
            let method = iface.methods[0].clone();
            for (generics, ty, body) in builtins(&iface.name) {
                let f = impl_fn_name(&iface.name, &ty, &method.name);
                let params: Vec<String> = method.params.iter().map(|p| p.name.clone()).collect();
                self.appended
                    .push_str(&format!("pull {f}({}) {body}\n", params.join(", ")));
                // The method as this impl has it: `Self` is the type.
                let mut m = method.clone();
                m.generics = generics.clone();
                for p in m.params.iter_mut() {
                    p.ty = Some(match &p.ty {
                        None => ty.clone(),
                        Some(t) => crate::types::subst_self(t, &ty),
                    });
                }
                m.ret = m.ret.as_ref().map(|t| crate::types::subst_self(t, &ty));
                self.prog.impls.push(ImplDecl {
                    generics,
                    interface: iface.name.clone(),
                    for_ty: ty,
                    methods: vec![m],
                    at: At::default(),
                    builtin: true,
                });
            }
            self.prog.standard.insert(iface.name.clone());
            self.prog.interfaces.insert(iface.name.clone(), iface);
        }
    }
}
