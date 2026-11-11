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
//! A binding moves too (#22 S5): `stick alias = data` gives `data` away when
//! it is a list, a map, a record or a channel, and so does storing it in a
//! list, a map or a field, or sending it.
//!
//! A function value (`pack`) that reads a name borrows it; giving the name
//! away while the function is still used afterwards is `E1503` (S6). One
//! that is stored or sent, or written `move pack`, owns what it captures.
//!
//! A name given away on some paths is gone after they join, as in Rust
//! (S7): using it is `E1503`, naming the branch. A path that ends in `snuff`
//! or `cough` does not join; one that ends in `break` joins after the loop.
//!
//! Ownership is erased when the program runs: `move x` is `x`.

use crate::types::{self, Mode, Ty};
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
    let mut rets: HashMap<String, Option<Ty>> = HashMap::new();
    for s in prog.fns.values() {
        rets.insert(s.name.clone(), s.ret.clone());
    }
    for (k, s) in &prog.module_fns {
        rets.insert(k.clone(), s.ret.clone());
    }
    let mut o = Own {
        prog,
        modes,
        rets,
        original,
        diags: Vec::new(),
        reported: HashSet::new(),
        borrowed: HashSet::new(),
        lambdas: 0,
        heavy: HashSet::new(),
        closures: HashMap::new(),
        loops: Vec::new(),
    };
    let mut moved = HashMap::new();
    o.stmts(&ast.body, &mut moved);
    o.diags
}

struct Own<'a> {
    prog: &'a types::Program,
    modes: HashMap<String, Vec<Mode>>,
    /// The declared result of each function, by name.
    rets: HashMap<String, Option<Ty>>,
    original: &'a dyn Fn(usize) -> usize,
    diags: Vec<Diagnostic>,
    reported: HashSet<(String, usize)>,
    /// The `borrow` parameters of the function being checked.
    borrowed: HashSet<String>,
    /// How many function values (`pack`) enclose the expression.
    lambdas: usize,
    /// Names known to hold a list, a map, a record or a channel: the values a
    /// binding moves. Anything not known is treated as a copy.
    heavy: HashSet<String>,
    /// Function values that borrow names: the names, and where the function
    /// is written.
    closures: HashMap<String, (Vec<String>, Span)>,
    /// The loops being walked, innermost last.
    loops: Vec<LoopCtx>,
}

/// Where a loop's `break` and `continue` leave the moves.
#[derive(Default)]
struct LoopCtx {
    breaks: Vec<(Moved, u32)>,
    continues: Vec<Moved>,
}

/// Why a name is gone.
#[derive(Clone, Debug)]
enum How {
    /// Given away: to an `owned` parameter, to `move`, to a binding.
    Plain,
    /// Moved into a function value written `move pack` or stored.
    Captured,
    /// Given away on some paths only; says which.
    Maybe(String),
    /// The key is a function value that borrows `name`, which was given away
    /// since; using the function is the error. The span is the function's.
    Borrowed(String, Span),
}

#[derive(Clone, Debug)]
struct Gone {
    at: Span,
    how: How,
}

/// Names that are gone, and where each went.
type Moved = HashMap<String, Gone>;

fn is_heavy_ty(t: &Ty) -> bool {
    matches!(
        t,
        Ty::List(_) | Ty::Map(_) | Ty::Record(..) | Ty::Channel(_)
    )
}

/// The names a loop body declares at its own level, and its variable.
fn declared_in<'a>(body: &'a [Stmt], var: Option<&'a String>) -> HashSet<&'a String> {
    body.iter()
        .filter_map(|s| match s {
            Stmt::Declare { name, .. } => Some(name),
            _ => None,
        })
        .chain(var)
        .collect()
}

/// Where the paths of a branch meet. A name given away on every path that
/// reaches the join is gone; on some of them, it may be.
fn join(moved: &mut Moved, ends: Vec<(Moved, String)>, complete: bool) {
    let mut fresh: Vec<(String, Gone, String, usize)> = Vec::new();
    for (state, label) in &ends {
        for (n, g) in state {
            if moved.contains_key(n) || fresh.iter().any(|(m, ..)| m == n) {
                continue;
            }
            let all = ends.iter().filter(|(s, _)| s.contains_key(n)).count();
            fresh.push((n.clone(), g.clone(), label.clone(), all));
        }
    }
    fresh.sort_by_key(|(_, g, ..)| g.at.start);
    for (n, g, label, count) in fresh {
        let how = match g.how {
            How::Plain | How::Captured if !(complete && count == ends.len()) => How::Maybe(label),
            other => other,
        };
        moved.insert(n, Gone { at: g.at, how });
    }
}

/// Every name an expression mentions.
fn names(e: &Expr, out: &mut Vec<String>) {
    match &e.kind {
        ExprKind::Ident(n) => out.push(n.clone()),
        ExprKind::Null | ExprKind::Bool(_) | ExprKind::Int(_) | ExprKind::Float(_) => {}
        ExprKind::Str(parts) => {
            for p in parts {
                if let StrPiece::Expr(x) = p {
                    names(x, out);
                }
            }
        }
        ExprKind::List(items) => items.iter().for_each(|i| names(i, out)),
        ExprKind::Map(entries) => {
            for (k, v) in entries {
                if let MapKey::Dynamic(x) = k {
                    names(x, out);
                }
                names(v, out);
            }
        }
        ExprKind::Unary { expr, .. } => names(expr, out),
        ExprKind::Binary { lhs, rhs, .. } | ExprKind::Pipe { lhs, rhs } => {
            names(lhs, out);
            names(rhs, out);
        }
        ExprKind::Call { callee, args } => {
            names(callee, out);
            args.iter().for_each(|a| names(a, out));
        }
        ExprKind::Method { receiver, args, .. } => {
            names(receiver, out);
            args.iter().for_each(|a| names(a, out));
        }
        ExprKind::Member { object, .. } => names(object, out),
        ExprKind::Index { object, index } => {
            names(object, out);
            names(index, out);
        }
        ExprKind::Lambda { body, .. } => match body {
            LambdaBody::Expr(x) => names(x, out),
            LambdaBody::Block(b) => block_names(&b.stmts, out),
        },
    }
}

fn block_names(ss: &[Stmt], out: &mut Vec<String>) {
    for s in ss {
        match s {
            Stmt::Declare { value, .. } => names(value, out),
            Stmt::Assign { target, value, .. } => {
                match target {
                    AssignTarget::Var(_) => {}
                    AssignTarget::Index { object, index } => {
                        names(object, out);
                        names(index, out);
                    }
                    AssignTarget::Member { object, .. } => names(object, out),
                }
                names(value, out);
            }
            Stmt::Snuff { value, .. } => {
                if let Some(v) = value {
                    names(v, out);
                }
            }
            Stmt::Exhale { values, .. } => values.iter().for_each(|v| names(v, out)),
            Stmt::Cough { value, .. } => names(value, out),
            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                for (c, b) in branches {
                    names(c, out);
                    block_names(&b.stmts, out);
                }
                if let Some(b) = otherwise {
                    block_names(&b.stmts, out);
                }
            }
            Stmt::While { cond, body, .. } => {
                names(cond, out);
                block_names(&body.stmts, out);
            }
            Stmt::For { iter, body, .. } => {
                names(iter, out);
                block_names(&body.stmts, out);
            }
            Stmt::Try {
                body,
                handler,
                finally,
                ..
            } => {
                block_names(&body.stmts, out);
                for b in [handler, finally].into_iter().flatten() {
                    block_names(&b.stmts, out);
                }
            }
            Stmt::Burn { body, unburn, .. } => {
                block_names(&body.stmts, out);
                if let Some(b) = unburn {
                    block_names(&b.stmts, out);
                }
            }
            Stmt::Expr(e) => names(e, out),
            Stmt::Pull(_) | Stmt::Chain(_) | Stmt::Pack(_) | Stmt::Break(_) | Stmt::Continue(_) => {
            }
        }
    }
}

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

    /// A run of statements. False when the path ends inside it (`snuff`,
    /// `cough`, `break`, `continue`): nothing after it runs.
    fn stmts(&mut self, ss: &[Stmt], moved: &mut Moved) -> bool {
        for s in ss {
            if !self.stmt(s, moved) {
                return false;
            }
        }
        true
    }

    /// A nested block: its moves end with it.
    fn block(&mut self, b: &Block, moved: &Moved) {
        let mut inner = moved.clone();
        self.stmts(&b.stmts, &mut inner);
    }

    /// `n` is given away; a function value that borrowed it is now stale.
    fn take(&mut self, n: &str, at: Span, how: How, moved: &mut Moved) {
        moved.insert(n.to_string(), Gone { at, how });
        let mut stale: Vec<(String, Span)> = self
            .closures
            .iter()
            .filter(|(f, (caps, _))| f.as_str() != n && caps.iter().any(|c| c == n))
            .map(|(f, (_, sp))| (f.clone(), *sp))
            .collect();
        stale.sort_by_key(|(_, sp)| sp.start);
        for (f, sp) in stale {
            moved.entry(f).or_insert(Gone {
                at,
                how: How::Borrowed(n.to_string(), sp),
            });
        }
    }

    /// Is the name known to hold a value a binding moves?
    fn moves_name(&self, e: &Expr) -> Option<String> {
        match &e.kind {
            ExprKind::Ident(n) if self.heavy.contains(n) && !self.borrowed.contains(n) => {
                Some(n.clone())
            }
            _ => None,
        }
    }

    /// Does the value hold a list, a map, a record or a channel?
    fn is_heavy(&self, e: &Expr) -> bool {
        match &e.kind {
            ExprKind::List(_) | ExprKind::Map(_) => true,
            ExprKind::Ident(n) => self.heavy.contains(n),
            ExprKind::Call { callee, .. } => match &callee.kind {
                ExprKind::Ident(f) => {
                    f == "_cg_channel"
                        || f.starts_with("_cg_new_")
                        || matches!(self.rets.get(f), Some(Some(t)) if is_heavy_ty(t))
                }
                _ => false,
            },
            ExprKind::Method { receiver, name, .. } => {
                if name == "copy" {
                    return self.is_heavy(receiver);
                }
                match &receiver.kind {
                    ExprKind::Ident(l) => {
                        matches!(self.rets.get(&format!("{l}.{name}")), Some(Some(t)) if is_heavy_ty(t))
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The names a function value reads from outside and that a binding
    /// would move.
    fn captures(&self, params: &[String], body: &LambdaBody) -> Vec<String> {
        let mut all = Vec::new();
        match body {
            LambdaBody::Expr(x) => names(x, &mut all),
            LambdaBody::Block(b) => block_names(&b.stmts, &mut all),
        }
        let mut out: Vec<String> = Vec::new();
        for n in all {
            if self.heavy.contains(&n)
                && !self.borrowed.contains(&n)
                && !params.contains(&n)
                && !out.contains(&n)
            {
                out.push(n);
            }
        }
        out
    }

    /// A value kept: moved into a binding, a list, a map, a field, or sent.
    /// A name is given away; a function value moves what it reads.
    fn keep(&mut self, v: &Expr, moved: &mut Moved) {
        if let Some(n) = self.moves_name(v) {
            self.take(&n, v.span, How::Plain, moved);
        } else if let ExprKind::Lambda { params, body } = &v.kind {
            for c in self.captures(params, body) {
                self.take(&c, v.span, How::Captured, moved);
            }
        }
    }

    fn stmt(&mut self, s: &Stmt, moved: &mut Moved) -> bool {
        match s {
            Stmt::Declare {
                name, value, span, ..
            } => {
                self.expr(value, moved);
                let owned_by_name = !name.starts_with("__cg_");
                if name.starts_with("__cg_v") {
                    // `snuff x` with a result type is lowered through a
                    // temporary: returning a borrow.
                    self.returned(value);
                } else if owned_by_name {
                    self.stored(value);
                }
                let typed = self.prog.bindings.get(&span.start).map(is_heavy_ty);
                let heavy = typed.unwrap_or_else(|| self.is_heavy(value));
                if owned_by_name {
                    if matches!(value.kind, ExprKind::Ident(_)) {
                        self.keep(value, moved);
                    }
                    if let ExprKind::Lambda { params, body } = &value.kind {
                        if self.prog.moves.contains(&value.span.start) {
                            self.closures.remove(name);
                        } else {
                            let caps = self.captures(params, body);
                            self.closures.insert(name.clone(), (caps, value.span));
                        }
                    } else {
                        self.closures.remove(name);
                    }
                }
                moved.remove(name);
                if heavy && owned_by_name {
                    self.heavy.insert(name.clone());
                } else {
                    self.heavy.remove(name);
                }
                true
            }
            Stmt::Assign {
                target, value, op, ..
            } => {
                self.expr(value, moved);
                match target {
                    AssignTarget::Var(n) => {
                        if *op == AssignOp::Set {
                            if matches!(value.kind, ExprKind::Ident(_)) {
                                self.keep(value, moved);
                            }
                            if self.is_heavy(value) {
                                self.heavy.insert(n.clone());
                            }
                            self.closures.remove(n);
                        }
                        moved.remove(n);
                    }
                    AssignTarget::Index { object, index } => {
                        self.expr(object, moved);
                        self.expr(index, moved);
                        self.stored(value);
                        self.keep(value, moved);
                    }
                    AssignTarget::Member { object, .. } => {
                        self.expr(object, moved);
                        self.stored(value);
                        self.keep(value, moved);
                    }
                }
                true
            }
            Stmt::Pull(f) => {
                self.pull(f);
                true
            }
            Stmt::Chain(c) => {
                for e in &c.steps {
                    self.expr(e, moved);
                }
                true
            }
            Stmt::Pack(p) => {
                for e in &p.roots {
                    self.expr(e, moved);
                }
                true
            }
            Stmt::Snuff { value, .. } => {
                if let Some(v) = value {
                    self.expr(v, moved);
                    self.returned(v);
                }
                false
            }
            Stmt::Exhale { values, .. } => {
                for v in values {
                    self.expr(v, moved);
                }
                true
            }
            Stmt::Cough { value, .. } => {
                self.expr(value, moved);
                false
            }
            Stmt::If {
                branches,
                otherwise,
                ..
            } => {
                let mut ends: Vec<(Moved, String)> = Vec::new();
                for (i, (c, b)) in branches.iter().enumerate() {
                    self.expr(c, moved);
                    let mut inner = moved.clone();
                    if self.stmts(&b.stmts, &mut inner) {
                        let mut ns = Vec::new();
                        names(c, &mut ns);
                        let label = if ns.iter().any(|n| n.starts_with("__cg_m")) {
                            "in this `match` arm"
                        } else if i == 0 {
                            "inside this `if`"
                        } else {
                            "inside this `else if`"
                        };
                        ends.push((inner, label.to_string()));
                    }
                }
                if let Some(b) = otherwise {
                    let mut inner = moved.clone();
                    if self.stmts(&b.stmts, &mut inner) {
                        ends.push((inner, "inside this `else`".to_string()));
                    }
                }
                let complete = otherwise.is_some();
                if ends.is_empty() && complete {
                    return false;
                }
                join(moved, ends, complete);
                true
            }
            Stmt::While { cond, body, .. } => {
                self.expr(cond, moved);
                self.looping(&body.stmts, moved, None);
                true
            }
            Stmt::For {
                var, iter, body, ..
            } => {
                self.expr(iter, moved);
                self.looping(&body.stmts, moved, Some(var));
                true
            }
            Stmt::Break(sp) => {
                if let Some(l) = self.loops.last_mut() {
                    l.breaks.push((moved.clone(), sp.line));
                }
                false
            }
            Stmt::Continue(_) => {
                if let Some(l) = self.loops.last_mut() {
                    l.continues.push(moved.clone());
                }
                false
            }
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
                true
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
                true
            }
            Stmt::Expr(e) => {
                self.expr(e, moved);
                true
            }
        }
    }

    /// A loop body. Moves that reach the end of a turn, or a `continue`, are
    /// there the next time round; moves before a `break` are there after the
    /// loop, on that path only.
    fn looping(&mut self, body: &[Stmt], moved: &mut Moved, var: Option<&String>) {
        let mut inner = moved.clone();
        if let Some(v) = var {
            inner.remove(v);
        }
        self.loops.push(LoopCtx::default());
        let flows = self.stmts(body, &mut inner);
        let ctx = self.loops.pop().unwrap_or_default();
        let mut end = if flows { inner } else { moved.clone() };
        for c in ctx.continues {
            for (n, g) in c {
                end.entry(n).or_insert(g);
            }
        }
        self.moved_in_loop(body, moved, &end, var);
        let declared = declared_in(body, var);
        for (state, line) in ctx.breaks {
            let mut gone: Vec<(String, Gone)> = state
                .into_iter()
                .filter(|(n, _)| !moved.contains_key(n) && !declared.contains(n))
                .collect();
            gone.sort_by_key(|(_, g)| g.at.start);
            for (n, g) in gone {
                let how = match g.how {
                    How::Plain | How::Captured => {
                        How::Maybe(format!("before the `break` on line {line}"))
                    }
                    other => other,
                };
                moved.entry(n).or_insert(Gone { at: g.at, how });
            }
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
        let declared = declared_in(body, var);
        let mut gone: Vec<(&String, &Gone)> = after
            .iter()
            .filter(|(n, g)| {
                !before.contains_key(*n)
                    && !declared.contains(*n)
                    && !matches!(g.how, How::Borrowed(..))
            })
            .collect();
        gone.sort_by_key(|(_, g)| g.at.start);
        for (n, g) in gone {
            self.err(
                "E1501",
                format!("`{n}` is given away inside a loop, so the next time round it is gone"),
                g.at,
                format!("give away a copy (`{n}.copy()`), or make `{n}` inside the loop"),
            );
        }
    }

    fn pull(&mut self, f: &FnDecl) {
        if f.name.starts_with("_cg_") && !f.name.starts_with("_cg_impl_") {
            return;
        }
        let sig = self.prog.fns.get(&f.span.start);
        let modes = sig.map(|s| s.params.iter().map(|p| p.mode).collect::<Vec<_>>());
        let owned_heavy: Vec<bool> = sig
            .map(|s| {
                s.params
                    .iter()
                    .map(|p| p.mode == Mode::Owned && p.ty.as_ref().is_some_and(is_heavy_ty))
                    .collect()
            })
            .unwrap_or_default();
        let saved = std::mem::take(&mut self.borrowed);
        let saved_heavy = std::mem::take(&mut self.heavy);
        let saved_closures = std::mem::take(&mut self.closures);
        let saved_loops = std::mem::take(&mut self.loops);
        if let Some(m) = modes {
            for (p, mode) in f.params.iter().zip(m) {
                if mode == Mode::Borrow {
                    self.borrowed.insert(p.clone());
                }
            }
        }
        for (p, h) in f.params.iter().zip(owned_heavy) {
            if h {
                self.heavy.insert(p.clone());
            }
        }
        let mut moved = Moved::new();
        self.stmts(&f.body.stmts, &mut moved);
        self.borrowed = saved;
        self.heavy = saved_heavy;
        self.closures = saved_closures;
        self.loops = saved_loops;
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
            self.take(n, e.span, How::Plain, moved);
        }
    }

    /// `n` is used and is not there: say why.
    fn used_after(&mut self, n: &str, g: &Gone, at: Span) {
        let line = g.at.line;
        match &g.how {
            How::Plain => self.err(
                "E1501",
                format!("`{n}` was given away on line {line} and cannot be used after"),
                at,
                format!("use it before it is moved, or move a copy (`{n}.copy()`)"),
            ),
            How::Captured => self.err(
                "E1501",
                format!("`{n}` was moved into a function on line {line} and cannot be used after"),
                at,
                format!("capture a copy (`{n}.copy()`), or drop `move` and let the function borrow it"),
            ),
            How::Maybe(branch) => self.err(
                "E1503",
                format!("`{n}` may have been given away on line {line} ({branch})"),
                at,
                format!(
                    "give away a copy (`{n}.copy()`), or leave the branch right after giving it away"
                ),
            ),
            How::Borrowed(what, f) => self.err(
                "E1503",
                format!(
                    "`{what}` is borrowed by the function on line {}, which is still used on line {}",
                    f.line, at.line
                ),
                g.at,
                format!(
                    "give away a copy (`{what}.copy()`), or finish with `{n}` before giving `{what}` away"
                ),
            ),
        }
    }

    fn expr(&mut self, e: &Expr, moved: &mut Moved) {
        match &e.kind {
            ExprKind::Ident(n) => {
                if let Some(g) = moved.get(n).cloned() {
                    self.used_after(n, &g, e.span);
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
                    self.keep(i, moved);
                }
            }
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    if let MapKey::Dynamic(x) = k {
                        self.expr(x, moved);
                    }
                    self.expr(v, moved);
                    self.stored(v);
                    self.keep(v, moved);
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
                    if matches!(name.as_str(), "push" | "insert" | "set" | "send") {
                        self.keep(a, moved);
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
            ExprKind::Lambda { params, body } => {
                let caps = self.captures(params, body);
                let mut inner = moved.clone();
                self.lambdas += 1;
                match body {
                    LambdaBody::Expr(x) => self.expr(x, &mut inner),
                    LambdaBody::Block(b) => {
                        self.stmts(&b.stmts, &mut inner);
                    }
                }
                self.lambdas -= 1;
                // A function that gives a name away owns it.
                let mut given: Vec<(&String, &Gone)> = inner
                    .iter()
                    .filter(|(n, _)| caps.contains(*n) && !moved.contains_key(*n))
                    .collect();
                given.sort_by_key(|(_, g)| g.at.start);
                let given: Vec<(String, Span)> =
                    given.into_iter().map(|(n, g)| (n.clone(), g.at)).collect();
                for (n, at) in given {
                    self.take(&n, at, How::Captured, moved);
                }
                if self.prog.moves.contains(&e.span.start) {
                    for c in caps {
                        self.take(&c, e.span, How::Captured, moved);
                    }
                }
            }
            ExprKind::Pipe { lhs, rhs } => {
                self.expr(lhs, moved);
                self.expr(rhs, moved);
            }
        }
    }
}
