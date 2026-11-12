// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Held to Rust's standard (#22, S1-S8): programs the checker must refuse,
//! each with the error it names first, and the programs beside them that
//! must still pass. `tests/strict/sN-NAME.cigar` + `.err` (code on line 1,
//! a substring of the message on line 2) is refused; `ok-sN-NAME.cigar`
//! checks clean.

use std::path::{Path, PathBuf};

use cigarscript::check::{check_file_source, check_file_source_deny, Deny, Kind};

fn cases(prefix: &str) -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/strict");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "cigar"))
        .filter(|p| {
            let n = p.file_name().unwrap().to_string_lossy();
            n.starts_with(prefix) || n.starts_with(&format!("ok-{prefix}"))
        })
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no cases for {prefix}");
    out
}

/// Every case with the prefix: a refused one fails first with its error,
/// an `ok-` one checks clean.
fn run(prefix: &str) {
    let mut wrong = Vec::new();
    for case in cases(prefix) {
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let src = std::fs::read_to_string(&case).unwrap();
        let checked = check_file_source(Kind::Cigar, &src, None, Some(&case));
        if name.starts_with("ok-") {
            if let Some(d) = checked.errors.first() {
                wrong.push(format!(
                    "{name}: should check, and gives {} {}",
                    d.code, d.message
                ));
            }
            // What a strict file says nothing about (L1-L6) says nothing.
            if prefix.starts_with('l') {
                if let Some(w) = checked.warnings.first() {
                    wrong.push(format!(
                        "{name}: should be silent, and warns {} {}",
                        w.code, w.message
                    ));
                }
            }
            continue;
        }
        // `NAME.warn`: it checks, and warns (code on line 1, a substring of
        // the message on line 2).
        if let Ok(w) = std::fs::read_to_string(case.with_extension("warn")) {
            let mut lines = w.lines();
            let (code, part) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
            if let Some(d) = checked.errors.first() {
                wrong.push(format!(
                    "{name}: should check, and gives {} {}",
                    d.code, d.message
                ));
            } else if !checked.warnings.iter().any(|d| {
                d.code == code
                    && (d.message.contains(part) || d.hint.as_deref().unwrap_or("").contains(part))
            }) {
                let got: Vec<String> = checked
                    .warnings
                    .iter()
                    .map(|d| format!("{} {}", d.code, d.message))
                    .collect();
                wrong.push(format!(
                    "{name}: want warning {code} containing `{part}`, got {got:?}"
                ));
            }
            continue;
        }
        let err = std::fs::read_to_string(case.with_extension("err")).unwrap();
        let mut lines = err.lines();
        let (code, part) = (lines.next().unwrap_or(""), lines.next().unwrap_or(""));
        match checked.errors.first() {
            Some(d)
                if d.code == code
                    && (d.message.contains(part)
                        || d.hint.as_deref().unwrap_or("").contains(part)) => {}
            Some(d) => wrong.push(format!(
                "{name}: want {code} containing `{part}`, got {} {} / {:?}",
                d.code, d.message, d.hint
            )),
            None => wrong.push(format!("{name}: want {code}, and it checks")),
        }
    }
    assert!(wrong.is_empty(), "{}", wrong.join("\n"));
}

#[test]
fn optional_must_be_narrowed() {
    run("s3-");
}

#[test]
fn every_path_returns() {
    run("s4-");
}

#[test]
fn mutable_containers_are_invariant() {
    run("s1-");
}

#[test]
fn function_parameters_are_contravariant() {
    run("s2-");
}

#[test]
fn parallel_body_cannot_write_shared() {
    run("s8-");
}

#[test]
fn reserved_words_are_names_after_dot_and_as_keys() {
    run("d1-");
}

#[test]
fn stick_is_deeply_immutable() {
    run("d2-");
}

/// D3 (ii): a medium file's value is lent to a full function, so the
/// caller keeps it; an `owned` parameter takes it only with `move` (gone
/// after, as at full) or `.copy()`; `move` is written nowhere else at
/// medium. The full functions are in `tests/strict/boundary/plan.cigar`.
#[test]
fn a_medium_caller_lends_to_a_full_function() {
    run("d3-");
}

/// D3 (ii): an `owned` parameter takes only what the caller can give: a
/// field or an element of a value the caller holds (`h.items`, `xss[0]`),
/// or a name put inside a list, map or record written at the call, is
/// still the caller's (`E1502`, at medium and at full); `.copy()` and
/// values made at the call are fine.
#[test]
fn an_owned_parameter_takes_only_what_can_be_given() {
    run("d3-place-");
}

/// D3 (ii): a function type carries no parameter modes, so a function
/// with an `owned` parameter is called by name only: as a function value
/// (bound to a name, passed as an argument, at medium and at full) it is
/// `E1502`. A `pack` that calls it by name is the way through.
#[test]
fn an_owned_function_is_not_a_function_value() {
    run("d3-fn-value-");
}

/// D3 (ii): the refusal of a bare name names both ways out.
#[test]
fn an_owned_parameter_from_medium_needs_move_or_copy() {
    let case =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/strict/d3-owned-without-move.cigar");
    let src = std::fs::read_to_string(&case).unwrap();
    let checked = check_file_source(Kind::Cigar, &src, None, Some(&case));
    let d = checked.errors.first().expect("refused");
    assert_eq!(d.code, "E1502");
    assert_eq!(d.line, Some(5));
    let hint = d.hint.as_deref().unwrap_or("");
    assert!(
        hint.contains("`move xs`") && hint.contains("`xs.copy()`"),
        "{hint}"
    );
}

/// D2 (#22, rule (a)): a `.cigar` file at mild is CigScript, and
/// writing through a `stick` stays accepted there. Read as medium it is
/// an error, but a mild file's checker never hits that rule. (Records
/// are a medium feature, so a map is used here.)
#[test]
fn a_mild_file_may_mutate_through_a_stick() {
    let src = "stick m = {k: 1}\nm.k = 2\nstick xs = [1]\nxs.push(2)\nexhale m.k, xs.len()\n";
    let c = check_file_source(Kind::Cigar, src, None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// A `.cig` file is never held to D2. A write through a stick-bound map
/// earns a hint when the file is read as medium (the superset rule); no
/// error, no warning on the mild side.
#[test]
fn a_cig_file_with_a_stick_mutation_is_not_refused() {
    let src = "stick m = {k: 1}\nm.k = 2\nexhale m.k\n";
    let c = check_file_source(Kind::Cig, src, None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    assert!(c.hints.iter().any(|h| h.code == "E1215"), "{:?}", c.hints);
}

fn warning_codes(src: &str) -> Vec<(String, String)> {
    let c = check_file_source(Kind::Cigar, src, None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    c.warnings
        .iter()
        .map(|w| (w.code.to_string(), w.message.clone()))
        .collect()
}

/// A name that shadows a contextual keyword is W1234 until the file's
/// strength reserves the word: at medium, `match` is an error and `layer`
/// (a full-strength word) is a warning naming the strength it turns on.
#[test]
fn a_name_that_shadows_a_keyword_warns_w1234() {
    let mild = warning_codes("stick match = 1\nexhale match\n");
    assert_eq!(mild.len(), 2, "{mild:?}");
    assert!(mild.iter().all(|(c, m)| c == "W1234"
        && m.contains("`match` will be a keyword if this file goes to medium")));

    let medium = warning_codes("strength medium\nstick layer = 1\nexhale layer\n");
    assert_eq!(medium.len(), 2, "{medium:?}");
    assert!(medium[0].1.contains("goes to full"), "{medium:?}");

    // Fields and keys are not names that shadow anything.
    assert!(warning_codes("stick m = {match: 1}\nexhale m.match\n").is_empty());
    // A keyword where its syntax begins is the feature, not a shadow.
    let c = check_file_source(Kind::Cigar, "record R { x: 1 }\n", None, None);
    assert_eq!(c.errors[0].code, "E1101");
}

/// L9: the message names the word and the escape, and the escape works.
#[test]
fn the_reserved_word_message_offers_r_hash() {
    let c = check_file_source(Kind::Cigar, "strength medium\nstick enum = 1\n", None, None);
    let d = &c.errors[0];
    assert_eq!(d.code, "E1105");
    assert_eq!(
        d.message,
        "`enum` is a reserved word in `.cigar` files; rename it, or write `r#enum`"
    );
    let c = check_file_source(
        Kind::Cigar,
        "strength medium\nstick r#enum = 1\nexhale r#enum\n",
        None,
        None,
    );
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// A `.cig` file never gets a new error: a word that would be reserved at
/// medium is a hint at most.
#[test]
fn a_cig_file_with_a_keyword_name_is_not_refused() {
    let c = check_file_source(Kind::Cig, "stick match = 1\nexhale match\n", None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// cig-orchestrator's `orchestrate.cig`, read as a mild `.cigar` file, gave 63
/// refusals for words that were fields (45 `layer`, 18 `use`). It has none
/// now. The file is read from `CIG_ORCHESTRATOR_SRC` (the checkout; default
/// `../cig-orchestrator`), and the test says so and passes when it is not there.
#[test]
fn orchestrate_cig_as_cigar_has_no_reserved_word_errors() {
    let root = std::env::var("CIG_ORCHESTRATOR_SRC")
        .map(PathBuf::from)
        .unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("../cig-orchestrator"));
    let file = root.join("orchestrate.cig");
    let Ok(src) = std::fs::read_to_string(&file) else {
        eprintln!("note: skipping; {} is not there", file.display());
        return;
    };
    let c = check_file_source(
        Kind::Cigar,
        &src,
        Some(cigarscript::features::Strength::Mild),
        None,
    );
    let reserved: Vec<_> = c
        .errors
        .iter()
        .filter(|d| matches!(d.code, "E1101" | "E1102" | "E1103" | "E1105"))
        .map(|d| format!("{} {} at {:?}", d.code, d.message, d.line))
        .collect();
    assert!(reserved.is_empty(), "{}", reserved.join("\n"));
}

#[test]
fn binding_moves_at_full() {
    run("s5-");
}

#[test]
fn closure_capture_borrows_or_moves() {
    run("s6-");
}

#[test]
fn possibly_moved_is_moved() {
    run("s7-");
}

#[test]
fn an_error_inside_an_interpolation_is_said_at_its_string() {
    // cig parses `${...}` on its own; the error must not land on line 1.
    let src = "strength full\nstick a = [1]\nstick b = a\nexhale \"x ${a.len()}\"\n";
    let checked = check_file_source(Kind::Cigar, src, None, None);
    let d = checked.errors.first().expect("an E1501");
    assert_eq!(d.code, "E1501", "{}", d.message);
    assert_eq!((d.line, d.col), (Some(4), Some(8)), "{}", d.message);
}

#[test]
fn any_parameters_warn_and_full_refuses_them() {
    run("l1-");
}

#[test]
fn unreachable_code_warns() {
    run("l2-");
}

#[test]
fn unused_names_warn() {
    run("l3-");
}

#[test]
fn dropped_results_warn() {
    run("l4-");
}

#[test]
fn constant_overflow_is_found_at_check_time() {
    run("l5-");
}

#[test]
fn constant_division_by_zero_is_found_at_check_time() {
    run("l6-");
}

#[test]
fn literals_out_of_range_in_containers_are_found_at_check_time() {
    run("l7-");
}

fn deny(
    src: &str,
    kind: Kind,
    strength: Option<cigarscript::features::Strength>,
    d: Deny,
) -> cigarscript::check::Checked {
    check_file_source_deny(kind, src, strength, None, d)
}

/// `--deny any` makes the untyped parameter an error at medium; `--deny
/// warnings` makes every other warning one.
#[test]
fn deny_makes_warnings_errors() {
    let any = "strength medium\npull f(x) {\n  snuff x + 1\n}\nexhale f(1)\n";
    let none = Deny::default();
    let c = deny(any, Kind::Cigar, None, none);
    assert!(c.errors.is_empty() && c.warnings[0].code == "W1230");
    let c = deny(
        any,
        Kind::Cigar,
        None,
        Deny {
            any: true,
            warnings: false,
        },
    );
    assert_eq!(c.errors[0].code, "W1230", "{:?}", c.errors);
    assert!(c.warnings.is_empty());

    let dead = "strength medium\npull f() -> Int {\n  snuff 1\n  exhale 2\n}\nexhale f()\n";
    let c = deny(
        dead,
        Kind::Cigar,
        None,
        Deny {
            any: true,
            warnings: false,
        },
    );
    assert!(
        c.errors.is_empty(),
        "`any` is not `warnings`: {:?}",
        c.errors
    );
    let c = deny(
        dead,
        Kind::Cigar,
        None,
        Deny {
            any: false,
            warnings: true,
        },
    );
    assert_eq!(c.errors[0].code, "W1231");
    assert!(c.errors[0]
        .hint
        .as_deref()
        .unwrap()
        .contains("--deny warnings"));
}

/// The superset rule (#1): a `.cig` file never gets one, denied or not.
#[test]
fn a_cig_file_is_never_held_to_a_warning() {
    let src = "pull f(x) { snuff x + 1 }\npull g() { snuff 1\n exhale 2 }\nstick unused = 5\nexhale f(1), g()\n";
    let all = Deny {
        any: true,
        warnings: true,
    };
    let c = deny(src, Kind::Cig, None, all);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
    assert!(
        c.warnings.iter().all(|w| !w.code.starts_with("W123")),
        "{:?}",
        c.warnings
    );
    assert!(c.hints.iter().all(|h| !h.code.starts_with("W123")));
}

/// Folding stops where a value is not known: a divisor read at run time,
/// and a float quotient.
#[test]
fn only_what_the_checker_can_see_is_folded() {
    let src = "strength full\npull div(a: Int, b: Int) -> Int { snuff a / b }\nroll z = 0\nexhale 5 / 2, 1.5 / 3.0, div(1, z)\n";
    let c = check_file_source(Kind::Cigar, src, None, None);
    assert!(c.errors.is_empty(), "{:?}", c.errors);
}

/// A parameter written on its own line is said at that line.
#[test]
fn a_parameter_is_said_where_it_is_written() {
    let src =
        "strength medium\npull f(\n  a: Int,\n  b\n) -> Int {\n  snuff a\n}\nexhale f(1, 2)\n";
    let c = check_file_source(Kind::Cigar, src, None, None);
    let lines: Vec<(String, Option<u32>)> = c
        .warnings
        .iter()
        .map(|w| (w.code.to_string(), w.line))
        .collect();
    assert!(lines.contains(&("W1230".into(), Some(4))), "{lines:?}");
    assert!(lines.contains(&("W1232".into(), Some(4))), "{lines:?}");
}

/// #38, section 1.1 (constraints): `max<T: Ord>` checks and runs with a
/// hand-written `impl Ord` (the stand-in for `derive(Ord)`, not built
/// yet); the bound's method is visible on a `T` in the body; a record with
/// no `impl Ord` is `E1230` at the call; and the lowering strips the bounds
/// and the `where` clause, so CigScript never sees them.
#[test]
fn bounded_generic_uses_the_bound() {
    let decls = "strength medium\n\
        interface Ord {\n  pull rank(self) -> Int\n}\n\
        record Point { x: Int, y: Int }\n\
        record Blob { n: Int }\n\
        impl Ord for Point {\n  pull rank(self) -> Int {\n    snuff self.x + self.y\n  }\n}\n\
        pull max<T: Ord>(xs: List<T>) -> T? {\n  roll best: T? = null\n  for x in xs {\n    if best == null or x.rank() > best.rank() {\n      best = x\n    }\n  }\n  snuff best\n}\n\
        pull biggest<T>(xs: List<T>) -> Int where T: Ord {\n  stick m = max(xs)\n  snuff m?.rank() ?? 0\n}\n";
    let good = format!(
        "{decls}exhale max([Point {{ x: 1, y: 2 }}, Point {{ x: 3, y: 4 }}])\nexhale biggest([Point {{ x: 9, y: 1 }}])\n"
    );
    let checked = cigarscript::check::check_source(Kind::Cigar, &good, None);
    assert!(
        checked.errors.is_empty(),
        "should check: {:?}",
        checked
            .errors
            .iter()
            .map(|d| format!("{} {}", d.code, d.message))
            .collect::<Vec<_>>()
    );
    let text = &checked.source_as_cigscript_saw_it;
    let sig = |name: &str| {
        text.lines()
            .find(|l| l.trim_start().starts_with(&format!("pull {name}")))
            .unwrap_or_else(|| panic!("no `pull {name}` in:\n{text}"))
            .to_string()
    };
    let (max, biggest) = (sig("max"), sig("biggest"));
    assert!(!max.contains("Ord"), "the bound reached CigScript: {max}");
    assert!(
        !biggest.contains("where") && !biggest.contains("Ord"),
        "the `where` clause reached CigScript: {biggest}"
    );

    // It runs: the bound's method dispatches by the value.
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("bounded.cigar");
    std::fs::write(&file, &good).unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cigar"))
        .arg("--plain")
        .arg("run")
        .arg("bounded.cigar")
        .current_dir(dir.path())
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("CIGSCRIPT_HOME", dir.path().join("cig"))
        .env("NO_COLOR", "1")
        .output()
        .expect("cigar runs");
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Point { x: 3, y: 4 }\n10\n",
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // A record without the `impl` the bound asks for.
    let bad = format!("{decls}exhale max([Blob {{ n: 1 }}])\n");
    let checked = cigarscript::check::check_source(Kind::Cigar, &bad, None);
    let d = checked.errors.first().expect("refused");
    assert_eq!(d.code, "E1230");
    assert_eq!(
        d.message,
        "`Blob` doesn't implement `Ord`, which `max` needs for `T`"
    );
    let hint = d.hint.as_deref().unwrap_or("");
    assert!(
        hint.contains("`T: Ord`") && hint.contains("impl Ord for Blob"),
        "{hint}"
    );
}

/// A corpus file's path.
fn corpus(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/corpus")
        .join(rel)
}

/// `cigar run` on a file from its folder, with a scratch state: its stdout.
fn run_file(file: &Path) -> String {
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cigar"))
        .arg("--plain")
        .arg("run")
        .arg(file.file_name().unwrap())
        .current_dir(file.parent().unwrap())
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("CIGSCRIPT_HOME", dir.path().join("cig"))
        .env("NO_COLOR", "1")
        .output()
        .expect("cigar runs");
    assert!(
        out.status.success(),
        "{} fails: {}",
        file.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).to_string()
}

/// The first error a file gives: (code, message).
fn first_error(file: &Path) -> Option<(String, String)> {
    let src = std::fs::read_to_string(file).unwrap();
    let checked = check_file_source(Kind::Cigar, &src, None, Some(file));
    checked
        .errors
        .first()
        .map(|d| (d.code.to_string(), d.message.clone()))
}

/// #38, section 1.2: `impl<T: Show> Show for List<T>` prints a
/// `List<Point>`, and a `List<List<Point>>` through itself; a list of a
/// record with no `impl Show` is `E1230`; and two `impl`s for lists of
/// different element types are picked by the elements when the value's
/// type is only known at run time.
#[test]
fn blanket_impl_selects() {
    let f = corpus("run/generic-impl-blanket.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        "[(1, 2); (3, 4)]\n[[(1, 2); (3, 4)]; [(0, 0)]]\n"
    );
    let (code, msg) = first_error(&corpus("check/e1230-blanket-bound.cigar")).expect("refused");
    assert_eq!(code, "E1230");
    assert!(
        msg.contains("which `impl Show for List<T>` needs for `T`"),
        "{msg}"
    );
    let f = corpus("run/dispatch-by-element-type.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(run_file(&f), "texts\nints\ntexts\n");
}

/// #38, section 1.2: `E1231` both ways. Two `impl`s that can cover the same
/// type overlap (no specialization); an `impl` of a module's interface for
/// a type this file does not declare is an orphan.
#[test]
fn orphan_and_overlap_refused() {
    for (rel, part) in [
        (
            "check/e1231-overlap.cigar",
            "overlaps `impl Show for List<Int>`",
        ),
        (
            "check/e1231-same-type-twice.cigar",
            "overlaps `impl Show for Int`",
        ),
        (
            "check/e1231-orphan-built-in/main.cigar",
            "belongs in the module that declares `Show`",
        ),
        (
            "check/e1231-orphan-module-record/main.cigar",
            "neither `Show` nor `Dot`",
        ),
    ] {
        let (code, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(code, "E1231", "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
    // An `impl` of a module's interface for a record this file declares is
    // not an orphan.
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join("tools")).unwrap();
    std::fs::copy(
        corpus("check/e1231-orphan-built-in/tools/shapes.cigar"),
        dir.path().join("tools/shapes.cigar"),
    )
    .unwrap();
    let main = dir.path().join("main.cigar");
    std::fs::write(
        &main,
        "strength medium\nuse tools.shapes\nrecord Mine { n: Int }\nimpl Show for Mine {\n  pull show(self) -> Text {\n    snuff \"mine ${self.n}\"\n  }\n}\npull say<T: Show>(x: T) -> Text {\n  snuff x.show()\n}\nexhale say(Mine { n: 3 })\n",
    )
    .unwrap();
    assert_eq!(first_error(&main), None);
    assert_eq!(run_file(&main), "mine 3\n");
}

/// #38, section 1.3: a method with a body in an interface is its default;
/// an `impl` that leaves it out gets it (and `self.show()` in it is the
/// implementing type's), one that writes it overrides it, through a known
/// type and through a bounded `T` alike.
#[test]
fn default_method_overridable() {
    let f = corpus("run/default-method.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(run_file(&f), "p1, p1\ntwo of #7\np2, p2\ntwo of #8\n");
}

/// #38, section 2: operators on records are their standard interfaces'
/// methods (`+` is `Add`, `-x` is `Neg`, `==` is `Eq`, `<` is `Ord`, `[k]`
/// is `Index`), on a bounded `T` through the bound, while the built-in
/// types' operators stay CigScript's; a record with no `impl` is `E1230` at
/// the operator; and a file's `impl` of a standard interface for its own
/// record is no orphan, while one for a built-in type overlaps the
/// prelude's (`E1231`).
#[test]
fn operators_on_records() {
    let f = corpus("run/operators-on-records.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/operators-on-records.out")).unwrap()
    );
    let f = corpus("run/standard-interfaces-built-in.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/standard-interfaces-built-in.out")).unwrap()
    );
    for (rel, code, part) in [
        (
            "check/e1230-operator-without-impl.cigar",
            "E1230",
            "`Vec2` doesn't implement `Add`, which `+` needs",
        ),
        (
            "check/e1230-comparison-without-ord.cigar",
            "E1230",
            "which `<` needs",
        ),
        (
            "check/e1230-operator-on-bounded-t.cigar",
            "E1230",
            "`T` doesn't implement `Add`",
        ),
        (
            "check/e1231-standard-impl-for-built-in.cigar",
            "E1231",
            "in the prelude",
        ),
    ] {
        let (c, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(c, code, "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
}

/// #38, section 2: `derive(Eq, Ord, Hash, Show, Debug, Default, Clone)` on
/// records and enums writes each `impl` field by field (`Ord` lexicographic,
/// an enum by variant order), field defaults fill a record literal and
/// `Default`, a bounded `T` uses what was derived, and a field that does not
/// implement the interface is `E1232`, named.
#[test]
fn derive_writes_the_standard_impls() {
    let f = corpus("run/derive-standard.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/derive-standard.out")).unwrap()
    );
    for (rel, code, part) in [
        (
            "check/e1232-field-without-impl.cigar",
            "E1232",
            "field `tags`",
        ),
        ("check/e1232-nested-record.cigar", "E1232", "field `inner`"),
        (
            "check/e1232-enum-default.cigar",
            "E1232",
            "no default variant",
        ),
        (
            "check/e1201-field-default.cigar",
            "E1201",
            "the default of `retries`",
        ),
    ] {
        let (c, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(c, code, "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
}

/// #22 D3 (i): a record that carries `derive(Copy)`, whose fields are all
/// copy types, is a copy type: binding it, passing it, storing it and giving
/// it to an `owned` parameter copy it, at full and at medium, and a write
/// through one copy is never seen through another. A record with a non-copy
/// field cannot derive it (`E1232`).
#[test]
fn derive_copy_makes_a_copy_type() {
    let f = corpus("run/derive-copy.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/derive-copy.out")).unwrap()
    );
    let (c, msg) = first_error(&corpus("check/e1232-copy-with-list-field.cigar")).expect("refused");
    assert_eq!(c, "E1232");
    assert!(msg.contains("field `items`"), "{msg}");
    for case in [
        "ok-d3-copy-record.cigar",
        "ok-s5-copy-record.cigar",
        "ok-s6-copy-record.cigar",
    ] {
        let f = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/strict")
            .join(case);
        assert_eq!(first_error(&f), None, "{case}");
    }
}

/// `cigar explain --derive file:Type` prints what `derive` wrote.
#[test]
fn explain_derive_prints_the_impls() {
    let f = corpus("run/derive-standard.cigar");
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_cigar"))
        .arg("--plain")
        .arg("explain")
        .arg("--derive")
        .arg(format!("{}:Point", f.display()))
        .env("XDG_CACHE_HOME", dir.path().join("cache"))
        .env("CIGSCRIPT_HOME", dir.path().join("cig"))
        .env("NO_COLOR", "1")
        .output()
        .expect("cigar runs");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8_lossy(&out.stdout);
    for want in [
        "impl Eq for Point",
        "impl Ord for Point",
        "impl Hash for Point",
        "impl Default for Point",
        "impl Clone for Point",
        "_cg_impl_Ord_Point_cmp",
    ] {
        assert!(text.contains(want), "{want} missing from:\n{text}");
    }
}

/// #38, section 2: `Map<Point, Text>` with `derive(Hash, Eq)` on the record,
/// and `Map<Int, Text>`: entries set, replaced, read, removed and iterated,
/// keys read back as the keys themselves, a map changed while it is
/// iterated behaving as a cig map does, and `Map<Text, V>` unchanged. A key
/// type without `Hash` or `Eq` is `E1230`; a `Map<K, V>` written out with
/// entries, and a key of the wrong type, are `E1201`.
#[test]
fn records_as_map_keys() {
    let f = corpus("run/records-as-map-keys.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/records-as-map-keys.out")).unwrap()
    );
    for (rel, code, part) in [
        (
            "check/e1230-map-key-without-hash.cigar",
            "E1230",
            "`Blob` doesn't implement `Hash`, which a `Map` key needs",
        ),
        (
            "check/e1201-keyed-map-written-out.cigar",
            "E1201",
            "starts empty",
        ),
        ("check/e1201-map-key-type.cigar", "E1201", "expected `Int`"),
    ] {
        let (c, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(c, code, "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
}

/// #38, section 2: `Set<T>` holds each value once (records by their `Hash`
/// and `Eq`), with `add`, `remove`, `has`, `len`, `items`, `union`,
/// `intersection` and `difference`; a loop over a set changed inside it reads
/// the elements as they were; an element type without `Eq` is `E1230`; and a
/// `stick` set cannot be added to (`E1215`).
#[test]
fn sets_hold_each_value_once() {
    let f = corpus("run/set-operations.cigar");
    assert_eq!(first_error(&f), None);
    assert_eq!(
        run_file(&f),
        std::fs::read_to_string(corpus("run/set-operations.out")).unwrap()
    );
    for (rel, code, part) in [
        (
            "check/e1230-set-element-without-eq.cigar",
            "E1230",
            "which a `Set` element needs",
        ),
        ("check/e1215-set-through-stick.cigar", "E1215", "`.add`"),
    ] {
        let (c, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(c, code, "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
}

/// #38, section 2: `derive(Json)`. `to_json` and `from_json` round-trip a
/// record with nested records, lists, maps, optionals and enums; a field
/// default fills a missing key; `from_json` gives `Err` naming the path of
/// the first mismatch (`value.tags[0]`), an unknown key (refused unless
/// `derive(Json(lenient))`), a missing key, an unknown variant and text that
/// is not JSON. A field with no JSON form is `E1232`.
#[test]
fn derive_json_round_trips() {
    let f = corpus("run/derive-json.cigar");
    assert_eq!(first_error(&f), None);
    let out = run_file(&f);
    assert_eq!(
        out,
        std::fs::read_to_string(corpus("run/derive-json.out")).unwrap()
    );
    for want in [
        "\ntrue\n",
        "retries: 3",
        "E1210 value.retries should be Int and is Text",
        "E1206 value.extra",
        "E1205 value.name",
        "E1210 value.tags[0] should be Text",
        "E1207 value.shape",
        "Loose { a: 1 }",
    ] {
        assert!(out.contains(want), "{want} missing from:\n{out}");
    }
    for (rel, code, part) in [
        (
            "check/e1232-json-field-without-json.cigar",
            "E1232",
            "no JSON form",
        ),
        ("check/e1232-json-set-field.cigar", "E1232", "`Set<Int>`"),
        ("check/e1202-json-option.cigar", "E1202", "`lenient` only"),
        (
            "check/e1201-from-json-not-text.cigar",
            "E1201",
            "expected `Text`",
        ),
    ] {
        let (c, msg) = first_error(&corpus(rel)).unwrap_or_else(|| panic!("{rel} checks"));
        assert_eq!(c, code, "{rel}: {msg}");
        assert!(msg.contains(part), "{rel}: {msg}");
    }
}
