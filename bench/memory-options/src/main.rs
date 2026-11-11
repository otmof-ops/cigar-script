// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The memory options, measured (docs/decisions/0002-memory.md).
//!
//!     memory-options <trees|nbody|lists> <variant> [rounds]
//!
//! Each program is a port of the cig-compile bench program of the same
//! name, and prints exactly what that program prints (the `.expect` files
//! in bench/fixtures/memory). `rounds` repeats the whole workload and
//! prints once, to make a Rust run long enough to measure; the default, 1,
//! is the program as the CigarScript runtime runs it.
//!
//! Variants: `own` (plain ownership), `rc` (reference counting, with reuse
//! where the program updates a value nobody else holds), `rc-copy`
//! (reference counting without reuse: lists only), `gc` (a tracing
//! collector, gc-arena).

mod lists;
mod nbody;
mod trees;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rounds = args
        .get(2)
        .and_then(|r| r.parse().ok())
        .unwrap_or(1u32)
        .max(1);
    let out = match (
        args.first().map(String::as_str),
        args.get(1).map(String::as_str),
    ) {
        (Some("trees"), Some(v)) => trees::run(v, rounds),
        (Some("nbody"), Some(v)) => nbody::run(v, rounds),
        (Some("lists"), Some(v)) => lists::run(v, rounds),
        _ => None,
    };
    match out {
        Some(text) => print!("{text}"),
        None => {
            eprintln!("usage: memory-options <trees|nbody|lists> <own|rc|rc-copy|gc> [rounds]");
            std::process::exit(3);
        }
    }
}
