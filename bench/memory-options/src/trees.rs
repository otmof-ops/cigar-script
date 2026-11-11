// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Binary trees: allocation and recursion (bench/trees.cig).

use gc_arena::{Arena, Collect, Gc, Rootable};
use std::fmt::Write;
use std::rc::Rc;

const MAXD: u32 = 12;

/// The program's shape, once, over any representation of a tree.
fn drive<T>(make: impl Fn(u32) -> T, check: impl Fn(&T) -> u64) -> String {
    let mut out = String::new();
    writeln!(out, "stretch {}", check(&make(MAXD + 1))).unwrap();
    let long = make(MAXD);
    let mut d = 4;
    while d <= MAXD {
        let n = 1u64 << (MAXD - d + 4);
        let mut c = 0;
        for _ in 0..n {
            c += check(&make(d));
        }
        writeln!(out, "{n} {d} {c}").unwrap();
        d += 2;
    }
    writeln!(out, "long {}", check(&long)).unwrap();
    out
}

// plain ownership: each node owns its children
struct Own(Option<(Box<Own>, Box<Own>)>);
fn own_make(d: u32) -> Own {
    if d == 0 {
        return Own(None);
    }
    Own(Some((Box::new(own_make(d - 1)), Box::new(own_make(d - 1)))))
}
fn own_check(t: &Own) -> u64 {
    match &t.0 {
        None => 1,
        Some((l, r)) => 1 + own_check(l) + own_check(r),
    }
}

// reference counting
struct Counted(Option<(Rc<Counted>, Rc<Counted>)>);
fn rc_make(d: u32) -> Rc<Counted> {
    if d == 0 {
        return Rc::new(Counted(None));
    }
    Rc::new(Counted(Some((rc_make(d - 1), rc_make(d - 1)))))
}
fn rc_check(t: &Rc<Counted>) -> u64 {
    match &t.0 {
        None => 1,
        Some((l, r)) => 1 + rc_check(l) + rc_check(r),
    }
}

// a tracing collector
#[derive(Collect)]
#[collect(no_drop)]
struct Node<'gc> {
    kids: Option<(Gc<'gc, Node<'gc>>, Gc<'gc, Node<'gc>>)>,
}
fn gc_check(n: Gc<'_, Node<'_>>) -> u64 {
    match n.kids {
        None => 1,
        Some((l, r)) => 1 + gc_check(l) + gc_check(r),
    }
}
#[derive(Collect)]
#[collect(no_drop)]
struct Root<'gc> {
    long: Option<Gc<'gc, Node<'gc>>>,
}
fn gc_make<'gc>(mc: &gc_arena::Mutation<'gc>, d: u32) -> Gc<'gc, Node<'gc>> {
    if d == 0 {
        return Gc::new(mc, Node { kids: None });
    }
    let l = gc_make(mc, d - 1);
    let r = gc_make(mc, d - 1);
    Gc::new(mc, Node { kids: Some((l, r)) })
}
fn gc_run() -> String {
    let mut arena = Arena::<Rootable![Root<'_>]>::new(|_| Root { long: None });
    let mut out = String::new();
    let stretch = arena.mutate(|mc, _| gc_check(gc_make(mc, MAXD + 1)));
    arena.collect_debt();
    writeln!(out, "stretch {stretch}").unwrap();
    arena.mutate_root(|mc, root| root.long = Some(gc_make(mc, MAXD)));
    let mut d = 4;
    while d <= MAXD {
        let n = 1u64 << (MAXD - d + 4);
        let mut c = 0;
        for _ in 0..n {
            c += arena.mutate(|mc, _| gc_check(gc_make(mc, d)));
            arena.collect_debt();
        }
        writeln!(out, "{n} {d} {c}").unwrap();
        d += 2;
    }
    let long = arena.mutate(|_, root| gc_check(root.long.unwrap()));
    writeln!(out, "long {long}").unwrap();
    out
}

pub fn run(variant: &str, rounds: u32) -> Option<String> {
    let mut out = String::new();
    for _ in 0..rounds {
        out = match variant {
            "own" => drive(own_make, own_check),
            "rc" | "rc-copy" => drive(rc_make, rc_check),
            "gc" => gc_run(),
            _ => return None,
        };
    }
    Some(out)
}
