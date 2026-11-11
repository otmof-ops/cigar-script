// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! n-body: float arithmetic on a small list of bodies (bench/nbody.cig).
//! The bodies are shared and mutated in place, as the program's list of
//! maps is, so a shared representation holds them behind a cell.

// The constants are the program's, digit for digit, so every variant starts from the same
// numbers (bench/nbody.cig): a rounded one would be a different program.
#![allow(clippy::excessive_precision, clippy::approx_constant)]

use gc_arena::lock::RefLock;
use gc_arena::{Arena, Collect, Gc, Rootable};
use std::cell::RefCell;
use std::rc::Rc;

const PI: f64 = 3.141592653589793;
const SM: f64 = 4.0 * PI * PI;
const DPY: f64 = 365.24;
const STEPS: u32 = 20000;

#[derive(Clone, Copy, Collect)]
#[collect(require_static)]
struct Body {
    x: f64,
    y: f64,
    z: f64,
    vx: f64,
    vy: f64,
    vz: f64,
    m: f64,
}

fn start() -> Vec<Body> {
    let b = |x, y, z, vx: f64, vy: f64, vz: f64, m: f64| Body {
        x,
        y,
        z,
        vx: vx * DPY,
        vy: vy * DPY,
        vz: vz * DPY,
        m: m * SM,
    };
    vec![
        Body {
            x: 0.0,
            y: 0.0,
            z: 0.0,
            vx: 0.0,
            vy: 0.0,
            vz: 0.0,
            m: SM,
        },
        b(
            4.84143144246472090,
            -1.16032004402742839,
            -0.103622044471123109,
            0.00166007664274403694,
            0.00769901118419740425,
            -0.0000690460016972063023,
            0.000954791938424326609,
        ),
        b(
            8.34336671824457987,
            4.12479856412430479,
            -0.403523417114321381,
            -0.00276742510726862411,
            0.00499852801234917238,
            0.0000230417297573763929,
            0.000285885980666130812,
        ),
        b(
            12.8943695621391310,
            -15.1111514016986312,
            -0.223307578892655734,
            0.00296460137564761618,
            0.00237847173959480950,
            -0.0000296589568540237556,
            0.0000436624404335156298,
        ),
        b(
            15.3796971148509165,
            -25.9193146099879641,
            0.179258772950371181,
            0.00268067772490389322,
            0.00162824170038242295,
            -0.0000951592254519715870,
            0.0000515138902046611451,
        ),
    ]
}

/// The program's maths, over a way to read and write body i.
trait Bodies {
    fn get(&self, i: usize) -> Body;
    fn set(&mut self, i: usize, b: Body);
}

fn energy(s: &impl Bodies) -> f64 {
    let mut e = 0.0;
    for i in 0..5 {
        let b = s.get(i);
        e += 0.5 * b.m * (b.vx * b.vx + b.vy * b.vy + b.vz * b.vz);
        for j in (i + 1)..5 {
            let c = s.get(j);
            let (dx, dy, dz) = (b.x - c.x, b.y - c.y, b.z - c.z);
            e -= b.m * c.m / (dx * dx + dy * dy + dz * dz).sqrt();
        }
    }
    e
}

fn advance(s: &mut impl Bodies, dt: f64) {
    for i in 0..5 {
        let mut b = s.get(i);
        for j in (i + 1)..5 {
            let mut c = s.get(j);
            let (dx, dy, dz) = (b.x - c.x, b.y - c.y, b.z - c.z);
            let d2 = dx * dx + dy * dy + dz * dz;
            let mag = dt / (d2 * d2.sqrt());
            b.vx -= dx * c.m * mag;
            b.vy -= dy * c.m * mag;
            b.vz -= dz * c.m * mag;
            c.vx += dx * b.m * mag;
            c.vy += dy * b.m * mag;
            c.vz += dz * b.m * mag;
            s.set(j, c);
        }
        s.set(i, b);
    }
    for i in 0..5 {
        let mut b = s.get(i);
        b.x += dt * b.vx;
        b.y += dt * b.vy;
        b.z += dt * b.vz;
        s.set(i, b);
    }
}

fn offset(s: &mut impl Bodies) {
    let (mut px, mut py, mut pz) = (0.0, 0.0, 0.0);
    for i in 0..5 {
        let b = s.get(i);
        px += b.vx * b.m;
        py += b.vy * b.m;
        pz += b.vz * b.m;
    }
    let mut sun = s.get(0);
    sun.vx = -px / SM;
    sun.vy = -py / SM;
    sun.vz = -pz / SM;
    s.set(0, sun);
}

fn report(before: f64, after: f64) -> String {
    format!("{before:.9}\n{after:.9}\n")
}

// plain ownership: the bodies are a value
struct Own(Vec<Body>);
impl Bodies for Own {
    fn get(&self, i: usize) -> Body {
        self.0[i]
    }
    fn set(&mut self, i: usize, b: Body) {
        self.0[i] = b;
    }
}

// reference counting: each body shared, and mutated through a cell
struct Counted(Vec<Rc<RefCell<Body>>>);
impl Bodies for Counted {
    fn get(&self, i: usize) -> Body {
        *self.0[i].borrow()
    }
    fn set(&mut self, i: usize, b: Body) {
        *self.0[i].borrow_mut() = b;
    }
}

// a tracing collector: each body a collected cell, written through a barrier
#[derive(Collect)]
#[collect(no_drop)]
struct Root<'gc> {
    bodies: Vec<Gc<'gc, RefLock<Body>>>,
}
struct Traced<'a, 'gc> {
    mc: &'a gc_arena::Mutation<'gc>,
    root: &'a Root<'gc>,
}
impl Bodies for Traced<'_, '_> {
    fn get(&self, i: usize) -> Body {
        *self.root.bodies[i].borrow()
    }
    fn set(&mut self, i: usize, b: Body) {
        *self.root.bodies[i].borrow_mut(self.mc) = b;
    }
}

fn drive(s: &mut impl Bodies) -> String {
    offset(s);
    let before = energy(s);
    for _ in 0..STEPS {
        advance(s, 0.01);
    }
    report(before, energy(s))
}

pub fn run(variant: &str, rounds: u32) -> Option<String> {
    let mut out = String::new();
    for _ in 0..rounds {
        out = match variant {
            "own" => drive(&mut Own(start())),
            "rc" | "rc-copy" => drive(&mut Counted(
                start()
                    .into_iter()
                    .map(|b| Rc::new(RefCell::new(b)))
                    .collect(),
            )),
            "gc" => {
                let mut arena = Arena::<Rootable![Root<'_>]>::new(|mc| Root {
                    bodies: start()
                        .into_iter()
                        .map(|b| Gc::new(mc, RefLock::new(b)))
                        .collect(),
                });
                // One mutation, then a collection: the program allocates nothing
                // after set-up, so the collector has nothing to do and the cost
                // measured is the write barrier.
                let text = arena.mutate(|mc, root| drive(&mut Traced { mc, root }));
                arena.collect_debt();
                text
            }
            _ => return None,
        };
    }
    Some(out)
}
