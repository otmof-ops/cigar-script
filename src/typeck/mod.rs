// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! CigarScript's static type checker (#8), on cig's AST of the lowered text.
//!
//! The checker is conservative by construction: a type it cannot work out is
//! `Any`, and `Any` never produces an error. It reports only what it can
//! prove wrong (docs: SPEC section 4, codes E1201-E1218, E1311-E1313).

pub mod stdlib;

use crate::types::{self, At, SizedInt, Ty};
use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::ast::*;
use cigscript::syntax::span::Span;
use std::collections::{HashMap, HashSet};

/// Check a lowered program. `original` maps a lowered byte offset to the original file's.
///
/// Every diagnostic's span carries ORIGINAL byte offsets (`start`/`end`); its
/// `line` is the lowered line (lowering keeps lines) and its `col` is the
/// lowered column, which the caller may recompute from the original text.
pub fn check(
    ast: &Program,
    prog: &types::Program,
    original: &dyn Fn(usize) -> usize,
) -> Vec<Diagnostic> {
    check_all(ast, prog, original).errors
}

/// What checking found: errors, warnings, and the rewrites the types decide
/// (which layer `measure x` means, which `impl` a method call reaches, the
/// range checks of sized numbers), as edits to the lowered text.
#[derive(Default)]
pub struct Output {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    pub resolutions: Vec<Resolution>,
}

/// One rewrite of the lowered text, at lowered offsets.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// Replace `[start, end)` (a callee's name) with `text`.
    Rename {
        start: usize,
        end: usize,
        text: String,
    },
    /// `recv.m(args)` becomes `func(recv, args)`: the receiver is
    /// `[recv_start, recv_end)` and the call ends at `call_end`.
    Method {
        recv_start: usize,
        recv_end: usize,
        call_end: usize,
        func: String,
        has_args: bool,
        /// Written `recv?.m(...)`: nothing is called on null.
        safe: bool,
    },
    /// Wrap `[start, end)` in `pre` and `post`.
    Wrap {
        start: usize,
        end: usize,
        pre: String,
        post: String,
    },
    /// Insert `text` at `at`.
    Insert { at: usize, text: String },
}

/// Check a lowered program, and work out its rewrites.
pub fn check_all(
    ast: &Program,
    prog: &types::Program,
    original: &dyn Fn(usize) -> usize,
) -> Output {
    let prog = normalize(prog);
    let mut ck = Ck::new(&prog, original);
    ck.validate_decls();
    ck.check_impls();
    ck.scopes.push(HashMap::new());
    ck.stmts(&ast.body);
    Output {
        errors: ck.diags,
        warnings: ck.warnings,
        resolutions: ck.resolutions,
    }
}

// ---------------------------------------------------------------- types

/// Record and enum names are resolved by what is declared.
fn norm_ty(t: &Ty, p: &types::Program) -> Ty {
    let go = |t: &Ty| norm_ty(t, p);
    match t {
        Ty::List(a) => Ty::List(Box::new(go(a))),
        Ty::Map(a) => Ty::Map(Box::new(go(a))),
        Ty::Opt(a) => Ty::Opt(Box::new(go(a))),
        Ty::Channel(a) => Ty::Channel(Box::new(go(a))),
        Ty::Fn(ps, r) => Ty::Fn(ps.iter().map(go).collect(), Box::new(go(r))),
        Ty::Record(n, a) | Ty::Enum(n, a) => {
            let a: Vec<Ty> = a.iter().map(go).collect();
            if p.enums.contains_key(n) && !p.records.contains_key(n) {
                Ty::Enum(n.clone(), a)
            } else {
                Ty::Record(n.clone(), a)
            }
        }
        o => o.clone(),
    }
}

fn normalize(p: &types::Program) -> types::Program {
    let mut q = p.clone();
    let n = |t: &Ty| norm_ty(t, p);
    for r in q.records.values_mut() {
        for f in r.fields.iter_mut() {
            f.1 = n(&f.1);
        }
    }
    for e in q.enums.values_mut() {
        for v in e.variants.iter_mut() {
            for f in v.fields.iter_mut() {
                f.1 = n(&f.1);
            }
        }
    }
    let sig = |s: &mut types::FnSig| {
        for pa in s.params.iter_mut() {
            pa.ty = pa.ty.as_ref().map(n);
        }
        s.ret = s.ret.as_ref().map(n);
        s.coughs = s.coughs.iter().map(n).collect();
    };
    for s in q.fns.values_mut() {
        sig(s);
    }
    for i in q.interfaces.values_mut() {
        i.methods.iter_mut().for_each(sig);
    }
    for i in q.impls.iter_mut() {
        i.for_ty = n(&i.for_ty);
        i.methods.iter_mut().for_each(sig);
    }
    for b in q.bindings.values_mut() {
        *b = n(b);
    }
    for l in q.lambdas.values_mut() {
        for t in l.iter_mut() {
            *t = t.as_ref().map(n);
        }
    }
    q
}

fn assignable(from: &Ty, to: &Ty) -> bool {
    use Ty::*;
    match (from, to) {
        (Any, _) | (_, Any) | (Never, _) | (Bytes, _) | (_, Bytes) => true,
        (Opt(a), Opt(b)) => assignable(a, b),
        (Null, Opt(_)) | (Null, Null) => true,
        (f, Opt(b)) => assignable(f, b),
        (Opt(_), _) => false,
        (Param(a), Param(b)) => a == b,
        (Param(_), _) | (_, Param(_)) => false,
        (Int | Sized(_), Int | Sized(_) | Float) => true,
        (Float, Float | Sized(_)) => true,
        (Text | Path, Text | Path) => true,
        (Bool, Bool) => true,
        (List(a), List(b)) | (Map(a), Map(b)) | (Channel(a), Channel(b)) => assignable(a, b),
        (Fn(pa, ra), Fn(pb, rb)) => {
            pa.len() == pb.len()
                && pa.iter().zip(pb).all(|(a, b)| assignable(b, a))
                && assignable(ra, rb)
        }
        (Record(a, aa), Record(b, ab)) | (Enum(a, aa), Enum(b, ab)) => {
            a == b
                && (aa.is_empty()
                    || ab.is_empty()
                    || (aa.len() == ab.len() && aa.iter().zip(ab).all(|(x, y)| assignable(x, y))))
        }
        _ => false,
    }
}

fn same_ty(a: &Ty, b: &Ty) -> bool {
    !matches!(a, Ty::Any) && !matches!(b, Ty::Any) && assignable(a, b) && assignable(b, a)
}

/// The least type both fit in; `Any` when there is none.
fn join(a: &Ty, b: &Ty) -> Ty {
    match (a, b) {
        (Ty::Null, Ty::Null) => Ty::Null,
        (Ty::Null, Ty::Opt(_)) => b.clone(),
        (Ty::Opt(_), Ty::Null) => a.clone(),
        (Ty::Null, Ty::Any) | (Ty::Any, Ty::Null) => Ty::Any,
        (Ty::Null, t) | (t, Ty::Null) => Ty::Opt(Box::new(t.clone())),
        (Ty::Any, _) | (_, Ty::Any) => Ty::Any,
        _ if assignable(a, b) => b.clone(),
        _ if assignable(b, a) => a.clone(),
        _ => Ty::Any,
    }
}

fn unify(pat: &Ty, act: &Ty, gens: &[String], b: &mut HashMap<String, Ty>) {
    match (pat, act) {
        (Ty::Param(n), _) if gens.contains(n) => {
            let t = match b.get(n) {
                Some(old) => join(old, act),
                None => act.clone(),
            };
            b.insert(n.clone(), t);
        }
        (Ty::Opt(p), Ty::Opt(a)) => unify(p, a, gens, b),
        (Ty::Opt(_), Ty::Null) => {}
        (Ty::Opt(p), a) => unify(p, a, gens, b),
        (Ty::List(p), Ty::List(a))
        | (Ty::Map(p), Ty::Map(a))
        | (Ty::Channel(p), Ty::Channel(a)) => unify(p, a, gens, b),
        (Ty::Fn(pp, pr), Ty::Fn(ap, ar)) => {
            for (p, a) in pp.iter().zip(ap) {
                unify(p, a, gens, b);
            }
            unify(pr, ar, gens, b);
        }
        (Ty::Record(n, pa), Ty::Record(m, aa)) | (Ty::Enum(n, pa), Ty::Enum(m, aa)) if n == m => {
            for (p, a) in pa.iter().zip(aa) {
                unify(p, a, gens, b);
            }
        }
        _ => {}
    }
}

/// Replace the call's own type parameters; unbound ones become `Any`.
fn subst(t: &Ty, gens: &[String], b: &HashMap<String, Ty>) -> Ty {
    let go = |t: &Ty| subst(t, gens, b);
    match t {
        Ty::Param(n) if gens.contains(n) => b.get(n).cloned().unwrap_or(Ty::Any),
        Ty::List(a) => Ty::List(Box::new(go(a))),
        Ty::Map(a) => Ty::Map(Box::new(go(a))),
        Ty::Opt(a) => match go(a) {
            Ty::Opt(i) => Ty::Opt(i),
            Ty::Any => Ty::Any,
            o => Ty::Opt(Box::new(o)),
        },
        Ty::Channel(a) => Ty::Channel(Box::new(go(a))),
        Ty::Fn(ps, r) => Ty::Fn(ps.iter().map(go).collect(), Box::new(go(r))),
        Ty::Record(n, a) => Ty::Record(n.clone(), a.iter().map(go).collect()),
        Ty::Enum(n, a) => Ty::Enum(n.clone(), a.iter().map(go).collect()),
        o => o.clone(),
    }
}

fn lit_str(e: &Expr) -> Option<String> {
    if let ExprKind::Str(parts) = &e.kind {
        let mut s = String::new();
        for p in parts {
            match p {
                StrPiece::Lit(l) => s.push_str(l),
                _ => return None,
            }
        }
        Some(s)
    } else {
        None
    }
}

/// Read a runtime descriptor (`{k: "list", of: {k: "int"}}`) back into a type.
fn decode(e: &Expr) -> Option<Ty> {
    let ExprKind::Map(entries) = &e.kind else {
        return None;
    };
    let get = |k: &str| {
        entries.iter().find_map(|(key, v)| match key {
            MapKey::Static(s) if s == k => Some(v),
            _ => None,
        })
    };
    let inner = || get("of").and_then(decode).unwrap_or(Ty::Any);
    let name = || get("name").and_then(lit_str).unwrap_or_default();
    Some(match lit_str(get("k")?)?.as_str() {
        "int" => Ty::Int,
        "float" => Ty::Float,
        "text" => Ty::Text,
        "bool" => Ty::Bool,
        "null" => Ty::Null,
        "list" => Ty::List(Box::new(inner())),
        "map" => Ty::Map(Box::new(inner())),
        "opt" => match inner() {
            Ty::Any => Ty::Any,
            t => Ty::Opt(Box::new(t)),
        },
        "record" => Ty::Record(name(), vec![]),
        "enum" => Ty::Enum(name(), vec![]),
        "sized" => SizedInt::ALL
            .iter()
            .find(|(n, _)| *n == name())
            .map(|(_, s)| Ty::Sized(*s))
            .unwrap_or(Ty::Any),
        _ => Ty::Any,
    })
}

// ---------------------------------------------------------------- state

#[derive(Clone)]
struct Var {
    /// The type here (narrowed inside an `if x != null`).
    ty: Ty,
    /// The type it was declared (or widened) to.
    declared: Ty,
    mutable: bool,
    /// Written by the programmer (a binding in `Program::bindings`, a typed
    /// parameter): assignments are checked against it.
    annotated: bool,
}

struct FnCtx {
    name: String,
    ret: Option<Ty>,
    coughs: Vec<Ty>,
    try_depth: usize,
}

struct Ck<'a> {
    prog: &'a types::Program,
    original: &'a dyn Fn(usize) -> usize,
    diags: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    resolutions: Vec<Resolution>,
    /// How many `burn` blocks enclose the statement being checked.
    burn_depth: usize,
    /// The value of a `stick` or `roll` whose arithmetic a statement guard
    /// checks, so it is not wrapped as well.
    guarded: Option<(usize, usize)>,
    /// The end of the statement being checked.
    stmt_end: Option<usize>,
    /// Helpers declared in place so far.
    locals: usize,
    /// The argument types of the last call `apply` checked.
    last_args: Vec<Ty>,
    scopes: Vec<HashMap<String, Var>>,
    fns: HashMap<String, types::FnSig>,
    impl_self: HashMap<String, Ty>,
    ctx: Vec<FnCtx>,
    seen: HashSet<(&'static str, usize, usize)>,
}

impl<'a> Ck<'a> {
    fn new(prog: &'a types::Program, original: &'a dyn Fn(usize) -> usize) -> Self {
        let mut fns = HashMap::new();
        for s in prog.fns.values() {
            fns.insert(s.name.clone(), s.clone());
        }
        let mut impl_self = HashMap::new();
        for im in &prog.impls {
            for m in &im.methods {
                let n = types::impl_fn_name(&im.interface, &im.for_ty, &m.name);
                impl_self.insert(n.clone(), im.for_ty.clone());
                let mut s = m.clone();
                for p in s.params.iter_mut() {
                    if p.name == "self" && p.ty.is_none() {
                        p.ty = Some(im.for_ty.clone());
                    }
                }
                fns.entry(n).or_insert(s);
            }
        }
        Ck {
            prog,
            original,
            diags: vec![],
            warnings: vec![],
            resolutions: vec![],
            burn_depth: 0,
            guarded: None,
            stmt_end: None,
            locals: 0,
            last_args: Vec::new(),
            scopes: vec![],
            fns,
            impl_self,
            ctx: vec![],
            seen: HashSet::new(),
        }
    }

    // ------------------------------------------------------ diagnostics

    fn push(
        &mut self,
        code: &'static str,
        msg: String,
        span: Option<Span>,
        at: Option<At>,
        hint: Option<String>,
    ) {
        let (s, e) = match (span, at) {
            (Some(sp), _) => ((self.original)(sp.start), (self.original)(sp.end)),
            (None, Some(a)) => (a.start, a.end),
            _ => (0, 0),
        };
        if !self.seen.insert((code, s, e)) {
            return;
        }
        let mut d = Diagnostic::new(Kind::Check, msg);
        d.code = code;
        d.hint = hint;
        let (line, col) = span.map(|sp| (sp.line, sp.col)).unwrap_or((0, 0));
        d.span = Some(Span::new(s, e.max(s), line, col));
        if line > 0 {
            d.line = Some(line);
            d.col = Some(col);
        }
        self.diags.push(d);
    }

    fn err(&mut self, code: &'static str, msg: String, span: Span, hint: Option<String>) {
        self.push(code, msg, Some(span), None, hint);
    }

    /// A helper declared at the end of the statement being checked, so what
    /// it coughs is reported on that statement's line. `pull` is its whole
    /// text, given its name.
    fn local(&mut self, pull: impl FnOnce(&str) -> String) -> Option<String> {
        let end = self.stmt_end?;
        self.locals += 1;
        let name = format!("__cg_l{}", self.locals);
        let text = format!("; {}", pull(&name));
        self.resolutions.push(Resolution::Insert { at: end, text });
        Some(name)
    }

    /// Check, when the program runs, that `e` fits `t`; a value that does not
    /// coughs on this statement's line.
    fn check_here(&mut self, e: &Expr, t: &Ty, what: &str) {
        let d = crate::lower::ty::descriptor(t);
        let f = self.local(|n| {
            format!("pull {n}(v) {{ if not _cg_fits(v, {d}) {{ cough _cg_why(v, {d}, \"{what}\") }}; snuff v }}")
        });
        if let Some(f) = f {
            self.resolutions.push(Resolution::Wrap {
                start: e.span.start,
                end: e.span.end,
                pre: format!("{f}("),
                post: ")".into(),
            });
        }
    }

    fn warn(&mut self, code: &'static str, msg: String, span: Span, hint: Option<String>) {
        let before = self.diags.len();
        self.push(code, msg, Some(span), None, hint);
        if self.diags.len() > before {
            let d = self.diags.pop().expect("just pushed");
            self.warnings.push(d);
        }
    }

    /// Report `found` not fitting `want` (E1209 for null into a non-optional).
    fn expect(
        &mut self,
        e: &Expr,
        found: &Ty,
        want: &Ty,
        code: &'static str,
        hint: String,
    ) -> bool {
        // A number written out that a sized type cannot hold.
        if let (Ty::Sized(sz), Some(v)) = (strip_opt(want), int_literal(e)) {
            let (lo, hi) = sz.range();
            if v < lo || v > hi {
                self.err(
                    "E1510",
                    format!("`{v}` is outside `{}` ({lo} to {hi})", sz.name()),
                    e.span,
                    Some(hint),
                );
                return false;
            }
        }
        if assignable(found, want) {
            return true;
        }
        // A list or map written out: point at the element that does not fit.
        let elements: Vec<&Expr> = match (&e.kind, strip_opt(want)) {
            (ExprKind::List(items), Ty::List(_)) => items.iter().collect(),
            (ExprKind::Map(entries), Ty::Map(_)) => entries.iter().map(|(_, v)| v).collect(),
            _ => vec![],
        };
        if let Ty::List(el) | Ty::Map(el) = strip_opt(want) {
            let mut reported = false;
            for x in elements {
                if let Some(t) = literal_ty(x) {
                    if !assignable(&t, &el) {
                        self.expect(x, &t, &el, code, hint.clone());
                        reported = true;
                    }
                }
            }
            if reported {
                return false;
            }
        }
        let (code, msg) = if matches!(found, Ty::Null) {
            ("E1209", format!("`null` where `{want}` is not optional"))
        } else {
            (code, format!("expected `{want}`, found `{found}`"))
        };
        let mut hint = hint;
        if let Ty::Opt(inner) = found {
            if assignable(inner, want) {
                hint.push_str("; the value may be null, so check it with `!= null` first");
            }
        }
        self.err(code, msg, e.span, Some(hint));
        false
    }

    // ------------------------------------------------------ declarations

    fn validate_ty(&mut self, t: &Ty, at: At, span: Option<Span>, generics: &[String]) {
        let prog = self.prog;
        match t {
            Ty::List(a) | Ty::Map(a) | Ty::Opt(a) | Ty::Channel(a) => {
                self.validate_ty(a, at, span, generics)
            }
            Ty::Fn(ps, r) => {
                for p in ps {
                    self.validate_ty(p, at, span, generics);
                }
                self.validate_ty(r, at, span, generics);
            }
            Ty::Record(n, a) | Ty::Enum(n, a) => {
                let want = prog
                    .records
                    .get(n)
                    .map(|r| r.generics.len())
                    .or_else(|| prog.enums.get(n).map(|e| e.generics.len()));
                match want {
                    None if generics.contains(n) => {}
                    None => self.push(
                        "E1202",
                        format!("unknown type `{n}`"),
                        span,
                        Some(at),
                        Some("declare it with `record` or `enum`, or check the spelling".into()),
                    ),
                    Some(w) if w != a.len() && !a.is_empty() => self.push(
                        "E1203",
                        format!("`{n}` takes {w} type argument(s), given {}", a.len()),
                        span,
                        Some(at),
                        None,
                    ),
                    _ => {}
                }
                for x in a {
                    self.validate_ty(x, at, span, generics);
                }
            }
            _ => {}
        }
    }

    fn validate_decls(&mut self) {
        let prog = self.prog;
        for r in prog.records.values() {
            for (_, t) in &r.fields {
                self.validate_ty(t, r.at, None, &r.generics);
            }
        }
        for e in prog.enums.values() {
            for v in &e.variants {
                for (_, t) in &v.fields {
                    self.validate_ty(t, v.at, None, &e.generics);
                }
            }
        }
        for s in prog.fns.values() {
            let g: Vec<String> = s.generics.iter().map(|g| g.0.clone()).collect();
            for p in &s.params {
                if let Some(t) = &p.ty {
                    self.validate_ty(t, s.at, None, &g);
                }
            }
            if let Some(t) = &s.ret {
                self.validate_ty(t, s.at, None, &g);
            }
            for t in &s.coughs {
                self.validate_ty(t, s.at, None, &g);
            }
        }
    }

    fn sig_text(s: &types::FnSig) -> String {
        let ps: Vec<String> = s
            .params
            .iter()
            .map(|p| match &p.ty {
                Some(t) => t.to_string(),
                None if p.name == "self" => "self".into(),
                None => "Any".into(),
            })
            .collect();
        format!(
            "({}) -> {}",
            ps.join(", "),
            s.ret
                .as_ref()
                .map(|t| t.to_string())
                .unwrap_or_else(|| "Any".into())
        )
    }

    /// E1311 (a method missing from an `impl`) and E1312 (its signature differs).
    fn check_impls(&mut self) {
        let prog = self.prog;
        for im in &prog.impls {
            let Some(iface) = prog.interfaces.get(&im.interface) else {
                continue;
            };
            for want in &iface.methods {
                let Some(have) = im.methods.iter().find(|m| m.name == want.name) else {
                    self.push(
                        "E1311",
                        format!(
                            "`impl {} for {}` is missing the method `{}`",
                            im.interface, im.for_ty, want.name
                        ),
                        None,
                        Some(im.at),
                        Some(format!(
                            "`{}` declares `{}{}`",
                            im.interface,
                            want.name,
                            Self::sig_text(want)
                        )),
                    );
                    continue;
                };
                let ty_ok = |a: &Option<Ty>, b: &Option<Ty>| match (a, b) {
                    (Some(a), Some(b)) => assignable(a, b) && assignable(b, a),
                    _ => true,
                };
                let same = want.params.len() == have.params.len()
                    && want
                        .params
                        .iter()
                        .zip(&have.params)
                        .all(|(a, b)| ty_ok(&a.ty, &b.ty))
                    && ty_ok(&want.ret, &have.ret);
                if !same {
                    self.push(
                        "E1312",
                        format!(
                            "`{}` in `impl {} for {}` is {}, the interface says {}",
                            want.name,
                            im.interface,
                            im.for_ty,
                            Self::sig_text(have),
                            Self::sig_text(want)
                        ),
                        None,
                        Some(have.at),
                        Some(format!(
                            "expected `{}` here, because of the signature in `{}`",
                            Self::sig_text(want),
                            im.interface
                        )),
                    );
                }
            }
        }
    }

    // ------------------------------------------------------ scopes

    fn find_var(&self, n: &str) -> Option<&Var> {
        self.scopes.iter().rev().find_map(|s| s.get(n))
    }

    fn define(&mut self, n: &str, ty: Ty, mutable: bool, annotated: bool) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(
                n.to_string(),
                Var {
                    declared: ty.clone(),
                    ty,
                    mutable,
                    annotated,
                },
            );
        }
    }

    fn narrow(&mut self, n: &str, ty: Ty) {
        if let Some(v) = self.find_var(n).cloned() {
            if let Some(s) = self.scopes.last_mut() {
                s.insert(n.to_string(), Var { ty, ..v });
            }
        }
    }

    fn set_all(&mut self, n: &str, f: impl Fn(&mut Var)) {
        for s in self.scopes.iter_mut() {
            if let Some(v) = s.get_mut(n) {
                f(v);
            }
        }
    }

    fn reset_narrowing(&mut self, names: &HashSet<String>) {
        for n in names {
            self.set_all(n, |v| v.ty = v.declared.clone());
        }
    }

    /// Facts a condition makes known: `x != null` narrows `x: T?` to `T`.
    fn facts(&self, c: &Expr, positive: bool) -> Vec<(String, Ty)> {
        match &c.kind {
            ExprKind::Unary {
                op: UnOp::Not,
                expr,
            } => self.facts(expr, !positive),
            ExprKind::Binary {
                op: BinOp::And,
                lhs,
                rhs,
            } if positive => {
                let mut f = self.facts(lhs, true);
                f.extend(self.facts(rhs, true));
                f
            }
            ExprKind::Binary {
                op: BinOp::Or,
                lhs,
                rhs,
            } if !positive => {
                let mut f = self.facts(lhs, false);
                f.extend(self.facts(rhs, false));
                f
            }
            ExprKind::Binary {
                op: op @ (BinOp::Ne | BinOp::Eq),
                lhs,
                rhs,
            } => {
                let non_null = (*op == BinOp::Ne) == positive;
                if !non_null {
                    return vec![];
                }
                let name = match (&lhs.kind, &rhs.kind) {
                    (ExprKind::Ident(n), ExprKind::Null) | (ExprKind::Null, ExprKind::Ident(n)) => {
                        n
                    }
                    _ => return vec![],
                };
                match self.find_var(name).map(|v| &v.ty) {
                    Some(Ty::Opt(inner)) => vec![(name.clone(), (**inner).clone())],
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    fn apply_facts(&mut self, f: Vec<(String, Ty)>) {
        for (n, t) in f {
            self.narrow(&n, t);
        }
    }

    // ------------------------------------------------------ statements

    fn block(&mut self, b: &Block) {
        self.scopes.push(HashMap::new());
        self.stmts(&b.stmts);
        self.scopes.pop();
    }

    fn stmts(&mut self, ss: &[Stmt]) {
        for s in ss {
            self.stmt(s);
        }
    }

    fn unwrap_check<'e>(&self, e: &'e Expr) -> Option<(&'e Expr, Option<Ty>)> {
        if let ExprKind::Call { callee, args } = &e.kind {
            if let ExprKind::Ident(n) = &callee.kind {
                if n == "_cg_check" && args.len() == 3 && self.find_var(n).is_none() {
                    return Some((&args[0], decode(&args[1])));
                }
            }
        }
        None
    }

    fn diverges(b: &Block) -> bool {
        matches!(
            b.stmts.last(),
            Some(Stmt::Snuff { .. } | Stmt::Cough { .. } | Stmt::Break(_) | Stmt::Continue(_))
        )
    }

    fn stmt(&mut self, s: &Stmt) {
        // Where a check that coughs can be declared for this statement.
        let outer = if matches!(s, Stmt::Pull(_)) {
            self.stmt_end
        } else {
            std::mem::replace(&mut self.stmt_end, stmt_tail(s))
        };
        self.stmt_inner(s);
        self.stmt_end = outer;
    }

    fn stmt_inner(&mut self, s: &Stmt) {
        match s {
            Stmt::Declare {
                name,
                mutable,
                value,
                span,
            } => {
                let bind = self.prog.bindings.get(&span.start).cloned();
                if let Some(b) = &bind {
                    self.validate_ty(b, At::default(), Some(*span), &[]);
                }
                let (ty, annotated) = if let Some((inner, d)) = self.unwrap_check(value) {
                    match bind.or(d) {
                        Some(dt) => {
                            let it = self.infer(inner);
                            let hint = format!(
                                "expected `{dt}` here, because `{name}` is declared as `{dt}`"
                            );
                            self.expect(inner, &it, &dt, "E1201", hint);
                            (dt, true)
                        }
                        None => (self.infer(inner), false),
                    }
                } else if let Some(b) = bind {
                    // `stick x: T = e`, its check a statement after it.
                    self.guarded = Some((value.span.start, value.span.end));
                    let it = self.infer(value);
                    self.guarded = None;
                    let hint =
                        format!("expected `{b}` here, because `{name}` is declared as `{b}`");
                    self.expect(value, &it, &b, "E1201", hint);
                    (b, true)
                } else if name.starts_with("__cg_v") {
                    // `snuff e` in a function with a result type: checked here,
                    // where the value is.
                    let t = self.infer(value);
                    let ret = self
                        .ctx
                        .last()
                        .and_then(|c| c.ret.clone().map(|r| (r, c.name.clone())));
                    if let Some((rt, f)) = ret {
                        let hint =
                            format!("expected `{rt}` here, because of the result type of `{f}`");
                        self.expect(value, &t, &rt, "E1217", hint);
                    }
                    (t, false)
                } else {
                    self.guarded = Some((value.span.start, value.span.end));
                    let t = self.infer(value);
                    self.guarded = None;
                    if let Ty::Sized(_) = &t {
                        // Sized arithmetic, range-checked where it is bound.
                        let text = format!("; {}", crate::lower::guard(name, &t, name));
                        self.resolutions.push(Resolution::Insert {
                            at: value.span.end,
                            text,
                        });
                    }
                    (
                        if *mutable && matches!(t, Ty::Null) {
                            Ty::Any
                        } else {
                            t
                        },
                        false,
                    )
                };
                self.define(name, ty, *mutable, annotated);
            }
            Stmt::Assign {
                target,
                op,
                value,
                span,
            } => self.assign(target, *op, value, *span),
            Stmt::Pull(f) => self.pull(f),
            Stmt::Chain(c) => {
                for e in &c.steps {
                    self.infer(e);
                }
            }
            Stmt::Pack(p) => {
                for e in &p.roots {
                    self.infer(e);
                }
            }
            Stmt::Snuff { value, .. } => {
                let ret = self
                    .ctx
                    .last()
                    .and_then(|c| c.ret.clone().map(|r| (r, c.name.clone())));
                if let Some(v) = value {
                    if matches!(&v.kind, ExprKind::Ident(n) if n.starts_with("__cg_v")) {
                        // checked where the value was bound
                        self.infer(v);
                        return;
                    }
                    let target = self.unwrap_check(v).map(|(i, _)| i).unwrap_or(v);
                    let t = self.infer(target);
                    if let Some((rt, f)) = ret {
                        let hint =
                            format!("expected `{rt}` here, because of the result type of `{f}`");
                        self.expect(target, &t, &rt, "E1217", hint);
                    }
                }
            }
            Stmt::Exhale { values, .. } => {
                for v in values {
                    self.infer(v);
                }
            }
            Stmt::Cough { value, .. } => {
                let t = self.infer(value);
                if let (Ty::Record(n, _) | Ty::Enum(n, _), Some(c)) = (&t, self.ctx.last()) {
                    if c.try_depth == 0
                        && !c.coughs.is_empty()
                        && !c
                            .coughs
                            .iter()
                            .any(|x| matches!(x, Ty::Record(m, _) | Ty::Enum(m, _) if m == n))
                    {
                        let list: Vec<String> = c.coughs.iter().map(|x| x.to_string()).collect();
                        let (f, list) = (c.name.clone(), list.join(", "));
                        self.err(
                            "E1218",
                            format!("`{f}` coughs `{n}`, which is not in its `coughs` list"),
                            value.span,
                            Some(format!("the list is `coughs {list}`; add `{n}` to it or handle it with ashtray")),
                        );
                    }
                }
            }
            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                let mut neg: Vec<(String, Ty)> = vec![];
                for (cond, blk) in branches {
                    self.scopes.push(HashMap::new());
                    self.apply_facts(neg.clone());
                    self.infer(cond);
                    let pos = self.facts(cond, true);
                    self.apply_facts(pos);
                    self.block(blk);
                    let nf = self.facts(cond, false);
                    self.scopes.pop();
                    neg.extend(nf);
                }
                if let Some(b) = otherwise {
                    self.scopes.push(HashMap::new());
                    self.apply_facts(neg.clone());
                    self.block(b);
                    self.scopes.pop();
                } else if branches.len() == 1 && Self::diverges(&branches[0].1) {
                    // `if x == null { snuff ... }`: x is not null from here on.
                    self.apply_facts(neg);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.reset_narrowing(&assigned(&body.stmts));
                self.infer(cond);
                let pos = self.facts(cond, true);
                self.scopes.push(HashMap::new());
                self.apply_facts(pos);
                self.block(body);
                self.scopes.pop();
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                self.reset_narrowing(&assigned(&body.stmts));
                let it = self.infer(iter);
                let el = match strip_opt(&it) {
                    Ty::List(t) => *t,
                    Ty::Map(_) => Ty::Text,
                    Ty::Text | Ty::Path => Ty::Text,
                    _ => Ty::Any,
                };
                self.scopes.push(HashMap::new());
                self.define(var, el, true, false);
                self.block(body);
                self.scopes.pop();
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Try {
                body,
                catch_var,
                handler,
                finally,
                ..
            } => {
                if let Some(c) = self.ctx.last_mut() {
                    c.try_depth += 1;
                }
                self.block(body);
                if let Some(c) = self.ctx.last_mut() {
                    c.try_depth -= 1;
                }
                if let Some(h) = handler {
                    self.scopes.push(HashMap::new());
                    if let Some(v) = catch_var {
                        self.define(v, Ty::Any, true, false);
                    }
                    self.block(h);
                    self.scopes.pop();
                }
                if let Some(f) = finally {
                    self.block(f);
                }
            }
            Stmt::Burn {
                state,
                label,
                body,
                unburn,
                ..
            } => {
                if let Some(l) = label {
                    self.infer(l);
                }
                self.scopes.push(HashMap::new());
                if let Some(s) = state {
                    self.define(s, Ty::Any, true, false);
                }
                self.burn_depth += 1;
                self.block(body);
                if let Some(u) = unburn {
                    self.block(u);
                }
                self.burn_depth -= 1;
                self.scopes.pop();
            }
            Stmt::Expr(e) => {
                self.infer(e);
            }
        }
    }

    fn pull(&mut self, f: &FnDecl) {
        if f.name.starts_with("_cg_") && !f.name.starts_with("_cg_impl_") {
            return; // the runtime prelude and lowered constructors
        }
        let sig = self
            .prog
            .fns
            .get(&f.span.start)
            .cloned()
            .or_else(|| self.fns.get(&f.name).cloned());
        self.scopes.push(HashMap::new());
        for (i, pn) in f.params.iter().enumerate() {
            let declared = sig
                .as_ref()
                .and_then(|s| s.params.get(i))
                .and_then(|p| p.ty.clone());
            let self_ty = if pn == "self" {
                self.impl_self.get(&f.name).cloned()
            } else {
                None
            };
            let annotated = declared.is_some();
            self.define(pn, declared.or(self_ty).unwrap_or(Ty::Any), true, annotated);
        }
        let shown = sig
            .as_ref()
            .map(|s| shown_name(&s.name))
            .unwrap_or_else(|| f.name.clone());
        self.ctx.push(FnCtx {
            name: shown,
            ret: sig.as_ref().and_then(|s| s.ret.clone()),
            coughs: sig.as_ref().map(|s| s.coughs.clone()).unwrap_or_default(),
            try_depth: 0,
        });
        // A function is not inside the burn it is written in: it runs where
        // it is called.
        let burns = std::mem::take(&mut self.burn_depth);
        self.stmts(&f.body.stmts);
        self.burn_depth = burns;
        self.ctx.pop();
        self.scopes.pop();
    }

    fn assign(&mut self, target: &AssignTarget, op: AssignOp, value: &Expr, span: Span) {
        let sized_target = match target {
            AssignTarget::Var(n) => self
                .find_var(n)
                .is_some_and(|v| matches!(v.declared, Ty::Sized(_))),
            _ => false,
        };
        if sized_target {
            self.guarded = Some((value.span.start, value.span.end));
        }
        let vt = self.infer(value);
        self.guarded = None;
        match target {
            AssignTarget::Var(n) => {
                let Some(var) = self.find_var(n).cloned() else {
                    return;
                };
                if !var.mutable {
                    self.err(
                        "E1215",
                        format!("`{n}` is a `stick` and cannot be assigned to"),
                        span,
                        Some(format!(
                            "declare it with `roll {n} = ...` if it has to change"
                        )),
                    );
                    return;
                }
                let new = if op == AssignOp::Set {
                    vt.clone()
                } else {
                    arith(
                        match op {
                            AssignOp::Add => BinOp::Add,
                            AssignOp::Sub => BinOp::Sub,
                            AssignOp::Mul => BinOp::Mul,
                            _ => BinOp::Div,
                        },
                        &var.declared,
                        &vt,
                    )
                };
                if var.annotated {
                    let d = var.declared.clone();
                    if op == AssignOp::Set {
                        let hint =
                            format!("expected `{d}` here, because `{n}` is declared as `{d}`");
                        self.expect(value, &vt, &d, "E1201", hint);
                    }
                    // A typed `roll` is a boundary each time it is set: a value
                    // the checker cannot see, or a sized number's arithmetic,
                    // is checked as the statement ends.
                    let unseen = op == AssignOp::Set && matches!(vt, Ty::Any);
                    if matches!(d, Ty::Sized(_)) || (unseen && crate::lower::ty::checks(&d)) {
                        let text = format!("; {}", crate::lower::guard(n, &d, n));
                        self.resolutions.push(Resolution::Insert {
                            at: value.span.end,
                            text,
                        });
                    }
                    self.set_all(n, |v| v.ty = v.declared.clone());
                } else {
                    let w = join(&var.declared, &new);
                    self.set_all(n, move |v| {
                        v.ty = w.clone();
                        v.declared = w.clone();
                    });
                }
            }
            AssignTarget::Index { object, index } => {
                self.infer(object);
                self.infer(index);
            }
            AssignTarget::Member { object, name } => {
                let ot = self.infer(object);
                if let Ty::Record(..) = strip_opt(&ot) {
                    if let Some(ft) =
                        self.field_ty(&strip_opt(&ot), name, object, value.span, false)
                    {
                        let hint = format!(
                            "expected `{ft}` here, because of the declared type of field `{name}`"
                        );
                        self.expect(value, &vt, &ft, "E1201", hint);
                    }
                }
            }
        }
    }

    // ------------------------------------------------------ expressions

    fn field_ty(
        &mut self,
        rec: &Ty,
        name: &str,
        _obj: &Expr,
        span: Span,
        report: bool,
    ) -> Option<Ty> {
        let Ty::Record(rn, args) = rec else {
            return None;
        };
        let r = self.prog.records.get(rn)?;
        match r.fields.iter().find(|(f, _)| f == name) {
            Some((_, t)) => {
                let b: HashMap<String, Ty> = r
                    .generics
                    .iter()
                    .cloned()
                    .zip(args.iter().cloned())
                    .collect();
                Some(subst(t, &r.generics, &b))
            }
            None => {
                let have: Vec<&str> = r.fields.iter().map(|(f, _)| f.as_str()).collect();
                let _ = report;
                self.err(
                    "E1204",
                    format!("`{rn}` has no field `{name}`"),
                    span,
                    Some(format!("its fields are: {}", have.join(", "))),
                );
                None
            }
        }
    }

    fn lookup(&self, n: &str) -> Ty {
        if let Some(v) = self.find_var(n) {
            return v.ty.clone();
        }
        match self.fns.get(n) {
            Some(s) if s.generics.is_empty() => Ty::Fn(
                s.params
                    .iter()
                    .map(|p| p.ty.clone().unwrap_or(Ty::Any))
                    .collect(),
                Box::new(s.ret.clone().unwrap_or(Ty::Any)),
            ),
            _ => Ty::Any,
        }
    }

    fn infer(&mut self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Null => Ty::Null,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Int(_) => Ty::Int,
            ExprKind::Float(_) => Ty::Float,
            ExprKind::Str(parts) => {
                for p in parts {
                    if let StrPiece::Expr(x) = p {
                        self.infer(x);
                    }
                }
                Ty::Text
            }
            ExprKind::Ident(n) => self.lookup(n),
            ExprKind::List(items) => {
                let mut el: Option<Ty> = None;
                for i in items {
                    let t = self.infer(i);
                    el = Some(match el {
                        Some(o) => join(&o, &t),
                        None => t,
                    });
                }
                Ty::List(Box::new(
                    el.filter(|t| !matches!(t, Ty::Null)).unwrap_or(Ty::Any),
                ))
            }
            ExprKind::Map(entries) => {
                let mut el: Option<Ty> = None;
                for (k, v) in entries {
                    if let MapKey::Dynamic(x) = k {
                        self.infer(x);
                    }
                    let t = self.infer(v);
                    el = Some(match el {
                        Some(o) => join(&o, &t),
                        None => t,
                    });
                }
                Ty::Map(Box::new(el.unwrap_or(Ty::Any)))
            }
            ExprKind::Unary { op, expr } => {
                let t = self.infer(expr);
                match op {
                    UnOp::Not => Ty::Bool,
                    UnOp::Neg => match t {
                        Ty::Int | Ty::Float | Ty::Sized(_) => t,
                        _ => Ty::Any,
                    },
                }
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let t = self.binary(*op, lhs, rhs);
                if let Ty::Sized(sz) = &t {
                    // A sized number's arithmetic is range-checked, unless
                    // the statement that binds it checks it already.
                    if self.guarded != Some((e.span.start, e.span.end)) {
                        let (lo, hi) = sz.range();
                        match self.stmt_end {
                            // A check declared at the end of the statement,
                            // so an overflow coughs on the statement's line.
                            Some(end) => {
                                self.locals += 1;
                                let f = format!("__cg_sz{}", self.locals);
                                let d = crate::lower::ty::descriptor(&t);
                                self.resolutions.push(Resolution::Wrap {
                                    start: e.span.start,
                                    end: e.span.end,
                                    pre: format!("{f}("),
                                    post: ")".into(),
                                });
                                self.resolutions.push(Resolution::Insert {
                                    at: end,
                                    text: format!(
                                        "; pull {f}(v) {{ if type_of(v) == \"int\" and v >= {lo} and v <= {hi} {{ snuff v }}; cough _cg_why(v, {d}, \"the result\") }}"
                                    ),
                                });
                            }
                            None => self.resolutions.push(Resolution::Wrap {
                                start: e.span.start,
                                end: e.span.end,
                                pre: "_cg_sized(".into(),
                                post: format!(", {lo}, {hi}, \"{}\")", sz.name()),
                            }),
                        }
                    }
                }
                t
            }
            ExprKind::Call { callee, args } => self.call(callee, args, e.span),
            ExprKind::Method {
                receiver,
                name,
                args,
                safe,
            } => self.method(receiver, name, args, *safe, e.span),
            ExprKind::Member { object, name, safe } => {
                let ot = self.infer(object);
                let inner = strip_opt(&ot);
                let t = match &inner {
                    Ty::Record(..) => self
                        .field_ty(&inner, name, object, e.span, true)
                        .unwrap_or(Ty::Any),
                    // `m.k` on a missing key is an error, not null (`m?.k` is null).
                    Ty::Map(v) => (**v).clone(),
                    _ => Ty::Any,
                };
                if *safe {
                    opt(t)
                } else {
                    t
                }
            }
            ExprKind::Index { object, index } => {
                let ot = self.infer(object);
                self.infer(index);
                match strip_opt(&ot) {
                    Ty::List(t) | Ty::Map(t) => *t,
                    Ty::Text | Ty::Path => Ty::Text,
                    _ => Ty::Any,
                }
            }
            ExprKind::Lambda { params, body } => {
                let declared = self.prog.lambdas.get(&e.span.start);
                self.scopes.push(HashMap::new());
                let mut ptys = vec![];
                for (i, p) in params.iter().enumerate() {
                    let t = declared.and_then(|d| d.get(i).cloned().flatten());
                    let annotated = t.is_some();
                    let t = t.unwrap_or(Ty::Any);
                    ptys.push(t.clone());
                    self.define(p, t, true, annotated);
                }
                let ret = match body {
                    LambdaBody::Expr(x) => self.infer(x),
                    LambdaBody::Block(b) => {
                        self.ctx.push(FnCtx {
                            name: "pack".into(),
                            ret: None,
                            coughs: vec![],
                            try_depth: 1,
                        });
                        self.stmts(&b.stmts);
                        self.ctx.pop();
                        Ty::Any
                    }
                };
                self.scopes.pop();
                Ty::Fn(ptys, Box::new(ret))
            }
            ExprKind::Pipe { lhs, rhs } => {
                self.infer(lhs);
                // `a >> f(b)` calls `f(a, b)`: arity is not checked here.
                match &rhs.kind {
                    ExprKind::Call { args, .. } | ExprKind::Method { args, .. } => {
                        for a in args {
                            self.infer(a);
                        }
                    }
                    _ => {
                        self.infer(rhs);
                    }
                }
                Ty::Any
            }
        }
    }

    fn binary(&mut self, op: BinOp, lhs: &Expr, rhs: &Expr) -> Ty {
        let l = self.infer(lhs);
        if op == BinOp::And || op == BinOp::Or {
            // the right side of `x != null and ...` sees x narrowed
            let f = if op == BinOp::And {
                self.facts(lhs, true)
            } else {
                self.facts(lhs, false)
            };
            self.scopes.push(HashMap::new());
            self.apply_facts(f);
            let r = self.infer(rhs);
            self.scopes.pop();
            return if l == Ty::Bool && r == Ty::Bool {
                Ty::Bool
            } else {
                Ty::Any
            };
        }
        let r = self.infer(rhs);
        match op {
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => Ty::Bool,
            BinOp::Range => Ty::List(Box::new(Ty::Int)),
            BinOp::Coalesce => match l {
                Ty::Opt(inner) => join(&inner, &r),
                Ty::Null => r,
                Ty::Any => Ty::Any,
                other => other,
            },
            _ => arith(op, &l, &r),
        }
    }

    // ------------------------------------------------------ calls

    fn sig_params(s: &types::FnSig) -> Vec<Ty> {
        s.params
            .iter()
            .map(|p| p.ty.clone().unwrap_or(Ty::Any))
            .collect()
    }

    #[allow(clippy::too_many_arguments)]
    fn apply(
        &mut self,
        what: &str,
        gens: &[String],
        params: &[Ty],
        optional: usize,
        rest: Option<&Ty>,
        ret: &Ty,
        args: &[Expr],
        span: Span,
        mut binds: HashMap<String, Ty>,
    ) -> Ty {
        let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
        self.last_args = arg_tys.clone();
        let min = params.len().saturating_sub(optional);
        let bad = args.len() < min || (rest.is_none() && args.len() > params.len());
        if bad {
            let want = if rest.is_some() {
                format!("at least {min}")
            } else if optional > 0 {
                format!("{min} to {}", params.len())
            } else {
                params.len().to_string()
            };
            self.err(
                "E1216",
                format!("`{what}` takes {want} argument(s), given {}", args.len()),
                span,
                Some(format!(
                    "expected {want} here, because of the signature of `{what}`"
                )),
            );
            return subst(ret, gens, &binds);
        }
        for (i, t) in arg_tys.iter().enumerate() {
            if let Some(p) = params.get(i).or(rest) {
                unify(p, t, gens, &mut binds);
            }
        }
        for (i, t) in arg_tys.iter().enumerate() {
            if let Some(p) = params.get(i).or(rest) {
                let p = subst(p, gens, &binds);
                let hint = format!("expected `{p}` here, because of this signature of `{what}`");
                self.expect(&args[i], t, &p, "E1201", hint);
            }
        }
        subst(ret, gens, &binds)
    }

    fn apply_sig(
        &mut self,
        what: &str,
        s: &stdlib::Sig,
        args: &[Expr],
        span: Span,
        binds: HashMap<String, Ty>,
    ) -> Ty {
        let gens: Vec<String> = s.generics.iter().map(|g| g.to_string()).collect();
        self.apply(
            what,
            &gens,
            &s.params,
            s.optional,
            s.rest.as_ref(),
            &s.ret,
            args,
            span,
            binds,
        )
    }

    fn call(&mut self, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        let prog = self.prog;
        if let ExprKind::Ident(name) = &callee.kind {
            if self.find_var(name).is_none() {
                if name == "_cg_check" && args.len() == 3 {
                    self.infer(&args[0]);
                    return decode(&args[1]).unwrap_or(Ty::Any);
                }
                if name == "_cg_variant" && args.len() == 3 {
                    for a in args {
                        self.infer(a);
                    }
                    if let (Some(en), Some(v)) = (lit_str(&args[1]), lit_str(&args[2])) {
                        if let Some(ed) = prog.enums.get(&en) {
                            if !ed.variants.iter().any(|x| x.name == v) {
                                let have: Vec<&str> =
                                    ed.variants.iter().map(|x| x.name.as_str()).collect();
                                self.err(
                                    "E1207",
                                    format!("enum `{en}` has no variant `{v}`"),
                                    args[2].span,
                                    Some(format!("its variants are: {}", have.join(", "))),
                                );
                            }
                        }
                    }
                    return Ty::Bool;
                }
                if name == "_cg_coughed" && args.len() == 1 {
                    // `cough R { ... }`, lowered: still an `R` to `coughs`.
                    return self.infer(&args[0]);
                }
                if let Some(layer) = name.strip_prefix("_cg_layer_") {
                    return self.layer_call(layer, callee, args, span);
                }
                if let Some(rest) = name.strip_prefix("_cg_named_") {
                    if let Some(t) = self.named_ctor(rest, args) {
                        return t;
                    }
                }
                if let Some(rn) = name.strip_prefix("_cg_new_") {
                    if let Some(rd) = prog.records.get(rn) {
                        return self.record_ctor(rd, args, span);
                    }
                }
                if let Some(t) = self.enum_ctor(name, args, span) {
                    return t;
                }
                if let Some(s) = self.fns.get(name).cloned() {
                    let gens: Vec<String> = s.generics.iter().map(|g| g.0.clone()).collect();
                    let params = Self::sig_params(&s);
                    let ret = s.ret.clone().unwrap_or(Ty::Any);
                    let t = self.apply(
                        name,
                        &gens,
                        &params,
                        0,
                        None,
                        &ret,
                        args,
                        span,
                        HashMap::new(),
                    );
                    // A value the checker cannot see is checked where it is
                    // passed, so a bad one coughs on the caller's line.
                    if gens.is_empty() && args.len() == params.len() {
                        let tys = std::mem::take(&mut self.last_args);
                        for (i, (a, at)) in args.iter().zip(&tys).enumerate() {
                            if needs_check(at, &params[i]) {
                                let what = format!("{}: {}", shown_name(name), s.params[i].name);
                                self.check_here(a, &params[i], &what);
                            }
                        }
                    }
                    return t;
                }
                if let Some(s) = stdlib::function(None, name) {
                    return self.apply_sig(name, &s, args, span, HashMap::new());
                }
                for a in args {
                    self.infer(a);
                }
                return Ty::Any;
            }
        }
        let ct = self.infer(callee);
        if let Ty::Fn(ps, r) = ct {
            let what = if let ExprKind::Ident(n) = &callee.kind {
                n.clone()
            } else {
                "this function".to_string()
            };
            return self.apply(&what, &[], &ps, 0, None, &r, args, span, HashMap::new());
        }
        for a in args {
            self.infer(a);
        }
        Ty::Any
    }

    /// A channel's `send`, `receive`, `close` and `closed`. A send is made
    /// through a helper on this line, so a send on a closed channel coughs
    /// here.
    fn channel_method(
        &mut self,
        receiver: &Expr,
        name: &str,
        args: &[Expr],
        el: Ty,
        safe: bool,
        span: Span,
    ) -> Ty {
        let tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
        let arity = |n: usize| if n == 1 { "argument" } else { "arguments" };
        let want = match name {
            "send" => 1,
            "receive" | "close" | "closed" => 0,
            _ => return Ty::Any,
        };
        if args.len() != want {
            self.err(
                "E1216",
                format!(
                    "`.{name}` takes {want} {}, given {}",
                    arity(want),
                    args.len()
                ),
                span,
                None,
            );
            return Ty::Any;
        }
        match name {
            "send" => {
                let hint = format!("expected `{el}` here, because the channel carries `{el}`");
                self.expect(&args[0], &tys[0], &el, "E1201", hint);
                if let Some(f) = self.local(|n| {
                    format!("pull {n}(c, v) {{ if c.closed() {{ cough {{code: \"E1520\", message: \"E1520 send on a closed channel\"}} }}; snuff c.send(v) }}")
                }) {
                    self.resolutions.push(Resolution::Method {
                        recv_start: receiver.span.start,
                        recv_end: receiver.span.end,
                        call_end: span.end,
                        func: f,
                        has_args: true,
                        safe,
                    });
                }
                Ty::Null
            }
            "receive" => opt(el),
            "closed" => Ty::Bool,
            _ => Ty::Null,
        }
    }

    /// `measure x` (`_cg_layer_measure(x)`) and `a into b`
    /// (`_cg_layer_into(a, b)`): the layer is picked by the type of `x` or
    /// `b`, here, and the call renamed to it. A type the checker cannot see
    /// is left to the run-time choice.
    fn layer_call(&mut self, layer: &str, callee: &Expr, args: &[Expr], span: Span) -> Ty {
        let tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
        let at = if layer == "into" { 1 } else { 0 };
        let Some(t) = tys.get(at).map(strip_opt) else {
            return Ty::Any;
        };
        if matches!(t, Ty::Any | Ty::Param(_) | Ty::Never) {
            self.untyped_layer(layer, callee, span);
            return Ty::Any;
        }
        let prog = self.prog;
        let user: Vec<&types::LayerDecl> = prog
            .layers
            .iter()
            .filter(|l| l.layer == layer && layer_fits(&l.for_ty, &t))
            .collect();
        let built_in = builtin_layer(layer, &t);
        let shown = if layer == "into" {
            "the target"
        } else {
            "the value"
        };
        match (user.as_slice(), built_in) {
            ([], None) => {
                self.err(
                    "E1402",
                    format!("nothing gives `{layer}` a meaning for `{t}`"),
                    span,
                    Some(format!(
                        "declare one: `{layer} for {t} = function` ({shown} is `{t}`)"
                    )),
                );
                Ty::Any
            }
            ([one], None) => {
                let f = types::impl_fn_name(layer, &one.for_ty, layer);
                self.resolutions.push(Resolution::Rename {
                    start: callee.span.start,
                    end: callee.span.end,
                    text: f,
                });
                let ret = self
                    .fns
                    .get(one.function.trim())
                    .and_then(|s| s.ret.clone());
                ret.unwrap_or(Ty::Any)
            }
            ([], Some((f, ret, effect))) => {
                if layer == "into" {
                    if let (Ty::List(el), Some(v)) = (&t, tys.first()) {
                        let hint = format!("expected `{el}` here, because the target is `{t}`");
                        self.expect(&args[0], v, el, "E1201", hint);
                    }
                }
                if effect && self.burn_depth == 0 {
                    let msg = format!(
                        "`{layer}` a path writes the file, so it must be inside a burn block"
                    );
                    let hint =
                        "wrap it: burn { ... }, or call this function from inside one".to_string();
                    if self.ctx.is_empty() {
                        self.err("E303", msg, span, Some(hint));
                    } else {
                        self.warn("E303", msg, span, Some(hint));
                    }
                }
                self.resolutions.push(Resolution::Rename {
                    start: callee.span.start,
                    end: callee.span.end,
                    text: f.to_string(),
                });
                ret
            }
            (users, b) => {
                let mut names: Vec<String> = users
                    .iter()
                    .map(|l| format!("`{layer} for {} = {}`", l.for_ty, l.function))
                    .collect();
                if b.is_some() {
                    names.insert(0, format!("the built-in `{layer}` for `{t}`"));
                }
                self.err(
                    "E1401",
                    format!(
                        "`{layer}` has {} meanings for `{t}`: {}",
                        names.len(),
                        names.join(" and ")
                    ),
                    span,
                    Some("keep one: a layer means one thing for a type".to_string()),
                );
                Ty::Any
            }
        }
    }

    /// A layer on a value the checker cannot see: chosen when the program
    /// runs, by a helper declared on this line (so what fails, fails here,
    /// and a write is in the text cig reads).
    fn untyped_layer(&mut self, layer: &str, callee: &Expr, span: Span) {
        let prog = self.prog;
        let into = layer == "into";
        let subject = if into { "t" } else { "v" };
        let mut lines = String::new();
        for l in prog.layers.iter().filter(|l| l.layer == layer) {
            let f = types::impl_fn_name(layer, &l.for_ty, layer);
            let test = crate::lower::type_test(&l.for_ty, subject);
            let call = if into {
                format!("{f}(v, t)")
            } else {
                format!("{f}(v)")
            };
            lines.push_str(&format!("if {test} {{ snuff {call} }}; "));
        }
        match layer {
            "measure" => lines.push_str(
                "stick k = type_of(v); if k == \"list\" or k == \"map\" or k == \"string\" { snuff len(v) }; ",
            ),
            "into" => lines.push_str(
                "if type_of(t) == \"list\" { t.push(v); snuff null }; if type_of(t) == \"string\" { if type_of(v) == \"string\" { fs.write_text(t, v) } else { fs.write_text(t, json.stringify(_cg_plain(v))) }; snuff null }; ",
            ),
            _ => {}
        }
        let params = if into { "v, t" } else { "v" };
        let f = self.local(|n| {
            format!(
                "pull {n}({params}) {{ {lines}cough {{code: \"E1402\", message: \"E1402 nothing gives `{layer}` a meaning for \" + _cg_kind({subject})}} }}"
            )
        });
        if let Some(f) = f {
            self.resolutions.push(Resolution::Rename {
                start: callee.span.start,
                end: callee.span.end,
                text: f,
            });
        }
        if into && self.burn_depth == 0 {
            self.warn(
                "E303",
                "`into` writes a file when its target is a path, and a write must be inside a burn block".into(),
                span,
                Some("wrap it: burn { ... }; a target that is a list needs no burn".into()),
            );
        }
    }

    /// `_cg_named_E_v({field: value, ...})`: an enum variant given by name.
    fn named_ctor(&mut self, rest: &str, args: &[Expr]) -> Option<Ty> {
        let prog = self.prog;
        for (en, ed) in &prog.enums {
            let Some(vn) = rest.strip_prefix(&format!("{en}_")) else {
                continue;
            };
            let Some(v) = ed.variants.iter().find(|v| v.name == vn) else {
                continue;
            };
            let gens = ed.generics.clone();
            let mut binds = HashMap::new();
            if let (1, Some(ExprKind::Map(entries))) = (args.len(), args.first().map(|a| &a.kind)) {
                let mut vals: Vec<(String, Ty, &Expr)> = vec![];
                for (k, val) in entries {
                    let t = self.infer(val);
                    if let MapKey::Static(n) = k {
                        vals.push((n.clone(), t, val));
                    }
                }
                for (fname, fty) in &v.fields {
                    if let Some((_, t, _)) = vals.iter().find(|(n, _, _)| n == fname) {
                        unify(fty, t, &gens, &mut binds);
                    }
                }
                for (n, t, val) in &vals {
                    if let Some((_, fty)) = v.fields.iter().find(|(f, _)| f == n) {
                        let want = subst(fty, &gens, &binds);
                        let hint = format!(
                            "expected `{want}` here, because of field `{n}` of `{en}.{vn}`"
                        );
                        if self.expect(val, t, &want, "E1201", hint) && needs_check(t, &want) {
                            self.check_here(val, &want, &format!("{en}.{vn} {n}"));
                        }
                    }
                }
            } else {
                for a in args {
                    self.infer(a);
                }
            }
            return Some(Ty::Enum(
                en.clone(),
                gens.iter()
                    .map(|g| binds.get(g).cloned().unwrap_or(Ty::Any))
                    .collect(),
            ));
        }
        None
    }

    fn record_ctor(&mut self, rd: &types::RecordDecl, args: &[Expr], span: Span) -> Ty {
        let gens = rd.generics.clone();
        let mut binds = HashMap::new();
        if let (1, Some(ExprKind::Map(entries))) = (args.len(), args.first().map(|a| &a.kind)) {
            let mut dynamic = false;
            let mut given: Vec<&str> = vec![];
            let mut vals: Vec<(&str, Ty, &Expr)> = vec![];
            for (k, v) in entries {
                let t = self.infer(v);
                match k {
                    MapKey::Static(s) => {
                        given.push(s);
                        vals.push((s, t, v));
                    }
                    MapKey::Dynamic(x) => {
                        self.infer(x);
                        dynamic = true;
                    }
                }
            }
            let names: Vec<&str> = rd.fields.iter().map(|(f, _)| f.as_str()).collect();
            for (fname, fty) in &rd.fields {
                if let Some((_, t, _)) = vals.iter().find(|(n, _, _)| n == fname) {
                    unify(fty, t, &gens, &mut binds);
                } else if !dynamic {
                    self.err(
                        "E1205",
                        format!("`{}` is missing the field `{fname}`", rd.name),
                        span,
                        Some(format!("`{}` declares: {}", rd.name, names.join(", "))),
                    );
                }
            }
            for (n, t, v) in &vals {
                match rd.fields.iter().find(|(f, _)| f == n) {
                    None => self.err(
                        "E1206",
                        format!("`{}` has no field `{n}`", rd.name),
                        v.span,
                        Some(format!("`{}` declares: {}", rd.name, names.join(", "))),
                    ),
                    Some((_, fty)) => {
                        let want = subst(fty, &gens, &binds);
                        let hint = format!(
                            "expected `{want}` here, because of the type of field `{n}` in `{}`",
                            rd.name
                        );
                        if self.expect(v, t, &want, "E1201", hint) && needs_check(t, &want) {
                            self.check_here(v, &want, &format!("{}.{n}", rd.name));
                        }
                    }
                }
            }
            let _ = given;
        } else {
            for a in args {
                self.infer(a);
            }
        }
        Ty::Record(
            rd.name.clone(),
            gens.iter()
                .map(|g| binds.get(g).cloned().unwrap_or(Ty::Any))
                .collect(),
        )
    }

    fn enum_ctor(&mut self, name: &str, args: &[Expr], span: Span) -> Option<Ty> {
        let prog = self.prog;
        for (en, ed) in &prog.enums {
            let Some(vn) = name.strip_prefix(&format!("_cg_{en}_")) else {
                continue;
            };
            let Some(v) = ed.variants.iter().find(|v| v.name == vn) else {
                for a in args {
                    self.infer(a);
                }
                let have: Vec<&str> = ed.variants.iter().map(|x| x.name.as_str()).collect();
                self.err(
                    "E1207",
                    format!("enum `{en}` has no variant `{vn}`"),
                    span,
                    Some(format!("its variants are: {}", have.join(", "))),
                );
                return Some(Ty::Enum(en.clone(), vec![]));
            };
            let gens = ed.generics.clone();
            let params: Vec<Ty> = v.fields.iter().map(|(_, t)| t.clone()).collect();
            let mut binds = HashMap::new();
            let arg_tys: Vec<Ty> = args.iter().map(|a| self.infer(a)).collect();
            if arg_tys.len() != params.len() {
                self.err(
                    "E1208",
                    format!(
                        "`{en}.{vn}` takes {} field(s), given {}",
                        params.len(),
                        arg_tys.len()
                    ),
                    span,
                    Some(format!(
                        "expected {} here, because of the declaration of `{vn}`",
                        params.len()
                    )),
                );
            } else {
                for (p, t) in params.iter().zip(&arg_tys) {
                    unify(p, t, &gens, &mut binds);
                }
                for (i, (p, t)) in params.iter().zip(&arg_tys).enumerate() {
                    let want = subst(p, &gens, &binds);
                    let hint = format!(
                        "expected `{want}` here, because of field `{}` of `{en}.{vn}`",
                        v.fields[i].0
                    );
                    if self.expect(&args[i], t, &want, "E1201", hint) && needs_check(t, &want) {
                        let what = format!("{en}.{vn} {}", v.fields[i].0);
                        self.check_here(&args[i], &want, &what);
                    }
                }
            }
            return Some(Ty::Enum(
                en.clone(),
                gens.iter()
                    .map(|g| binds.get(g).cloned().unwrap_or(Ty::Any))
                    .collect(),
            ));
        }
        None
    }

    fn method(&mut self, receiver: &Expr, name: &str, args: &[Expr], safe: bool, span: Span) -> Ty {
        let prog = self.prog;
        // `fs.size(p)`: a stdlib module call
        if let ExprKind::Ident(m) = &receiver.kind {
            if self.find_var(m).is_none() && stdlib::is_module(m) {
                let what = format!("{m}.{name}");
                return match stdlib::function(Some(m), name) {
                    Some(s) => self.apply_sig(&what, &s, args, span, HashMap::new()),
                    None => {
                        for a in args {
                            self.infer(a);
                        }
                        Ty::Any
                    }
                };
            }
        }
        // `plan.f(x)` for `use tools.plan`: the module's signature.
        if let ExprKind::Ident(l) = &receiver.kind {
            if let Some(sig) = prog.module_fns.get(&format!("{l}.{name}")).cloned() {
                self.infer(receiver);
                let gens: Vec<String> = sig.generics.iter().map(|g| g.0.clone()).collect();
                let params = Self::sig_params(&sig);
                let ret = sig.ret.clone().unwrap_or(Ty::Any);
                let what = format!("{l}.{name}");
                let t = self.apply(
                    &what,
                    &gens,
                    &params,
                    0,
                    None,
                    &ret,
                    args,
                    span,
                    HashMap::new(),
                );
                return if safe { opt(t) } else { t };
            }
        }
        let rt = self.infer(receiver);
        let recv = strip_opt(&rt);
        let result = if matches!(recv, Ty::Any | Ty::Param(_) | Ty::Fn(..)) {
            // An interface's method on a value the checker cannot see: picked
            // by the value when it runs, by a dispatcher declared on this
            // line (so a value with no `impl` fails here).
            if !matches!(recv, Ty::Fn(..)) {
                if let Some(i) = prog
                    .interfaces
                    .values()
                    .find(|i| i.methods.iter().any(|m| m.name == name))
                {
                    let params: Vec<String> = (1..=args.len()).map(|k| format!("a{k}")).collect();
                    let list = if params.is_empty() {
                        String::new()
                    } else {
                        format!(", {}", params.join(", "))
                    };
                    let lines = crate::lower::dispatch_lines(prog, &i.name, name, &list);
                    let plain = params.join(", ");
                    let func = self
                        .local(|n| {
                            format!("pull {n}(self{list}) {{ {lines}snuff self.{name}({plain}) }}")
                        })
                        .unwrap_or_else(|| format!("_cg_dispatch_{}_{name}", i.name));
                    self.resolutions.push(Resolution::Method {
                        recv_start: receiver.span.start,
                        recv_end: receiver.span.end,
                        call_end: span.end,
                        func,
                        has_args: !args.is_empty(),
                        safe,
                    });
                }
            }
            for a in args {
                self.infer(a);
            }
            Ty::Any
        } else if let Ty::Channel(el) = &recv {
            self.channel_method(receiver, name, args, (**el).clone(), safe, span)
        } else if let Some(s) = stdlib::method(&recv, name) {
            let mut binds = HashMap::new();
            match &recv {
                Ty::List(t) => {
                    binds.insert("T".to_string(), (**t).clone());
                }
                Ty::Map(t) => {
                    binds.insert("V".to_string(), (**t).clone());
                }
                _ => {}
            }
            self.apply_sig(&format!(".{name}"), &s, args, span, binds)
        } else {
            // an interface method, picked by the receiver's type
            let candidates: Vec<(&types::InterfaceDecl, &types::FnSig)> = prog
                .interfaces
                .values()
                .filter_map(|i| i.methods.iter().find(|m| m.name == name).map(|m| (i, m)))
                .collect();
            if candidates.is_empty() {
                for a in args {
                    self.infer(a);
                }
                Ty::Any
            } else {
                let found = candidates.iter().find_map(|(i, m)| {
                    prog.impls
                        .iter()
                        .find(|im| im.interface == i.name && same_ty(&im.for_ty, &recv))
                        .map(|im| (i, m, im))
                });
                match found {
                    Some((i, m, im)) => {
                        self.resolutions.push(Resolution::Method {
                            recv_start: receiver.span.start,
                            recv_end: receiver.span.end,
                            call_end: span.end,
                            func: types::impl_fn_name(&i.name, &im.for_ty, name),
                            has_args: !args.is_empty(),
                            safe,
                        });
                        let params: Vec<Ty> = m
                            .params
                            .iter()
                            .filter(|p| p.name != "self")
                            .map(|p| p.ty.clone().unwrap_or(Ty::Any))
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
                    None => {
                        for a in args {
                            self.infer(a);
                        }
                        let names: Vec<&str> =
                            candidates.iter().map(|(i, _)| i.name.as_str()).collect();
                        self.err(
                            "E1313",
                            format!("no `impl` gives `{recv}` the method `{name}`"),
                            span,
                            Some(format!(
                                "`{name}` belongs to {}; add `impl {} for {recv} {{ ... }}`",
                                names.join(", "),
                                names[0]
                            )),
                        );
                        Ty::Any
                    }
                }
            }
        };
        if safe {
            opt(result)
        } else {
            result
        }
    }
}

/// Does a value of type `found` need a check when the program runs to be
/// sure it fits `want`?
fn needs_check(found: &Ty, want: &Ty) -> bool {
    crate::lower::ty::checks(want)
        && (matches!(found, Ty::Any) || (matches!(strip_opt(want), Ty::Sized(_)) && found != want))
}

/// A whole number written out: `3`, `-3`.
fn int_literal(e: &Expr) -> Option<i64> {
    match &e.kind {
        ExprKind::Int(v) => Some(*v),
        ExprKind::Unary {
            op: UnOp::Neg,
            expr,
        } => match &expr.kind {
            ExprKind::Int(v) => v.checked_neg(),
            _ => None,
        },
        _ => None,
    }
}

/// Where a statement's own text ends: after its last expression or its
/// closing `}`, before any comment and the line's end (cig's span for a
/// simple statement runs on through both).
fn stmt_tail(s: &Stmt) -> Option<usize> {
    Some(match s {
        Stmt::Declare { value, .. } | Stmt::Assign { value, .. } | Stmt::Cough { value, .. } => {
            value.span.end
        }
        Stmt::Snuff { value, .. } => value.as_ref()?.span.end,
        Stmt::Exhale { values, .. } => values.last()?.span.end,
        Stmt::Expr(e) => e.span.end,
        Stmt::If {
            branches,
            otherwise,
            ..
        } => match otherwise {
            Some(b) => b.span.end,
            None => branches.last()?.1.span.end,
        },
        Stmt::While { body, .. } | Stmt::For { body, .. } => body.span.end,
        Stmt::Try {
            body,
            handler,
            finally,
            ..
        } => {
            finally
                .as_ref()
                .or(handler.as_ref())
                .unwrap_or(body)
                .span
                .end
        }
        Stmt::Burn { body, unburn, .. } => unburn.as_ref().unwrap_or(body).span.end,
        Stmt::Pull(_) | Stmt::Chain(_) | Stmt::Pack(_) | Stmt::Break(_) | Stmt::Continue(_) => {
            return None
        }
    })
}

/// Does a layer declared for `d` apply to a value of type `t`? `Path` and
/// `Text` are told apart here: a layer is what makes them different.
fn layer_fits(d: &Ty, t: &Ty) -> bool {
    match (d, t) {
        (Ty::List(a), Ty::List(_)) | (Ty::Map(a), Ty::Map(_)) => {
            matches!(**a, Ty::Any) || assignable(t, d)
        }
        (Ty::Record(a, _), Ty::Record(b, _)) | (Ty::Enum(a, _), Ty::Enum(b, _)) => a == b,
        (Ty::Text, Ty::Text)
        | (Ty::Path, Ty::Path)
        | (Ty::Int, Ty::Int)
        | (Ty::Float, Ty::Float)
        | (Ty::Bool, Ty::Bool) => true,
        (Ty::Sized(a), Ty::Sized(b)) => a == b,
        _ => false,
    }
}

/// The built-in layers: the function, its result, and whether it is an
/// effect (needs a burn).
fn builtin_layer(layer: &str, t: &Ty) -> Option<(&'static str, Ty, bool)> {
    match (layer, t) {
        ("measure", Ty::List(_) | Ty::Map(_) | Ty::Text) => Some(("len", Ty::Int, false)),
        ("measure", Ty::Path) => Some(("fs.size", Ty::Int, false)),
        ("into", Ty::List(_)) => Some(("_cg_into_list", Ty::Null, false)),
        ("into", Ty::Path | Ty::Text) => Some(("_cg_into_path", Ty::Null, true)),
        _ => None,
    }
}

/// A function's name as written: an `impl` method's lowered name is
/// `_cg_impl_<interface>_<type>_<method>`.
fn shown_name(n: &str) -> String {
    if n.starts_with("_cg_impl_") {
        if let Some(i) = n.rfind('_') {
            return n[i + 1..].to_string();
        }
    }
    n.to_string()
}

/// The type of a literal, worked out without checking anything (so without
/// reporting anything twice).
fn literal_ty(e: &Expr) -> Option<Ty> {
    Some(match &e.kind {
        ExprKind::Null => Ty::Null,
        ExprKind::Bool(_) => Ty::Bool,
        ExprKind::Int(_) => Ty::Int,
        ExprKind::Float(_) => Ty::Float,
        ExprKind::Str(_) => Ty::Text,
        ExprKind::List(items) => {
            let mut el: Option<Ty> = None;
            for i in items {
                let t = literal_ty(i)?;
                el = Some(match el {
                    Some(o) => join(&o, &t),
                    None => t,
                });
            }
            Ty::List(Box::new(el.unwrap_or(Ty::Any)))
        }
        _ => return None,
    })
}

fn strip_opt(t: &Ty) -> Ty {
    match t {
        Ty::Opt(i) => (**i).clone(),
        o => o.clone(),
    }
}

fn opt(t: Ty) -> Ty {
    match t {
        Ty::Any | Ty::Opt(_) => t,
        o => Ty::Opt(Box::new(o)),
    }
}

fn arith(op: BinOp, l: &Ty, r: &Ty) -> Ty {
    use Ty::*;
    match (l, r) {
        // A sized number's arithmetic stays that size (and is range-checked).
        (Sized(a), Sized(b)) if a == b && op != BinOp::Div => Sized(*a),
        (Sized(a), Int) | (Int, Sized(a)) if op != BinOp::Div => Sized(*a),
        (Int | Sized(_), Int | Sized(_)) if op != BinOp::Div => Int,
        (Int | Sized(_), Int | Sized(_)) => Any,
        (Int | Sized(_) | Float, Int | Sized(_) | Float) => Float,
        (Text | Path, Text | Path) if op == BinOp::Add => Text,
        (List(a), List(b)) if op == BinOp::Add => List(Box::new(join(a, b))),
        _ => Any,
    }
}

/// Names assigned anywhere in these statements (to drop narrowing on loop entry).
fn assigned(ss: &[Stmt]) -> HashSet<String> {
    fn walk(ss: &[Stmt], out: &mut HashSet<String>) {
        for s in ss {
            match s {
                Stmt::Assign {
                    target: AssignTarget::Var(n),
                    ..
                } => {
                    out.insert(n.clone());
                }
                Stmt::If {
                    branches,
                    otherwise,
                    ..
                } => {
                    for (_, b) in branches {
                        walk(&b.stmts, out);
                    }
                    if let Some(b) = otherwise {
                        walk(&b.stmts, out);
                    }
                }
                Stmt::While { body, .. } | Stmt::For { body, .. } => walk(&body.stmts, out),
                Stmt::Try {
                    body,
                    handler,
                    finally,
                    ..
                } => {
                    walk(&body.stmts, out);
                    for b in handler.iter().chain(finally.iter()) {
                        walk(&b.stmts, out);
                    }
                }
                Stmt::Burn { body, unburn, .. } => {
                    walk(&body.stmts, out);
                    if let Some(b) = unburn {
                        walk(&b.stmts, out);
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = HashSet::new();
    walk(ss, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FnSig, Param, RecordDecl};

    fn run(src: &str, prog: &types::Program) -> Vec<Diagnostic> {
        let ast = cigscript::syntax::parse(src).expect("test program parses");
        check(&ast, prog, &|o| o)
    }

    fn codes(d: &[Diagnostic]) -> Vec<&'static str> {
        d.iter().map(|d| d.code).collect()
    }

    fn at(src: &str, needle: &str) -> usize {
        src.find(needle)
            .unwrap_or_else(|| panic!("{needle} not in source"))
    }

    fn move_rec() -> RecordDecl {
        RecordDecl {
            name: "Move".into(),
            generics: vec![],
            fields: vec![("src".into(), Ty::Path), ("dst".into(), Ty::Path)],
            public: false,
            at: At::default(),
        }
    }

    fn param(n: &str, t: Option<Ty>) -> Param {
        Param {
            name: n.into(),
            ty: t,
            mode: types::Mode::Shared,
        }
    }

    #[test]
    fn mismatch_on_a_typed_stick() {
        let src = "stick x = _cg_check(\"hi\", {k: \"int\"}, \"x\")\n";
        let mut p = types::Program::default();
        p.bindings.insert(0, Ty::Int);
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1201"]);
        assert!(d[0].message.contains("expected `Int`, found `Text`"));
        assert!(d[0].hint.as_deref().unwrap().contains("declared as `Int`"));
        let sp = d[0].span.unwrap();
        assert_eq!(&src[sp.start..sp.end], "\"hi\"");
    }

    #[test]
    fn null_into_a_non_optional() {
        let src = "stick x = _cg_check(null, {k: \"int\"}, \"x\")\n";
        let mut p = types::Program::default();
        p.bindings.insert(0, Ty::Int);
        assert_eq!(codes(&run(src, &p)), ["E1209"]);
    }

    #[test]
    fn record_literal_missing_and_extra_field() {
        let src = "stick a = _cg_new_Move({src: \"a\"})\nstick b = _cg_new_Move({src: \"a\", dst: \"b\", size: 1})\n";
        let mut p = types::Program::default();
        p.records.insert("Move".into(), move_rec());
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1205", "E1206"]);
        assert!(d[0].message.contains("missing the field `dst`"));
    }

    #[test]
    fn record_field_type_is_checked() {
        let src = "stick a = _cg_new_Move({src: \"a\", dst: 3})\n";
        let mut p = types::Program::default();
        p.records.insert("Move".into(), move_rec());
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1201"]);
        assert!(d[0].hint.as_deref().unwrap().contains("field `dst`"));
    }

    #[test]
    fn unknown_field_read() {
        let src = "stick m = _cg_new_Move({src: \"a\", dst: \"b\"})\nexhale m.src\nexhale m.nope\n";
        let mut p = types::Program::default();
        p.records.insert("Move".into(), move_rec());
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1204"]);
    }

    fn first_prog(src: &str) -> types::Program {
        let mut p = types::Program::default();
        let t = || Ty::Param("T".into());
        p.fns.insert(
            at(src, "pull first"),
            FnSig {
                name: "first".into(),
                generics: vec![("T".into(), vec![])],
                params: vec![param("xs", Some(Ty::List(Box::new(t()))))],
                ret: Some(Ty::Opt(Box::new(t()))),
                ..Default::default()
            },
        );
        p
    }

    #[test]
    fn generic_first_returns_t_or_null() {
        let src = "pull first(xs) {\n  snuff _cg_check(xs[0], {k: \"opt\", of: {k: \"any\"}}, \"result\")\n}\n\
stick ok = _cg_check(first([1, 2]), {k: \"opt\", of: {k: \"int\"}}, \"ok\")\n\
stick bad = _cg_check(first([1, 2]), {k: \"int\"}, \"bad\")\n\
stick worse = _cg_check(first([\"a\"]), {k: \"opt\", of: {k: \"int\"}}, \"worse\")\n";
        let mut p = first_prog(src);
        p.bindings
            .insert(at(src, "stick ok"), Ty::Opt(Box::new(Ty::Int)));
        p.bindings.insert(at(src, "stick bad"), Ty::Int);
        p.bindings
            .insert(at(src, "stick worse"), Ty::Opt(Box::new(Ty::Int)));
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1201", "E1201"], "{d:?}");
        assert!(d[0].message.contains("found `Int?`"));
        assert!(d[0].hint.as_deref().unwrap().contains("!= null"));
        assert!(d[1].message.contains("found `Text?`"));
    }

    #[test]
    fn generic_arity() {
        let src = "pull first(xs) { snuff null }\nstick a = first([1], 2)\n";
        let p = first_prog(src);
        assert_eq!(codes(&run(src, &p)), ["E1216"]);
    }

    #[test]
    fn snuff_of_the_wrong_type() {
        let src = "pull f(a) {\n  snuff _cg_check(\"x\", {k: \"int\"}, \"result\")\n}\n";
        let mut p = types::Program::default();
        p.fns.insert(
            0,
            FnSig {
                name: "f".into(),
                params: vec![param("a", None)],
                ret: Some(Ty::Int),
                ..Default::default()
            },
        );
        let d = run(src, &p);
        assert_eq!(codes(&d), ["E1217"]);
        assert!(d[0].hint.as_deref().unwrap().contains("result type of `f`"));
    }

    #[test]
    fn assignment_to_a_stick() {
        let src = "stick x = 1\nroll y = 1\ny = 2\nx = 2\n";
        let d = run(src, &types::Program::default());
        assert_eq!(codes(&d), ["E1215"]);
        assert_eq!(d[0].line, Some(4));
    }

    #[test]
    fn any_never_errors() {
        let src = "pull g(a) {\n  stick n = a.b + 1\n  stick y = _cg_check(a, {k: \"int\"}, \"y\")\n  stick z = _cg_check(mystery(a, 1), {k: \"text\"}, \"z\")\n  snuff _cg_check(a.c, {k: \"int\"}, \"result\")\n}\n\
roll r = null\nr = 5\nr = \"now text\"\nstick q = _cg_check(r, {k: \"int\"}, \"q\")\n";
        let mut p = types::Program::default();
        p.fns.insert(
            0,
            FnSig {
                name: "g".into(),
                params: vec![param("a", None)],
                ret: Some(Ty::Int),
                ..Default::default()
            },
        );
        p.bindings.insert(at(src, "stick y"), Ty::Int);
        p.bindings.insert(at(src, "stick z"), Ty::Text);
        p.bindings.insert(at(src, "stick q"), Ty::Int);
        assert_eq!(codes(&run(src, &p)), Vec::<&str>::new());
    }

    #[test]
    fn narrowing_by_null_checks() {
        let src = "pull f(o) {\n  stick a = _cg_check(o, {k: \"opt\", of: {k: \"int\"}}, \"a\")\n  if a != null {\n    stick b = _cg_check(a, {k: \"int\"}, \"b\")\n  }\n  if a == null {\n    snuff 0\n  }\n  stick c = _cg_check(a, {k: \"int\"}, \"c\")\n  stick d = _cg_check(a ?? 0, {k: \"int\"}, \"d\")\n}\n\
roll w = _cg_check(null, {k: \"opt\", of: {k: \"int\"}}, \"w\")\nstick e = _cg_check(w, {k: \"int\"}, \"e\")\n";
        let mut p = types::Program::default();
        p.fns.insert(
            0,
            FnSig {
                name: "f".into(),
                params: vec![param("o", None)],
                ..Default::default()
            },
        );
        let opt_int = Ty::Opt(Box::new(Ty::Int));
        p.bindings.insert(at(src, "stick a"), opt_int.clone());
        p.bindings.insert(at(src, "stick b"), Ty::Int);
        p.bindings.insert(at(src, "stick c"), Ty::Int);
        p.bindings.insert(at(src, "stick d"), Ty::Int);
        p.bindings.insert(at(src, "roll w"), opt_int);
        p.bindings.insert(at(src, "stick e"), Ty::Int);
        let d = run(src, &p);
        // only the un-narrowed top-level `w` is an error
        assert_eq!(codes(&d), ["E1201"], "{d:?}");
        assert!(d[0].message.contains("found `Int?`"));
    }

    #[test]
    fn enum_constructors() {
        let src = "stick a = _cg_Status_failed(\"disk\")\nstick b = _cg_Status_failed(1)\nstick c = _cg_Status_ok(1)\nstick d = _cg_Status_nope()\n";
        let mut p = types::Program::default();
        p.enums.insert(
            "Status".into(),
            types::EnumDecl {
                name: "Status".into(),
                generics: vec![],
                variants: vec![
                    types::Variant {
                        name: "ok".into(),
                        fields: vec![],
                        at: At::default(),
                    },
                    types::Variant {
                        name: "failed".into(),
                        fields: vec![("reason".into(), Ty::Text)],
                        at: At::default(),
                    },
                ],
                public: false,
                at: At::default(),
            },
        );
        assert_eq!(codes(&run(src, &p)), ["E1201", "E1208", "E1207"]);
    }

    #[test]
    fn spans_are_mapped_to_the_original() {
        let src = "stick x = _cg_check(\"hi\", {k: \"int\"}, \"x\")\n";
        let mut p = types::Program::default();
        p.bindings.insert(0, Ty::Int);
        let ast = cigscript::syntax::parse(src).unwrap();
        let d = check(&ast, &p, &|o| o + 100);
        assert_eq!(d[0].span.unwrap().start, 100 + at(src, "\"hi\""));
    }

    #[test]
    fn a_clean_typed_program() {
        let src = "pull size_of(p) {\n  snuff _cg_check(fs.size(p), {k: \"int\"}, \"result\")\n}\n\
stick m = _cg_new_Move({src: \"a.txt\", dst: \"b/a.txt\"})\n\
stick n = _cg_check(size_of(m.src), {k: \"int\"}, \"n\")\n\
roll names = _cg_check([], {k: \"list\", of: {k: \"text\"}}, \"names\")\n\
names = [\"a\", \"b\"]\n\
for s in names {\n  stick t = _cg_check(s, {k: \"text\"}, \"t\")\n}\n\
stick total = _cg_check(len(names) + 1, {k: \"int\"}, \"total\")\n\
stick f = pack(a, b) => a + b\nstick g = f(1, 2)\n";
        let mut p = types::Program::default();
        p.records.insert("Move".into(), move_rec());
        p.fns.insert(
            0,
            FnSig {
                name: "size_of".into(),
                params: vec![param("p", Some(Ty::Path))],
                ret: Some(Ty::Int),
                ..Default::default()
            },
        );
        p.bindings.insert(at(src, "stick n"), Ty::Int);
        p.bindings
            .insert(at(src, "roll names"), Ty::List(Box::new(Ty::Text)));
        p.bindings.insert(at(src, "stick t"), Ty::Text);
        p.bindings.insert(at(src, "stick total"), Ty::Int);
        assert_eq!(codes(&run(src, &p)), Vec::<&str>::new());
    }
}
