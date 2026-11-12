// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! What a strict checker says before a program runs and that is not a type
//! error (#22, L1-L4): a parameter nobody typed (`W1230`), code that cannot
//! run (`W1231`), a name nobody reads (`W1232`) and a result thrown away
//! (`W1233`).
//!
//! It reads cig's AST of the lowered text, beside the declarations the
//! lowering found. Only text the programmer wrote is judged: what the
//! lowering wrote (its type checks, its `match` bookkeeping, its helpers)
//! never earns a warning.

use std::collections::{HashMap, HashSet};

use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::ast::*;
use cigscript::syntax::span::Span;

use super::stdlib;
use super::{match_ends, stmt_ends};
use crate::features::Strength;
use crate::types::{self, Ty};

/// The flags of `cigar check --deny`: which warnings are errors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Deny {
    /// `--deny any`: a parameter with no type is an error at medium.
    pub any: bool,
    /// `--deny warnings`: every warning is an error.
    pub warnings: bool,
}

/// What the pass found.
#[derive(Default)]
pub struct Found {
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
}

/// Run the pass. `original` maps a lowered offset to the file as written;
/// `written` says whether the programmer wrote the text at a lowered offset;
/// `text` is the lowered text.
pub fn run(
    ast: &Program,
    prog: &types::Program,
    text: &str,
    strength: Strength,
    deny: Deny,
    original: &dyn Fn(usize) -> usize,
    written: &dyn Fn(usize) -> bool,
) -> Found {
    let mut l = Lint {
        prog,
        text,
        strength,
        deny,
        original,
        written,
        out: Found::default(),
        scopes: vec![HashMap::new()],
        decls: Vec::new(),
        free_reads: HashSet::new(),
        imports: HashSet::new(),
        fn_rets: HashMap::new(),
        method_names: HashSet::new(),
        seen: HashSet::new(),
    };
    for s in prog.fns.values() {
        l.fn_rets.insert(s.name.clone(), s.clone());
    }
    for im in &prog.impls {
        for m in &im.methods {
            l.method_names.insert(m.name.clone());
        }
    }
    for i in prog.interfaces.values() {
        for m in &i.methods {
            l.method_names.insert(m.name.clone());
        }
    }
    for s in &ast.body {
        if let Stmt::Pull(f) = s {
            l.untyped_params(f);
        }
    }
    l.block_stmts(&ast.body);
    l.finish();
    l.out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum What {
    Local,
    Param,
    Import,
    /// Not judged: a loop variable, a lambda's parameter, a catch name.
    Quiet,
}

struct Decl {
    name: String,
    at: Span,
    what: What,
    used: bool,
    top: bool,
}

struct Lint<'a> {
    prog: &'a types::Program,
    text: &'a str,
    strength: Strength,
    deny: Deny,
    original: &'a dyn Fn(usize) -> usize,
    written: &'a dyn Fn(usize) -> bool,
    out: Found,
    scopes: Vec<HashMap<String, usize>>,
    decls: Vec<Decl>,
    /// Names read that no scope held when they were read: a function reads
    /// a top-level name that is declared after it.
    free_reads: HashSet<String>,
    imports: HashSet<String>,
    fn_rets: HashMap<String, types::FnSig>,
    /// Every method name a program declares (an `impl` or an interface): a
    /// call of one is not a call of the built-in method of that name.
    method_names: HashSet<String>,
    seen: HashSet<(&'static str, usize)>,
}

/// Methods that give a value back and change nothing, whatever they are
/// called on. A method that also changes its receiver (`push`, `pop`,
/// `insert`, `remove`, `set`, `merge`, `sort`, `reverse`) is not here: its
/// result may be dropped.
const PURE: &[&str] = &[
    "len",
    "is_empty",
    "copy",
    "keys",
    "values",
    "entries",
    "has",
    "contains",
    "starts_with",
    "ends_with",
    "upper",
    "lower",
    "trim",
    "trim_start",
    "trim_end",
    "split",
    "lines",
    "chars",
    "join",
    "index_of",
    "first",
    "last",
    "slice",
    "map",
    "filter",
    "any",
    "all",
    "find",
    "flatten",
    "sum",
    "min",
    "max",
    "zip",
    "enumerate",
    "chunks",
    "unique",
    "to_int",
    "to_float",
    "repeat",
    "pad_left",
    "pad_right",
    "replace",
];

impl<'a> Lint<'a> {
    // ------------------------------------------------------ diagnostics

    fn diag(&self, code: &'static str, msg: String, span: Span, hint: String) -> Diagnostic {
        let (s, e) = ((self.original)(span.start), (self.original)(span.end));
        let mut d = Diagnostic::new(Kind::Check, msg);
        d.code = code;
        d.hint = Some(hint);
        d.span = Some(Span::new(s, e.max(s), span.line, span.col));
        if span.line > 0 {
            d.line = Some(span.line);
            d.col = Some(span.col);
        }
        d
    }

    fn warn(&mut self, code: &'static str, msg: String, span: Span, hint: String) {
        if !self.seen.insert((code, span.start)) {
            return;
        }
        let d = self.diag(code, msg, span, hint);
        self.out.warnings.push(d);
    }

    // --------------------------------------------------------- L1: Any

    /// An unannotated parameter of a top-level `pull`: typed by what its body
    /// does with it where that says one type, otherwise `W1230`, and an error
    /// at full or under `--deny any`.
    fn untyped_params(&mut self, f: &FnDecl) {
        if f.name.starts_with('_') || !(self.written)(f.span.start) {
            return;
        }
        let Some(sig) = self.prog.fns.get(&f.span.start).cloned() else {
            return;
        };
        // The header, to find where each name is written.
        let body_at = f.body.span.start.max(f.span.start);
        let header = self.text.get(f.span.start..body_at).unwrap_or("");
        let open = header.find('(').map(|i| i + 1).unwrap_or(0);
        let mut from = open;
        for (i, pn) in f.params.iter().enumerate() {
            let at = word_at(header, pn, from);
            if let Some((s, e)) = at {
                from = e;
                let declared = sig.params.get(i).and_then(|p| p.ty.as_ref());
                if declared.is_some() || pn == "self" || pn.starts_with('_') {
                    continue;
                }
                if self.inferred(f, &sig, pn) {
                    continue;
                }
                let span = place(f.span, header, s, e);
                let strict = self.strength == Strength::Full || self.deny.any;
                let hint = if self.strength == Strength::Full {
                    format!("write its type: `{pn}: Int`; at full strength a parameter has a type (`{pn}: Any` says it is meant)")
                } else {
                    format!("write its type: `{pn}: Int`, or `{pn}: Any` to say it is meant")
                };
                let msg = format!("`{pn}` is `Any`: write its type");
                if strict {
                    if self.seen.insert(("W1230", span.start)) {
                        let d = self.diag("W1230", msg, span, hint);
                        self.out.errors.push(d);
                    }
                } else {
                    self.warn("W1230", msg, span, hint);
                }
            }
        }
    }

    /// Does the body say what type `p` is? Passed to a typed parameter,
    /// given back where the result type is written, or added to text.
    fn inferred(&self, f: &FnDecl, sig: &types::FnSig, p: &str) -> bool {
        let found = std::cell::Cell::new(false);
        let ret_typed = sig
            .ret
            .as_ref()
            .is_some_and(|r| !matches!(r, Ty::Any | Ty::Never | Ty::Param(_)));
        walk_block(
            &f.body,
            &mut |s| {
                if let Stmt::Declare { name, value, .. } = s {
                    if ret_typed
                        && name.starts_with("__cg_v")
                        && matches!(&value.kind, ExprKind::Ident(n) if n == p)
                    {
                        found.set(true);
                    }
                }
            },
            &mut |e| match &e.kind {
                ExprKind::Binary {
                    op: BinOp::Add,
                    lhs,
                    rhs,
                } => {
                    let is_p = |x: &Expr| matches!(&x.kind, ExprKind::Ident(n) if n == p);
                    let is_text = |x: &Expr| matches!(&x.kind, ExprKind::Str(_));
                    if (is_p(lhs) && is_text(rhs)) || (is_p(rhs) && is_text(lhs)) {
                        found.set(true);
                    }
                }
                ExprKind::Call { callee, args } => {
                    for (i, a) in args.iter().enumerate() {
                        if !matches!(&a.kind, ExprKind::Ident(n) if n == p) {
                            continue;
                        }
                        let want = self.callee_param(callee, i);
                        if want.is_some_and(|t| !matches!(t, Ty::Any | Ty::Param(_))) {
                            found.set(true);
                        }
                    }
                }
                _ => {}
            },
        );
        found.get()
    }

    fn callee_param(&self, callee: &Expr, i: usize) -> Option<Ty> {
        match &callee.kind {
            ExprKind::Ident(g) => {
                if let Some(s) = self.fn_rets.get(g) {
                    return s.params.get(i).and_then(|p| p.ty.clone());
                }
                stdlib::function(None, g).and_then(|s| s.params.get(i).cloned())
            }
            ExprKind::Member { object, name, .. } => match &object.kind {
                ExprKind::Ident(m) if stdlib::is_module(m) => {
                    stdlib::function(Some(m), name).and_then(|s| s.params.get(i).cloned())
                }
                _ => None,
            },
            _ => None,
        }
    }

    // ------------------------------------------------- statements and scopes

    fn declare(&mut self, name: &str, at: Span, what: What) {
        let top = self.scopes.len() == 1;
        self.decls.push(Decl {
            name: name.to_string(),
            at,
            what,
            used: false,
            top,
        });
        let id = self.decls.len() - 1;
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string(), id);
        }
    }

    fn read(&mut self, name: &str) {
        for s in self.scopes.iter().rev() {
            if let Some(&id) = s.get(name) {
                self.decls[id].used = true;
                return;
            }
        }
        self.free_reads.insert(name.to_string());
    }

    fn scoped(&mut self, f: impl FnOnce(&mut Self)) {
        self.scopes.push(HashMap::new());
        f(self);
        self.scopes.pop();
    }

    fn block(&mut self, b: &Block) {
        self.scoped(|l| l.block_stmts(&b.stmts));
    }

    fn block_stmts(&mut self, ss: &[Stmt]) {
        self.unreachable(ss);
        // A function is hoisted: its name is known to what is written before it.
        for s in ss {
            self.stmt(s);
        }
    }

    /// L2: the first statement the programmer wrote after one that always
    /// ends the path.
    fn unreachable(&mut self, ss: &[Stmt]) {
        let mut ended: Option<usize> = None;
        let mut i = 0;
        while i < ss.len() {
            let s = &ss[i];
            if let Some(end) = ended {
                if (self.written)(s.span().start) && !matches!(s, Stmt::Pull(_)) {
                    let why = match &ss[end] {
                        Stmt::Snuff { .. } => "`snuff`",
                        Stmt::Cough { .. } => "`cough`",
                        Stmt::Break(_) => "`break`",
                        Stmt::Continue(_) => "`continue`",
                        Stmt::If { .. } => "every branch of the `if` above ends",
                        Stmt::While { .. } => "the loop above never ends",
                        _ => "the code above ends the path",
                    };
                    let line = ss[end].span().line;
                    let what = if why.starts_with('`') {
                        format!("the {why} on line {line} ends this path")
                    } else {
                        format!("{why} (line {line})")
                    };
                    self.warn(
                        "W1231",
                        "this code can never run".to_string(),
                        s.span(),
                        format!("{what}; remove this code, or move it before it"),
                    );
                    return;
                }
                i += 1;
                continue;
            }
            match s {
                Stmt::Break(_) | Stmt::Continue(_) => ended = Some(i),
                _ if is_arm(s) => {
                    // The run of arms of one lowered `match`.
                    let mut j = i;
                    while j + 1 < ss.len() && is_arm(&ss[j + 1]) {
                        j += 1;
                    }
                    if match_ends(&ss[i..=j]) {
                        ended = Some(j);
                        // the line of the `match` is its first arm's
                        i = j;
                    } else {
                        i = j;
                    }
                }
                _ if stmt_ends(s) => ended = Some(i),
                _ => {}
            }
            i += 1;
        }
    }

    fn stmt(&mut self, s: &Stmt) {
        match s {
            Stmt::Declare {
                name, value, span, ..
            } => {
                self.expr(value);
                let wrote = (self.written)(span.start);
                let judged = wrote && !name.starts_with('_');
                // `use a.b` is lowered to `stick b = _cg_mod_a_b()`, and
                // `use a.b.{c}` to `stick c = b.c`.
                let module = !wrote
                    && matches!(&value.kind, ExprKind::Call { callee, .. }
                        if matches!(&callee.kind, ExprKind::Ident(n) if n.starts_with("_cg_mod_")));
                let member = !wrote
                    && matches!(&value.kind, ExprKind::Member { object, .. }
                        if matches!(&object.kind, ExprKind::Ident(m) if self.imports.contains(m)));
                if module || member {
                    self.imports.insert(name.clone());
                }
                let (what, at) = if (module || member) && !name.starts_with('_') {
                    (What::Import, *span)
                } else if judged {
                    let at = self.name_span(name, *span);
                    (What::Local, at)
                } else {
                    (What::Quiet, *span)
                };
                self.declare(name, at, what);
            }
            Stmt::Assign {
                target, op, value, ..
            } => {
                self.expr(value);
                match target {
                    // Setting a name is not reading it; `x += 1` reads it.
                    AssignTarget::Var(n, _) => {
                        if *op != AssignOp::Set {
                            self.read(n);
                        }
                    }
                    AssignTarget::Index { object, index } => {
                        self.expr(object);
                        self.expr(index);
                    }
                    AssignTarget::Member { object, .. } => self.expr(object),
                }
            }
            Stmt::Pull(f) => self.pull(f),
            Stmt::Chain(c) => c.steps.iter().for_each(|e| self.expr(e)),
            Stmt::Pack(p) => p.roots.iter().for_each(|e| self.expr(e)),
            Stmt::Snuff { value, .. } => {
                if let Some(v) = value {
                    self.expr(v);
                }
            }
            Stmt::Exhale { values, .. } => values.iter().for_each(|e| self.expr(e)),
            Stmt::Cough { value, .. } => self.expr(value),
            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                for (c, b) in branches {
                    self.expr(c);
                    self.block(b);
                }
                if let Some(b) = otherwise {
                    self.block(b);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond);
                self.block(body);
            }
            Stmt::For {
                var,
                iter,
                body,
                span,
            } => {
                self.expr(iter);
                self.scoped(|l| {
                    l.declare(var, *span, What::Quiet);
                    l.block_stmts(&body.stmts);
                });
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Try {
                body,
                catch_var,
                handler,
                finally,
                span,
            } => {
                self.block(body);
                if let Some(h) = handler {
                    self.scoped(|l| {
                        if let Some(v) = catch_var {
                            l.declare(v, *span, What::Quiet);
                        }
                        l.block_stmts(&h.stmts);
                    });
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
                span,
                ..
            } => {
                if let Some(l) = label {
                    self.expr(l);
                }
                self.scoped(|l| {
                    if let Some(s) = state {
                        l.declare(s, *span, What::Quiet);
                    }
                    l.block_stmts(&body.stmts);
                    if let Some(u) = unburn {
                        l.block(u);
                    }
                });
            }
            Stmt::Expr(e) => {
                self.unused_result(e);
                self.expr(e);
            }
        }
    }

    fn pull(&mut self, f: &FnDecl) {
        // The lowering's own helpers, constructors and module bodies.
        if f.name.starts_with("_cg_") && !f.name.starts_with("_cg_impl_") {
            return;
        }
        // A function is a name too: a hoisted one is read by its callers.
        let own_fn = (self.written)(f.span.start) && !f.name.starts_with('_');
        self.scoped(|l| {
            let body_at = f.body.span.start.max(f.span.start);
            let header = l.text.get(f.span.start..body_at).unwrap_or("");
            let mut from = header.find('(').map(|i| i + 1).unwrap_or(0);
            for pn in &f.params {
                let found = word_at(header, pn, from);
                match found {
                    Some((s, e)) if own_fn && pn != "self" && !pn.starts_with('_') => {
                        from = e;
                        let at = place(f.span, header, s, e);
                        l.declare(pn, at, What::Param);
                    }
                    other => {
                        if let Some((_, e)) = other {
                            from = e;
                        }
                        l.declare(pn, f.span, What::Quiet);
                    }
                }
            }
            l.block_stmts(&f.body.stmts);
        });
    }

    /// The span of the name in a `stick` or `roll`: after the keyword.
    fn name_span(&self, name: &str, stmt: Span) -> Span {
        let rest = self.text.get(stmt.start..).unwrap_or("");
        let head: &str = rest.split(['=', '\n']).next().unwrap_or("");
        match word_at(head, name, 0) {
            Some((s, e)) => place(stmt, head, s, e),
            None => stmt,
        }
    }

    // --------------------------------------------------- expressions

    fn expr(&mut self, e: &Expr) {
        match &e.kind {
            ExprKind::Ident(n) => self.read(n),
            ExprKind::Str(parts) => {
                for p in parts {
                    if let StrPiece::Expr(x) = p {
                        self.expr(x);
                    }
                }
            }
            ExprKind::List(xs) => xs.iter().for_each(|x| self.expr(x)),
            ExprKind::Map(es) => {
                for (k, v) in es {
                    if let MapKey::Dynamic(x) = k {
                        self.expr(x);
                    }
                    self.expr(v);
                }
            }
            ExprKind::Unary { expr, .. } => self.expr(expr),
            ExprKind::Binary { lhs, rhs, .. } | ExprKind::Pipe { lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Call { callee, args } => {
                // The lowering's own type checks read a value to test it, and
                // that is not the programmer using it.
                if matches!(&callee.kind, ExprKind::Ident(n) if n == "_cg_fits" || n == "_cg_why") {
                    return;
                }
                self.expr(callee);
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Method { receiver, args, .. } => {
                self.expr(receiver);
                args.iter().for_each(|a| self.expr(a));
            }
            ExprKind::Member { object, .. } => self.expr(object),
            ExprKind::Index { object, index } => {
                self.expr(object);
                self.expr(index);
            }
            ExprKind::Lambda { params, body } => {
                self.scoped(|l| {
                    for p in params {
                        l.declare(p, e.span, What::Quiet);
                    }
                    match body {
                        LambdaBody::Expr(x) => l.expr(x),
                        LambdaBody::Block(b) => l.block_stmts(&b.stmts),
                    }
                });
            }
            ExprKind::Null | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) => {}
        }
    }

    // ------------------------------------------------------ L4: results

    /// A statement that is only a call: its result is dropped.
    fn unused_result(&mut self, e: &Expr) {
        if !(self.written)(e.span.start) {
            return;
        }
        match &e.kind {
            ExprKind::Method { name, .. }
                if PURE.contains(&name.as_str()) && !self.method_names.contains(name) =>
            {
                self.warn(
                    "W1233",
                    format!("the result of `.{name}()` is dropped"),
                    e.span,
                    "`.{name}()` changes nothing, so calling it for nothing does nothing; use its result, or write `_ = ...` to say it is meant"
                        .replace("{name}", name),
                );
            }
            ExprKind::Call { callee, .. } => {
                let ExprKind::Ident(g) = &callee.kind else {
                    return;
                };
                let Some(sig) = self.fn_rets.get(g) else {
                    return;
                };
                let gives = sig
                    .ret
                    .as_ref()
                    .is_some_and(|r| !matches!(r, Ty::Any | Ty::Null | Ty::Never));
                if !gives || g.starts_with('_') {
                    return;
                }
                let t = sig.ret.clone().map(|r| r.to_string()).unwrap_or_default();
                let shown = super::shown_name(g);
                self.warn(
                    "W1233",
                    format!("the `{t}` that `{shown}` gives back is dropped"),
                    e.span,
                    format!("use the result, or write `_ = {shown}(...)` to say it is meant"),
                );
            }
            _ => {}
        }
    }

    // ------------------------------------------------------ L3: unused

    fn finish(&mut self) {
        let free = std::mem::take(&mut self.free_reads);
        let mut found = Vec::new();
        for d in &self.decls {
            let used = d.used || (d.top && free.contains(&d.name));
            if used || d.what == What::Quiet {
                continue;
            }
            found.push((d.name.clone(), d.at, d.what));
        }
        for (name, at, what) in found {
            let (msg, hint) = match what {
                What::Param => (
                    format!("parameter `{name}` is never used"),
                    format!("remove it, or write `_{name}` if the function has to take it"),
                ),
                What::Import => (
                    format!("`{name}` is imported and never used"),
                    "remove the `use`".to_string(),
                ),
                _ => (
                    format!("`{name}` is never used"),
                    format!("remove it, or write `_{name}` if it is meant"),
                ),
            };
            self.warn("W1232", msg, at, hint);
        }
    }
}

/// The span of `[s, e)` in `text`, which starts where `base` does.
fn place(base: Span, text: &str, s: usize, e: usize) -> Span {
    let before = &text[..s];
    let lines = before.matches('\n').count() as u32;
    let col = match before.rfind('\n') {
        None => base.col + before.chars().count() as u32,
        Some(i) => before[i + 1..].chars().count() as u32 + 1,
    };
    Span::new(base.start + s, base.start + e, base.line + lines, col)
}

fn is_arm(s: &Stmt) -> bool {
    super::is_match_arm(s)
}

/// Where `word` is in `s` as a whole name, at or after `from`.
fn word_at(s: &str, word: &str, from: usize) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut at = from;
    while let Some(i) = s.get(at..)?.find(word) {
        let (st, en) = (at + i, at + i + word.len());
        let before = st == 0 || !ident(b[st - 1]);
        let after = en >= b.len() || !ident(b[en]);
        if before && after {
            return Some((st, en));
        }
        at = st + 1;
    }
    None
}

/// Every statement and expression under a block, outside nested functions'
/// own parameters' scope (lambdas and nested functions are walked: a use in
/// one is a use).
fn walk_block(b: &Block, fs: &mut dyn FnMut(&Stmt), fe: &mut dyn FnMut(&Expr)) {
    for s in &b.stmts {
        walk_stmt(s, fs, fe);
    }
}

fn walk_stmt(s: &Stmt, fs: &mut dyn FnMut(&Stmt), fe: &mut dyn FnMut(&Expr)) {
    fs(s);
    match s {
        Stmt::Declare { value, .. } => walk_expr(value, fs, fe),
        Stmt::Assign { target, value, .. } => {
            walk_expr(value, fs, fe);
            match target {
                AssignTarget::Var(..) => {}
                AssignTarget::Index { object, index } => {
                    walk_expr(object, fs, fe);
                    walk_expr(index, fs, fe);
                }
                AssignTarget::Member { object, .. } => walk_expr(object, fs, fe),
            }
        }
        Stmt::Pull(f) => walk_block(&f.body, fs, fe),
        Stmt::Chain(c) => c.steps.iter().for_each(|e| walk_expr(e, fs, fe)),
        Stmt::Pack(p) => p.roots.iter().for_each(|e| walk_expr(e, fs, fe)),
        Stmt::Snuff { value, .. } => {
            if let Some(v) = value {
                walk_expr(v, fs, fe);
            }
        }
        Stmt::Exhale { values, .. } => values.iter().for_each(|e| walk_expr(e, fs, fe)),
        Stmt::Cough { value, .. } => walk_expr(value, fs, fe),
        Stmt::If {
            branches,
            otherwise,
            ..
        } => {
            for (c, b) in branches {
                walk_expr(c, fs, fe);
                walk_block(b, fs, fe);
            }
            if let Some(b) = otherwise {
                walk_block(b, fs, fe);
            }
        }
        Stmt::While { cond, body, .. } => {
            walk_expr(cond, fs, fe);
            walk_block(body, fs, fe);
        }
        Stmt::For { iter, body, .. } => {
            walk_expr(iter, fs, fe);
            walk_block(body, fs, fe);
        }
        Stmt::Break(_) | Stmt::Continue(_) => {}
        Stmt::Try {
            body,
            handler,
            finally,
            ..
        } => {
            walk_block(body, fs, fe);
            if let Some(h) = handler {
                walk_block(h, fs, fe);
            }
            if let Some(f) = finally {
                walk_block(f, fs, fe);
            }
        }
        Stmt::Burn {
            label,
            body,
            unburn,
            ..
        } => {
            if let Some(l) = label {
                walk_expr(l, fs, fe);
            }
            walk_block(body, fs, fe);
            if let Some(u) = unburn {
                walk_block(u, fs, fe);
            }
        }
        Stmt::Expr(e) => walk_expr(e, fs, fe),
    }
}

fn walk_expr(e: &Expr, fs: &mut dyn FnMut(&Stmt), fe: &mut dyn FnMut(&Expr)) {
    fe(e);
    match &e.kind {
        ExprKind::Str(parts) => {
            for p in parts {
                if let StrPiece::Expr(x) = p {
                    walk_expr(x, fs, fe);
                }
            }
        }
        ExprKind::List(xs) => xs.iter().for_each(|x| walk_expr(x, fs, fe)),
        ExprKind::Map(es) => {
            for (k, v) in es {
                if let MapKey::Dynamic(x) = k {
                    walk_expr(x, fs, fe);
                }
                walk_expr(v, fs, fe);
            }
        }
        ExprKind::Unary { expr, .. } => walk_expr(expr, fs, fe),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Pipe { lhs, rhs } => {
            walk_expr(lhs, fs, fe);
            walk_expr(rhs, fs, fe);
        }
        ExprKind::Call { callee, args } => {
            walk_expr(callee, fs, fe);
            args.iter().for_each(|a| walk_expr(a, fs, fe));
        }
        ExprKind::Method { receiver, args, .. } => {
            walk_expr(receiver, fs, fe);
            args.iter().for_each(|a| walk_expr(a, fs, fe));
        }
        ExprKind::Member { object, .. } => walk_expr(object, fs, fe),
        ExprKind::Index { object, index } => {
            walk_expr(object, fs, fe);
            walk_expr(index, fs, fe);
        }
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(x) => walk_expr(x, fs, fe),
            LambdaBody::Block(b) => walk_block(b, fs, fe),
        },
        _ => {}
    }
}
