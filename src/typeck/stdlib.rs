// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The standard library's type signatures (cig 1.20.0).
//!
//! Source of truth: every module's `TABLE` in cig's `src/stdlib/*.rs`, the
//! global table in `core.rs`, and the method tables in `methods.rs`. A
//! signature never rejects a program cig accepts: where a result depends
//! on runtime data, or cig takes more than one shape, the type is `Any`.
//!
//! Arity is cig's: `params.len()` is the maximum number of fixed
//! arguments, `optional` of the trailing ones may be left out, and `rest`
//! is a variadic tail. A method's receiver is not in `params`.

use crate::types::Ty;

/// A signature. Type parameters appear as `Ty::Param("T")` etc.
#[derive(Clone, Debug, PartialEq)]
pub struct Sig {
    pub generics: &'static [&'static str],
    pub params: Vec<Ty>,
    /// How many of the trailing `params` may be left out.
    pub optional: usize,
    /// A variadic tail, if any.
    pub rest: Option<Ty>,
    pub ret: Ty,
    /// Needs a burn (cig's Effect::Write/Hop/...): true for anything cig refuses outside a burn.
    pub effect: bool,
}

// ----- builders --------------------------------------------------------------

fn list(t: Ty) -> Ty {
    Ty::List(Box::new(t))
}
fn map(t: Ty) -> Ty {
    Ty::Map(Box::new(t))
}
fn opt(t: Ty) -> Ty {
    Ty::Opt(Box::new(t))
}
fn fun(params: Vec<Ty>, ret: Ty) -> Ty {
    Ty::Fn(params, Box::new(ret))
}
fn p(name: &str) -> Ty {
    Ty::Param(name.to_string())
}
const ANY: Ty = Ty::Any;

/// `params`, the last `optional` of them optional, returning `ret`.
fn sig(params: Vec<Ty>, optional: usize, ret: Ty) -> Sig {
    Sig {
        generics: &[],
        params,
        optional,
        rest: None,
        ret,
        effect: false,
    }
}
/// A signature with type parameters.
fn gens(generics: &'static [&'static str], mut s: Sig) -> Sig {
    s.generics = generics;
    s
}
/// A variadic signature: `params` fixed, then any number of `rest`.
fn var(params: Vec<Ty>, rest: Ty, ret: Ty) -> Sig {
    Sig {
        generics: &[],
        params,
        optional: 0,
        rest: Some(rest),
        ret,
        effect: false,
    }
}
/// The same signature, needing a burn.
fn burn(mut s: Sig) -> Sig {
    s.effect = true;
    s
}

// ----- modules ---------------------------------------------------------------

/// Every module cig has, so `fs.x` is told from a field access.
const MODULES: &[&str] = &[
    "fs", "path", "json", "text", "proc", "time", "hash", "cig", "pass", "math", "env", "csv",
    "log",
];

pub fn is_module(name: &str) -> bool {
    MODULES.contains(&name)
}

/// A function: `module` is `Some("fs")` for `fs.read_text`, `None` for a
/// global like `len` or `str`.
pub fn function(module: Option<&str>, name: &str) -> Option<Sig> {
    match module {
        None => global(name),
        Some("fs") => fs(name),
        Some("path") => path(name),
        Some("json") => json(name),
        Some("text") => text(name),
        Some("proc") => proc(name),
        Some("time") => time(name),
        Some("hash") => hash(name),
        Some("cig") => cig(name),
        Some("pass") => pass(name),
        Some("math") => math(name),
        Some("env") => env(name),
        Some("csv") => csv(name),
        Some("log") => log(name),
        Some(_) => None,
    }
}

fn global(name: &str) -> Option<Sig> {
    Some(match name {
        "len" => sig(vec![ANY], 0, Ty::Int),
        "str" | "repr" => sig(vec![ANY], 0, Ty::Text),
        "int" => sig(vec![ANY], 0, Ty::Int),
        "float" => sig(vec![ANY], 0, Ty::Float),
        "bool" => sig(vec![ANY], 0, Ty::Bool),
        "type_of" => sig(vec![ANY], 0, Ty::Text),
        // range(stop) / range(start, stop) / range(start, stop, step)
        "range" => sig(vec![Ty::Int, Ty::Int, Ty::Int], 2, list(Ty::Int)),
        "keys" => gens(&["V"], sig(vec![map(p("V"))], 0, list(Ty::Text))),
        "values" => gens(&["V"], sig(vec![map(p("V"))], 0, list(p("V")))),
        "entries" => gens(&["V"], sig(vec![map(p("V"))], 0, list(list(ANY)))),
        "assert" => sig(vec![ANY, ANY], 1, Ty::Null),
        "exit" => sig(vec![Ty::Int], 1, Ty::Never),
        "stdin" => sig(vec![], 0, Ty::Text),
        // light(chain_or_steps, opts?) gives a report map
        "light" => sig(vec![ANY, ANY], 1, map(ANY)),
        _ => return None,
    })
}

fn fs(name: &str) -> Option<Sig> {
    Some(match name {
        "read_text" => sig(vec![Ty::Path], 0, Ty::Text),
        "read_lines" => sig(vec![Ty::Path], 0, list(Ty::Text)),
        "exists" | "is_file" | "is_dir" => sig(vec![Ty::Path], 0, Ty::Bool),
        "size" | "modified_ms" => sig(vec![Ty::Path], 0, Ty::Int),
        "list" => sig(vec![Ty::Path], 0, list(Ty::Path)),
        "glob" => sig(vec![Ty::Text], 0, list(Ty::Path)),
        "cwd" => sig(vec![], 0, Ty::Path),
        // null when no home is set
        "home" => sig(vec![], 0, ANY),
        // (path, text, sensitive?)
        "write_text" | "append_text" => burn(sig(vec![Ty::Path, Ty::Text, ANY], 1, ANY)),
        // true when it made the folder, false when it was there
        "mkdir" => burn(sig(vec![Ty::Path], 0, Ty::Bool)),
        "rm" => burn(sig(vec![Ty::Path], 0, Ty::Null)),
        "cp" | "mv" => burn(sig(vec![Ty::Path, Ty::Path], 0, ANY)),
        _ => return None,
    })
}

fn path(name: &str) -> Option<Sig> {
    Some(match name {
        "join" => var(vec![Ty::Path], Ty::Path, Ty::Path),
        "base" | "ext" | "stem" => sig(vec![Ty::Path], 0, Ty::Text),
        "dir" | "normalize" => sig(vec![Ty::Path], 0, Ty::Path),
        "with_ext" => sig(vec![Ty::Path, Ty::Text], 0, Ty::Path),
        "is_abs" => sig(vec![Ty::Path], 0, Ty::Bool),
        "abs" => sig(vec![Ty::Path], 0, Ty::Path),
        "parts" => sig(vec![Ty::Path], 0, list(Ty::Text)),
        _ => return None,
    })
}

fn json(name: &str) -> Option<Sig> {
    Some(match name {
        "parse" => sig(vec![Ty::Text], 0, ANY),
        // (value, pretty?)
        "stringify" => sig(vec![ANY, ANY], 1, Ty::Text),
        "load" => sig(vec![Ty::Path], 0, ANY),
        // (path, value, pretty?)
        "save" => burn(sig(vec![Ty::Path, ANY, ANY], 1, ANY)),
        _ => return None,
    })
}

fn text(name: &str) -> Option<Sig> {
    Some(match name {
        "matches" => sig(vec![Ty::Text, Ty::Text], 0, Ty::Bool),
        "find" => sig(vec![Ty::Text, Ty::Text], 0, opt(Ty::Text)),
        "find_all" | "split" => sig(vec![Ty::Text, Ty::Text], 0, list(Ty::Text)),
        // null when it does not match; a group that did not take part is null
        "captures" => sig(vec![Ty::Text, Ty::Text], 0, opt(list(opt(Ty::Text)))),
        "replace" => sig(vec![Ty::Text, Ty::Text, Ty::Text], 0, Ty::Text),
        "template" => sig(vec![Ty::Text, map(ANY)], 0, Ty::Text),
        _ => return None,
    })
}

fn proc(name: &str) -> Option<Sig> {
    // (command, args?, options?)
    let hop = |ret: Ty| burn(sig(vec![ANY, ANY, ANY], 2, ret));
    Some(match name {
        "run" => hop(map(ANY)),
        "text" => hop(Ty::Text),
        "lines" => hop(list(Ty::Text)),
        "json" => hop(ANY),
        // rows are lists or maps by the header option
        "csv" => hop(list(ANY)),
        "kv" => hop(map(ANY)),
        "pipe" => burn(sig(vec![ANY, ANY], 1, map(ANY))),
        "shell" => burn(sig(vec![Ty::Text, ANY], 1, map(ANY))),
        // the resolved path, or null
        "which" => sig(vec![Ty::Text], 0, ANY),
        "apply_layer" => burn(sig(vec![ANY, ANY], 1, ANY)),
        "discard_layer" => burn(sig(vec![ANY], 0, ANY)),
        _ => return None,
    })
}

fn time(name: &str) -> Option<Sig> {
    Some(match name {
        "now_ms" => sig(vec![], 0, Ty::Int),
        "now_iso" | "stamp" => sig(vec![], 0, Ty::Text),
        "sleep_ms" => sig(vec![Ty::Int], 0, Ty::Null),
        "format_ms" => sig(vec![Ty::Int, Ty::Text], 1, Ty::Text),
        _ => return None,
    })
}

fn hash(name: &str) -> Option<Sig> {
    Some(match name {
        // bytes are full's; cig takes what it can hash
        "sha256" => sig(vec![ANY], 0, Ty::Text),
        "sha256_file" => sig(vec![Ty::Path], 0, Ty::Text),
        "short" => sig(vec![ANY, Ty::Int], 1, Ty::Text),
        _ => return None,
    })
}

fn cig(name: &str) -> Option<Sig> {
    Some(match name {
        "version" | "os" | "arch" => sig(vec![], 0, Ty::Text),
        // null in a dry run and under `cig check`
        "run_id" => sig(vec![], 0, ANY),
        _ => return None,
    })
}

fn pass(name: &str) -> Option<Sig> {
    Some(match name {
        // (kind, subject, work, opts?)
        "issue" | "memo" => sig(vec![Ty::Text, ANY, ANY, ANY], 1, ANY),
        // (kind, subject, opts?): the newest pass, or null
        "find" => sig(vec![Ty::Text, ANY, ANY], 1, ANY),
        // (pass, action?) gives {ok, why}
        "check" => sig(vec![ANY, ANY], 1, ANY),
        "narrow" => sig(vec![ANY, ANY], 0, ANY),
        "spend" => sig(vec![ANY, ANY], 1, ANY),
        "show" => sig(vec![ANY], 0, ANY),
        "recording" => sig(vec![], 0, Ty::Bool),
        _ => return None,
    })
}

fn math(name: &str) -> Option<Sig> {
    // Int or Float in, Int or Float out: `Any` where it depends.
    Some(match name {
        "abs" => sig(vec![ANY], 0, ANY),
        "min" | "max" => var(vec![ANY], ANY, ANY),
        "floor" | "ceil" => sig(vec![Ty::Float], 0, Ty::Int),
        // round(x) is an Int, round(x, places) a Float
        "round" => sig(vec![Ty::Float, Ty::Int], 1, ANY),
        "sqrt" => sig(vec![Ty::Float], 0, Ty::Float),
        "pow" => sig(vec![Ty::Float, Ty::Float], 0, ANY),
        "clamp" => sig(vec![ANY, ANY, ANY], 0, ANY),
        _ => return None,
    })
}

fn env(name: &str) -> Option<Sig> {
    Some(match name {
        // (name, default?): text, the default, or null
        "get" => sig(vec![Ty::Text, ANY], 1, ANY),
        "has" => sig(vec![Ty::Text], 0, Ty::Bool),
        "all" => sig(vec![], 0, map(Ty::Text)),
        "set" => burn(sig(vec![Ty::Text, ANY], 0, Ty::Null)),
        "unset" => burn(sig(vec![Ty::Text], 0, Ty::Null)),
        _ => return None,
    })
}

fn csv(name: &str) -> Option<Sig> {
    Some(match name {
        // rows are lists or maps by the header option
        "parse" => sig(vec![Ty::Text, ANY], 1, ANY),
        "stringify" => sig(vec![list(ANY), ANY], 1, Ty::Text),
        "read" => sig(vec![Ty::Path, ANY], 1, ANY),
        "write" => burn(sig(vec![Ty::Path, list(ANY), ANY], 1, ANY)),
        _ => return None,
    })
}

fn log(name: &str) -> Option<Sig> {
    Some(match name {
        "debug" | "info" | "warn" | "error" => var(vec![ANY], ANY, Ty::Null),
        _ => return None,
    })
}

// ----- methods ---------------------------------------------------------------

/// A method on a receiver of type `recv`. For `List<T>` the element type is
/// bound to `"T"`, for `Map<V>` the value type to `"V"`. `Text` and `Path`
/// share text's methods. `Any` and every other type have none here: the
/// checker leaves them alone.
pub fn method(recv: &Ty, name: &str) -> Option<Sig> {
    match recv {
        Ty::Text | Ty::Path => text_method(name),
        Ty::List(_) | Ty::ListView(_) => list_method(name),
        Ty::Map(_) => map_method(name),
        _ => None,
    }
}

fn text_method(name: &str) -> Option<Sig> {
    let t = Ty::Text;
    Some(match name {
        "len" | "to_int" => sig(vec![], 0, Ty::Int),
        "upper" | "lower" | "trim" | "trim_start" | "trim_end" | "reverse" => sig(vec![], 0, t),
        "split" => sig(vec![t], 1, list(Ty::Text)),
        "lines" | "chars" => sig(vec![], 0, list(Ty::Text)),
        "contains" | "starts_with" | "ends_with" => sig(vec![t], 0, Ty::Bool),
        "replace" => sig(vec![Ty::Text, Ty::Text], 0, t),
        // the char index, or -1
        "find" => sig(vec![t], 0, Ty::Int),
        "slice" => sig(vec![Ty::Int, Ty::Int], 1, t),
        "repeat" => sig(vec![Ty::Int], 0, t),
        "pad_left" | "pad_right" => sig(vec![Ty::Int, Ty::Text], 1, t),
        "is_empty" => sig(vec![], 0, Ty::Bool),
        "to_float" => sig(vec![], 0, Ty::Float),
        _ => return None,
    })
}

fn list_method(name: &str) -> Option<Sig> {
    let t = || p("T");
    let lt = || list(p("T"));
    const T: &[&str] = &["T"];
    const TU: &[&str] = &["T", "U"];
    Some(match name {
        "len" | "index_of" => {
            if name == "len" {
                gens(T, sig(vec![], 0, Ty::Int))
            } else {
                gens(T, sig(vec![ANY], 0, Ty::Int))
            }
        }
        "is_empty" => gens(T, sig(vec![], 0, Ty::Bool)),
        // pushes in place and gives the list back
        "push" => gens(T, var(vec![t()], t(), lt())),
        "pop" | "first" | "last" => gens(T, sig(vec![], 0, opt(t()))),
        "insert" => gens(T, sig(vec![Ty::Int, t()], 0, lt())),
        "remove" => gens(T, sig(vec![Ty::Int], 0, t())),
        // (index, default?): the default may be any type
        "get" => gens(T, sig(vec![Ty::Int, ANY], 1, ANY)),
        "slice" => gens(T, sig(vec![Ty::Int, Ty::Int], 1, lt())),
        "contains" => gens(T, sig(vec![ANY], 0, Ty::Bool)),
        "map" => gens(TU, sig(vec![fun(vec![t()], p("U"))], 0, list(p("U")))),
        "filter" => gens(T, sig(vec![fun(vec![t()], ANY)], 0, lt())),
        "reduce" => gens(
            TU,
            sig(vec![fun(vec![p("U"), t()], p("U")), p("U")], 0, p("U")),
        ),
        "each" => gens(T, sig(vec![fun(vec![t()], ANY)], 0, Ty::Null)),
        "any" | "all" => gens(T, sig(vec![fun(vec![t()], ANY)], 0, Ty::Bool)),
        "find" => gens(T, sig(vec![fun(vec![t()], ANY)], 0, opt(t()))),
        "sort" | "reverse" | "unique" | "copy" => gens(T, sig(vec![], 0, lt())),
        "sort_by" => gens(T, sig(vec![fun(vec![t()], ANY)], 0, lt())),
        // elements are shown as text
        "join" => gens(T, sig(vec![Ty::Text], 1, Ty::Text)),
        // one level; non-list elements stay as they are
        "flatten" => gens(T, sig(vec![], 0, list(ANY))),
        // Int or Float; min and max of an empty list
        "sum" | "min" | "max" => gens(T, sig(vec![], 0, ANY)),
        "zip" => gens(T, sig(vec![list(ANY)], 0, list(list(ANY)))),
        "enumerate" => gens(T, sig(vec![], 0, list(list(ANY)))),
        "chunks" => gens(T, sig(vec![Ty::Int], 0, list(lt()))),
        _ => return None,
    })
}

fn map_method(name: &str) -> Option<Sig> {
    const V: &[&str] = &["V"];
    let v = || p("V");
    let mv = || map(p("V"));
    Some(match name {
        "len" => gens(V, sig(vec![], 0, Ty::Int)),
        "is_empty" => gens(V, sig(vec![], 0, Ty::Bool)),
        "keys" => gens(V, sig(vec![], 0, list(Ty::Text))),
        "values" => gens(V, sig(vec![], 0, list(v()))),
        "entries" => gens(V, sig(vec![], 0, list(list(ANY)))),
        "has" => gens(V, sig(vec![Ty::Text], 0, Ty::Bool)),
        // (key, default?): the default may be any type
        "get" => gens(V, sig(vec![Ty::Text, ANY], 1, ANY)),
        // sets in place and gives the map back
        "set" => gens(V, sig(vec![Ty::Text, v()], 0, mv())),
        // the removed value, or null
        "remove" => gens(V, sig(vec![Ty::Text], 0, opt(v()))),
        "merge" => gens(V, sig(vec![mv()], 0, mv())),
        "copy" => gens(V, sig(vec![], 0, mv())),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// cig 1.20.0's tables, name and whether it needs a burn.
    const TABLES: &[(&str, &[(&str, bool)])] = &[
        (
            "fs",
            &[
                ("read_text", false),
                ("read_lines", false),
                ("exists", false),
                ("is_file", false),
                ("is_dir", false),
                ("size", false),
                ("modified_ms", false),
                ("list", false),
                ("glob", false),
                ("cwd", false),
                ("home", false),
                ("write_text", true),
                ("append_text", true),
                ("mkdir", true),
                ("rm", true),
                ("cp", true),
                ("mv", true),
            ],
        ),
        (
            "path",
            &[
                ("join", false),
                ("base", false),
                ("dir", false),
                ("ext", false),
                ("stem", false),
                ("with_ext", false),
                ("normalize", false),
                ("is_abs", false),
                ("abs", false),
                ("parts", false),
            ],
        ),
        (
            "json",
            &[
                ("parse", false),
                ("stringify", false),
                ("load", false),
                ("save", true),
            ],
        ),
        (
            "text",
            &[
                ("matches", false),
                ("find", false),
                ("find_all", false),
                ("captures", false),
                ("replace", false),
                ("split", false),
                ("template", false),
            ],
        ),
        (
            "proc",
            &[
                ("run", true),
                ("text", true),
                ("lines", true),
                ("json", true),
                ("csv", true),
                ("kv", true),
                ("pipe", true),
                ("shell", true),
                ("which", false),
                ("apply_layer", true),
                ("discard_layer", true),
            ],
        ),
        (
            "time",
            &[
                ("now_ms", false),
                ("now_iso", false),
                ("stamp", false),
                ("sleep_ms", false),
                ("format_ms", false),
            ],
        ),
        (
            "hash",
            &[("sha256", false), ("sha256_file", false), ("short", false)],
        ),
        (
            "cig",
            &[
                ("version", false),
                ("os", false),
                ("arch", false),
                ("run_id", false),
            ],
        ),
        (
            "pass",
            &[
                ("issue", false),
                ("find", false),
                ("memo", false),
                ("check", false),
                ("narrow", false),
                ("spend", false),
                ("show", false),
                ("recording", false),
            ],
        ),
        (
            "math",
            &[
                ("abs", false),
                ("min", false),
                ("max", false),
                ("floor", false),
                ("ceil", false),
                ("round", false),
                ("sqrt", false),
                ("pow", false),
                ("clamp", false),
            ],
        ),
        (
            "env",
            &[
                ("get", false),
                ("has", false),
                ("all", false),
                ("set", true),
                ("unset", true),
            ],
        ),
        (
            "csv",
            &[
                ("parse", false),
                ("stringify", false),
                ("read", false),
                ("write", true),
            ],
        ),
        (
            "log",
            &[
                ("debug", false),
                ("info", false),
                ("warn", false),
                ("error", false),
            ],
        ),
    ];

    const GLOBALS: &[&str] = &[
        "len", "str", "repr", "int", "float", "bool", "type_of", "range", "keys", "values",
        "entries", "assert", "exit", "stdin", "light",
    ];

    const STR: &[&str] = &[
        "len",
        "upper",
        "lower",
        "trim",
        "trim_start",
        "trim_end",
        "split",
        "lines",
        "chars",
        "contains",
        "starts_with",
        "ends_with",
        "replace",
        "find",
        "slice",
        "repeat",
        "pad_left",
        "pad_right",
        "reverse",
        "is_empty",
        "to_int",
        "to_float",
    ];
    const LIST: &[&str] = &[
        "len",
        "is_empty",
        "push",
        "pop",
        "insert",
        "remove",
        "first",
        "last",
        "get",
        "slice",
        "contains",
        "index_of",
        "map",
        "filter",
        "reduce",
        "each",
        "any",
        "all",
        "find",
        "sort",
        "sort_by",
        "reverse",
        "join",
        "unique",
        "flatten",
        "sum",
        "min",
        "max",
        "zip",
        "enumerate",
        "chunks",
        "copy",
    ];
    const MAP: &[&str] = &[
        "len", "is_empty", "keys", "values", "entries", "has", "get", "set", "remove", "merge",
        "copy",
    ];

    #[test]
    fn every_function_in_cigs_tables_is_known_with_its_effect() {
        let mut n = 0;
        for (module, names) in TABLES {
            assert!(is_module(module), "{module} is not a module");
            for (name, effect) in *names {
                let s = function(Some(module), name)
                    .unwrap_or_else(|| panic!("missing {module}.{name}"));
                assert_eq!(s.effect, *effect, "effect of {module}.{name}");
                assert!(s.optional <= s.params.len(), "{module}.{name}");
                n += 1;
            }
        }
        for g in GLOBALS {
            assert!(function(None, g).is_some(), "missing global {g}");
            n += 1;
        }
        assert_eq!(n, 17 + 10 + 4 + 7 + 11 + 5 + 3 + 4 + 8 + 9 + 5 + 4 + 4 + 15);
    }

    #[test]
    fn every_method_is_known_and_unknowns_are_not() {
        for m in STR {
            assert!(method(&Ty::Text, m).is_some(), "text.{m}");
            assert!(method(&Ty::Path, m).is_some(), "path.{m}");
        }
        for m in LIST {
            assert!(
                method(&Ty::List(Box::new(Ty::Int)), m).is_some(),
                "list.{m}"
            );
        }
        for m in MAP {
            assert!(method(&Ty::Map(Box::new(Ty::Int)), m).is_some(), "map.{m}");
        }
        assert!(method(&Ty::Text, "nope").is_none());
        assert!(method(&Ty::Any, "len").is_none());
        assert!(function(Some("fs"), "nope").is_none());
        assert!(function(Some("nope"), "x").is_none());
        assert!(function(None, "nope").is_none());
        assert!(!is_module("nope"));
        assert!(is_module("fs") && is_module("math") && is_module("pass"));
    }

    #[test]
    fn a_few_signatures_read_right() {
        let s = function(Some("fs"), "list").unwrap();
        assert_eq!(s.ret, Ty::List(Box::new(Ty::Path)));
        assert!(!s.effect);
        let s = function(Some("path"), "join").unwrap();
        assert_eq!(s.ret, Ty::Path);
        assert!(s.rest.is_some());
        let s = function(Some("fs"), "write_text").unwrap();
        assert!(s.effect);
        assert_eq!((s.params.len(), s.optional), (3, 1));
        let s = method(&Ty::List(Box::new(Ty::Int)), "map").unwrap();
        assert_eq!(s.generics, &["T", "U"]);
        assert_eq!(
            s.params,
            vec![Ty::Fn(
                vec![Ty::Param("T".into())],
                Box::new(Ty::Param("U".into()))
            )]
        );
        assert_eq!(s.ret, Ty::List(Box::new(Ty::Param("U".into()))));
        let s = method(&Ty::Map(Box::new(Ty::Int)), "remove").unwrap();
        assert_eq!(s.ret, Ty::Opt(Box::new(Ty::Param("V".into()))));
        let s = function(None, "range").unwrap();
        assert_eq!((s.params.len(), s.optional), (3, 2));
    }
}
