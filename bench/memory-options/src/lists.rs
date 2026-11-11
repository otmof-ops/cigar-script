// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Lists: sort, filter and map (bench/lists.cig). Every step makes a new
//! list in CigarScript; a representation that can see nobody else holds
//! the old one may reuse its storage.

use gc_arena::{Arena, Collect, Gc, Rootable};
use std::fmt::Write;
use std::rc::Rc;

const N: i64 = 60000;

fn input() -> Vec<i64> {
    (0..N).map(|i| (i * 7919) % 10007).collect()
}

fn text(len: usize, first: i64, last: i64) -> String {
    let mut s = String::new();
    writeln!(s, "{len} {first} {last}").unwrap();
    s
}

// plain ownership: each step consumes its list and hands on the same storage
fn own() -> String {
    let mut xs = input();
    xs.sort();
    xs.retain(|x| x % 2 == 0);
    for x in xs.iter_mut() {
        *x *= 2;
    }
    text(xs.len(), xs[0], xs[xs.len() - 1])
}

// reference counting with reuse: a list held once is updated in place
fn rc() -> String {
    let mut xs = Rc::new(input());
    Rc::make_mut(&mut xs).sort();
    Rc::make_mut(&mut xs).retain(|x| x % 2 == 0);
    for x in Rc::make_mut(&mut xs).iter_mut() {
        *x *= 2;
    }
    text(xs.len(), xs[0], xs[xs.len() - 1])
}

// reference counting without reuse: every step builds a new list
fn rc_copy() -> String {
    let xs = Rc::new(input());
    let mut sorted = (*xs).clone();
    sorted.sort();
    let sorted = Rc::new(sorted);
    let evens: Rc<Vec<i64>> = Rc::new(sorted.iter().copied().filter(|x| x % 2 == 0).collect());
    let doubled: Rc<Vec<i64>> = Rc::new(evens.iter().map(|x| x * 2).collect());
    text(doubled.len(), doubled[0], doubled[doubled.len() - 1])
}

// a tracing collector: every step builds a new list, and the old ones are
// garbage. gc-arena counts the size of what Gc::new takes, not a Vec's
// buffer, so it would never see this garbage mount up: the run collects
// after each step, as a collector that counted bytes would have by then.
#[derive(Collect)]
#[collect(no_drop)]
struct Root<'gc> {
    list: Option<Gc<'gc, Vec<i64>>>,
}
fn gc() -> String {
    let mut arena = Arena::<Rootable![Root<'_>]>::new(|_| Root { list: None });
    arena.mutate_root(|mc, r| r.list = Some(Gc::new(mc, input())));
    arena.collect_all();
    arena.mutate_root(|mc, r| {
        let mut s = r.list.unwrap().to_vec();
        s.sort();
        r.list = Some(Gc::new(mc, s));
    });
    arena.collect_all();
    arena.mutate_root(|mc, r| {
        let e: Vec<i64> = r
            .list
            .unwrap()
            .iter()
            .copied()
            .filter(|x| x % 2 == 0)
            .collect();
        r.list = Some(Gc::new(mc, e));
    });
    arena.collect_all();
    arena.mutate_root(|mc, r| {
        let d: Vec<i64> = r.list.unwrap().iter().map(|x| x * 2).collect();
        r.list = Some(Gc::new(mc, d));
    });
    arena.collect_all();
    arena.mutate(|_, r| {
        let l = r.list.unwrap();
        text(l.len(), l[0], l[l.len() - 1])
    })
}

pub fn run(variant: &str, rounds: u32) -> Option<String> {
    let f: fn() -> String = match variant {
        "own" => own,
        "rc" => rc,
        "rc-copy" => rc_copy,
        "gc" => gc,
        _ => return None,
    };
    let mut out = String::new();
    for _ in 0..rounds {
        out = f();
    }
    Some(out)
}
