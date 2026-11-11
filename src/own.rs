// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Ownership at full (#12): `owned`, `borrow` and `move`, checked statically
//! and per block, on cig's AST of the lowered text.
//!
//! - A name given to an `owned` parameter, or to `move`, is gone: using it
//!   again in that block is `E1501`, naming where it went.
//! - A `borrow` parameter is the caller's: returning it, storing it (into a
//!   list, a map or a field) or giving it away (to `owned`, to `move`) is
//!   `E1502`.
//!
//! The check is conservative: a move inside a branch or a loop body ends
//! with that block, so it never reports a use that might be fine. Ownership
//! is erased when the program runs: `move x` is `x`.

use crate::types::{self, Mode};
use cigscript::diagnostics::{Diagnostic, Kind};
use cigscript::syntax::ast::*;
use cigscript::syntax::span::Span;
use std::collections::{HashMap, HashSet};

/// Check every function and the top level.
pub fn check(
    ast: &Program,
    prog: &types::Program,
    original: &dyn Fn(usize) -> usize,
) -> Vec<Diagnostic> {
    let mut modes: HashMap<String, Vec<Mode>> = HashMap::new();
    for s in prog.fns.values() {
        modes.insert(s.name.clone(), s.params.iter().map(|p| p.mode).collect());
    }
    // A module's functions, as this file calls them: `plan.take` and, for a
    // name brought in with braces, `take`.
    for (k, s) in &prog.module_fns {
        modes.insert(k.clone(), s.params.iter().map(|p| p.mode).collect());
    }
    let any =
        modes.values().any(|m| m.iter().any(|x| *x != Mode::Shared)) || !prog.moves.is_empty();
    if !any {
        return Vec::new();
    }
    let mut o = Own {
        prog,
        modes,
        original,
        diags: Vec::new(),
        reported: HashSet::new(),
        borrowed: HashSet::new(),
        lambdas: 0,
    };
    let mut moved = HashMap::new();
    o.stmts(&ast.body, &mut moved);
    o.diags
}

struct Own<'a> {
    prog: &'a types::Program,
    modes: HashMap<String, Vec<Mode>>,
    original: &'a dyn Fn(usize) -> usize,
    diags: Vec<Diagnostic>,
    reported: HashSet<(String, usize)>,
    /// The `borrow` parameters of the function being checked.
    borrowed: HashSet<String>,
    /// How many function values (`pack`) enclose the expression.
    lambdas: usize,
}

/// Names that are gone, and where each went.
type Moved = HashMap<String, Span>;

impl Own<'_> {
    fn err(&mut self, code: &'static str, msg: String, span: Span, hint: String) {
        let (s, e) = ((self.original)(span.start), (self.original)(span.end));
        if !self.reported.insert((msg.clone(), s)) {
            return;
        }
        let mut d = Diagnostic::new(Kind::Check, msg).with_hint(hint);
        d.code = code;
        d.span = Some(Span::new(s, e.max(s), span.line, span.col));
        d.line = Some(span.line);
        d.col = Some(span.col);
        self.diags.push(d);
    }

    fn stmts(&mut self, ss: &[Stmt], moved: &mut Moved) {
        for s in ss {
            self.stmt(s, moved);
        }
    }

    /// A nested block: its moves end with it.
    fn block(&mut self, b: &Block, moved: &Moved) {
        let mut inner = moved.clone();
        self.stmts(&b.stmts, &mut inner);
    }

    fn stmt(&mut self, s: &Stmt, moved: &mut Moved) {
        match s {
            Stmt::Declare { name, value, .. } => {
                self.expr(value, moved);
                if name.starts_with("__cg_v") {
                    // `snuff x` with a result type is lowered through a
                    // temporary: returning a borrow.
                    self.returned(value);
                } else if !name.starts_with("__cg_") {
                    self.stored(value);
                }
                moved.remove(name);
            }
            Stmt::Assign { target, value, .. } => {
                self.expr(value, moved);
                match target {
                    AssignTarget::Var(n) => {
                        moved.remove(n);
                    }
                    AssignTarget::Index { object, index } => {
                        self.expr(object, moved);
                        self.expr(index, moved);
                        self.stored(value);
                    }
                    AssignTarget::Member { object, .. } => {
                        self.expr(object, moved);
                        self.stored(value);
                    }
                }
            }
            Stmt::Pull(f) => self.pull(f),
            Stmt::Chain(c) => {
                for e in &c.steps {
                    self.expr(e, moved);
                }
            }
            Stmt::Pack(p) => {
                for e in &p.roots {
                    self.expr(e, moved);
                }
            }
            Stmt::Snuff { value, .. } => {
                if let Some(v) = value {
                    self.expr(v, moved);
                    self.returned(v);
                }
            }
            Stmt::Exhale { values, .. } => {
                for v in values {
                    self.expr(v, moved);
                }
            }
            Stmt::Cough { value, .. } => self.expr(value, moved),
            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                for (c, b) in branches {
                    self.expr(c, moved);
                    self.block(b, moved);
                }
                if let Some(b) = otherwise {
                    self.block(b, moved);
                }
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond, moved);
                let mut inner = moved.clone();
                self.stmts(&body.stmts, &mut inner);
                self.moved_in_loop(&body.stmts, moved, &inner, None);
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                self.expr(iter, moved);
                let mut inner = moved.clone();
                inner.remove(var);
                self.stmts(&body.stmts, &mut inner);
                self.moved_in_loop(&body.stmts, moved, &inner, Some(var));
            }
            Stmt::Break(_) | Stmt::Continue(_) => {}
            Stmt::Try {
                body,
                catch_var,
                handler,
                finally,
                ..
            } => {
                self.block(body, moved);
                if let Some(h) = handler {
                    let mut inner = moved.clone();
                    if let Some(v) = catch_var {
                        inner.remove(v);
                    }
                    self.stmts(&h.stmts, &mut inner);
                }
                if let Some(f) = finally {
                    self.block(f, moved);
                }
            }
            Stmt::Burn {
                label,
                body,
                unburn,
                ..
            } => {
                if let Some(l) = label {
                    self.expr(l, moved);
                }
                self.block(body, moved);
                if let Some(u) = unburn {
                    self.block(u, moved);
                }
            }
            Stmt::Expr(e) => self.expr(e, moved),
        }
    }

    /// A name from outside a loop, given away inside it: the next time
    /// round, it is gone.
    fn moved_in_loop(
        &mut self,
        body: &[Stmt],
        before: &Moved,
        after: &Moved,
        var: Option<&String>,
    ) {
        let declared: HashSet<&String> = body
            .iter()
            .filter_map(|s| match s {
                Stmt::Declare { name, .. } => Some(name),
                _ => None,
            })
            .chain(var)
            .collect();
        let mut gone: Vec<(&String, &Span)> = after
            .iter()
            .filter(|(n, _)| !before.contains_key(*n) && !declared.contains(n))
            .collect();
        gone.sort_by_key(|(_, sp)| sp.start);
        for (n, sp) in gone {
            self.err(
                "E1501",
                format!("`{n}` is given away inside a loop, so the next time round it is gone"),
                *sp,
                format!("give away a copy (`{n}.copy()`), or make `{n}` inside the loop"),
            );
        }
    }

    fn pull(&mut self, f: &FnDecl) {
        if f.name.starts_with("_cg_") && !f.name.starts_with("_cg_impl_") {
            return;
        }
        let modes = self
            .prog
            .fns
            .get(&f.span.start)
            .map(|s| s.params.iter().map(|p| p.mode).collect::<Vec<_>>());
        let saved = std::mem::take(&mut self.borrowed);
        if let Some(m) = modes {
            for (p, mode) in f.params.iter().zip(m) {
                if mode == Mode::Borrow {
                    self.borrowed.insert(p.clone());
                }
            }
        }
        let mut moved = Moved::new();
        self.stmts(&f.body.stmts, &mut moved);
        self.borrowed = saved;
    }

    fn is_borrow(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) if self.borrowed.contains(n) => Some(n.clone()),
            _ => None,
        }
    }

    fn returned(&mut self, v: &Expr) {
        if let Some(n) = self.is_borrow(v) {
            self.err(
                "E1502",
                format!("`{n}` is borrowed, so it cannot be returned"),
                v.span,
                format!("return a copy (`{n}.copy()`), or take it `owned`"),
            );
        }
    }

    fn stored(&mut self, v: &Expr) {
        if let Some(n) = self.is_borrow(v) {
            self.err(
                "E1502",
                format!("`{n}` is borrowed, so it cannot be stored"),
                v.span,
                format!("store a copy (`{n}.copy()`), or take it `owned`"),
            );
        }
    }

    /// `e` is used; then, if it is a name given away, it is gone.
    fn give(&mut self, e: &Expr, how: &str, moved: &mut Moved) {
        if let Some(n) = self.is_borrow(e) {
            self.err(
                "E1502",
                format!("`{n}` is borrowed, so it cannot be given away ({how})"),
                e.span,
                format!("give a copy (`{n}.copy()`), or take `{n}` `owned`"),
            );
            return;
        }
        if let ExprKind::Ident(n) = &e.kind {
            moved.insert(n.clone(), e.span);
        }
    }

    fn expr(&mut self, e: &Expr, moved: &mut Moved) {
        match &e.kind {
            ExprKind::Ident(n) => {
                if let Some(at) = moved.get(n).copied() {
                    let line = at.line;
                    self.err(
                        "E1501",
                        format!("`{n}` was given away on line {line} and cannot be used after"),
                        e.span,
                        format!("use it before it is moved, or move a copy (`{n}.copy()`)"),
                    );
                }
                if self.lambdas > 0 {
                    if let Some(b) = self.is_borrow(e) {
                        self.err(
                            "E1502",
                            format!("`{b}` is borrowed, so a function value cannot keep it"),
                            e.span,
                            format!("pass it in as an argument, or capture a copy (`{b}.copy()`)"),
                        );
                    }
                }
                if self.prog.moves.contains(&e.span.start) {
                    self.give(e, "to `move`", moved);
                }
            }
            ExprKind::Null | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) => {}
            ExprKind::Str(parts) => {
                for p in parts {
                    if let StrPiece::Expr(x) = p {
                        self.expr(x, moved);
                    }
                }
            }
            ExprKind::List(items) => {
                for i in items {
                    self.expr(i, moved);
                    self.stored(i);
                }
            }
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    if let MapKey::Dynamic(x) = k {
                        self.expr(x, moved);
                    }
                    self.expr(v, moved);
                    self.stored(v);
                }
            }
            ExprKind::Unary { expr, .. } => self.expr(expr, moved),
            ExprKind::Binary { lhs, rhs, .. } => {
                self.expr(lhs, moved);
                self.expr(rhs, moved);
            }
            ExprKind::Call { callee, args } => {
                self.expr(callee, moved);
                let modes = match &callee.kind {
                    ExprKind::Ident(f) => self.modes.get(f).cloned(),
                    _ => None,
                };
                for (i, a) in args.iter().enumerate() {
                    self.expr(a, moved);
                    if modes.as_ref().and_then(|m| m.get(i)) == Some(&Mode::Owned) {
                        let f = match &callee.kind {
                            ExprKind::Ident(f) => f.clone(),
                            _ => String::new(),
                        };
                        self.give(a, &format!("to `{f}`, which takes it `owned`"), moved);
                    }
                }
            }
            ExprKind::Method {
                receiver,
                name,
                args,
                ..
            } => {
                self.expr(receiver, moved);
                // `plan.take(a)`: a module's function, with its modes.
                let modes = match &receiver.kind {
                    ExprKind::Ident(l) => self.modes.get(&format!("{l}.{name}")).cloned(),
                    _ => None,
                };
                for (i, a) in args.iter().enumerate() {
                    self.expr(a, moved);
                    if matches!(name.as_str(), "push" | "insert" | "set") {
                        self.stored(a);
                    }
                    if modes.as_ref().and_then(|m| m.get(i)) == Some(&Mode::Owned) {
                        self.give(a, &format!("to `{name}`, which takes it `owned`"), moved);
                    }
                }
            }
            ExprKind::Member { object, .. } => self.expr(object, moved),
            ExprKind::Index { object, index } => {
                self.expr(object, moved);
                self.expr(index, moved);
            }
            ExprKind::Lambda { body, .. } => {
                let mut inner = moved.clone();
                self.lambdas += 1;
                match body {
                    LambdaBody::Expr(x) => self.expr(x, &mut inner),
                    LambdaBody::Block(b) => self.stmts(&b.stmts, &mut inner),
                }
                self.lambdas -= 1;
            }
            ExprKind::Pipe { lhs, rhs } => {
                self.expr(lhs, moved);
                self.expr(rhs, moved);
            }
        }
    }
}
