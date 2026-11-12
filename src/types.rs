// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! CigarScript's types, and what the lowering pass hands the checker.
//!
//! The pipeline (docs/LANGUAGE.md, "How a file is checked and run"):
//!
//! 1. `lower` reads a `.cigar` file's new constructs (records, enums,
//!    `match`, annotations, ...) and rewrites them **in place** into
//!    CigScript text that keeps every line on its line. Helpers and
//!    lowered declarations are `pull`s appended after the last line (cig
//!    hoists `pull`s). It returns the text and a [`Program`]: the
//!    declarations and every annotation, keyed by **offsets in the lowered
//!    text**.
//! 2. cig parses and checks the lowered text, exactly as `cig check` does.
//! 3. The type checker walks cig's AST of the lowered text with the
//!    [`Program`] beside it.
//! 4. `cigar run` hands the lowered text to cig's own run machinery.
//!
//! Every name the lowering makes starts with `_cg_` (functions) or
//! `__cg_` (temporaries); a `.cigar` file may not use either prefix.

use std::collections::BTreeMap;
use std::fmt;

/// A type. `Any` is the type of every untyped value (mild code, and an
/// unannotated binding the checker cannot infer): it is compatible both
/// ways, and a value crossing from `Any` into a typed place is checked at
/// run time, once, at that boundary.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Ty {
    Any,
    /// The type of `null`.
    Null,
    Bool,
    Int,
    Float,
    /// Text: a string.
    Text,
    /// A path: text that names a file or folder. A `Path` is accepted
    /// where `Text` is (it is text), and text is accepted where a `Path`
    /// is (cig's paths are strings); the difference is the layer `into`
    /// and `measure` pick for it.
    Path,
    /// Raw bytes (full).
    Bytes,
    /// A sized integer (full): `u8`..`u64`, `i8`..`i64`. Checked for range
    /// on every arithmetic result.
    Sized(SizedInt),
    List(Box<Ty>),
    /// `ListView<T>`: a list that can be read and not written through. It is
    /// covariant in `T`, which a `List<T>` is not.
    ListView(Box<Ty>),
    /// Keys are text in cig; `Map<V>` and `Map<Text, V>` are the same.
    Map(Box<Ty>),
    /// `T?`: a `T` or `null`.
    Opt(Box<Ty>),
    /// A function: parameters, result.
    Fn(Vec<Ty>, Box<Ty>),
    /// A record by name, with its type arguments.
    Record(String, Vec<Ty>),
    /// An enum by name, with its type arguments.
    Enum(String, Vec<Ty>),
    /// A type parameter in a generic declaration.
    Param(String),
    /// A channel of `T` (full).
    Channel(Box<Ty>),
    /// What a block that never finishes (a `cough`) has: fits anywhere.
    Never,
}

/// The sized integers of full.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SizedInt {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
}

impl SizedInt {
    pub const ALL: [(&'static str, SizedInt); 8] = [
        ("u8", SizedInt::U8),
        ("u16", SizedInt::U16),
        ("u32", SizedInt::U32),
        ("u64", SizedInt::U64),
        ("i8", SizedInt::I8),
        ("i16", SizedInt::I16),
        ("i32", SizedInt::I32),
        ("i64", SizedInt::I64),
    ];

    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(_, s)| *s == self)
            .map(|(n, _)| *n)
            .unwrap_or("i64")
    }

    /// The inclusive range, as cig's 64-bit integers can hold it (`u64`
    /// is held to `i64::MAX`: cig's integers are signed 64-bit).
    pub fn range(self) -> (i64, i64) {
        match self {
            SizedInt::U8 => (0, u8::MAX as i64),
            SizedInt::U16 => (0, u16::MAX as i64),
            SizedInt::U32 => (0, u32::MAX as i64),
            SizedInt::U64 => (0, i64::MAX),
            SizedInt::I8 => (i8::MIN as i64, i8::MAX as i64),
            SizedInt::I16 => (i16::MIN as i64, i16::MAX as i64),
            SizedInt::I32 => (i32::MIN as i64, i32::MAX as i64),
            SizedInt::I64 => (i64::MIN, i64::MAX),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fn args(f: &mut fmt::Formatter<'_>, a: &[Ty]) -> fmt::Result {
            if a.is_empty() {
                return Ok(());
            }
            write!(f, "<")?;
            for (i, t) in a.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{t}")?;
            }
            write!(f, ">")
        }
        match self {
            Ty::Any => write!(f, "Any"),
            Ty::Null => write!(f, "Null"),
            Ty::Bool => write!(f, "Bool"),
            Ty::Int => write!(f, "Int"),
            Ty::Float => write!(f, "Float"),
            Ty::Text => write!(f, "Text"),
            Ty::Path => write!(f, "Path"),
            Ty::Bytes => write!(f, "Bytes"),
            Ty::Sized(s) => write!(f, "{}", s.name()),
            Ty::List(t) => write!(f, "List<{t}>"),
            Ty::ListView(t) => write!(f, "ListView<{t}>"),
            Ty::Map(t) => write!(f, "Map<{t}>"),
            Ty::Opt(t) => write!(f, "{t}?"),
            Ty::Fn(ps, r) => {
                write!(f, "(")?;
                for (i, p) in ps.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{p}")?;
                }
                write!(f, ") -> {r}")
            }
            Ty::Record(n, a) | Ty::Enum(n, a) => {
                write!(f, "{n}")?;
                args(f, a)
            }
            Ty::Param(n) => write!(f, "{n}"),
            Ty::Channel(t) => write!(f, "Channel<{t}>"),
            Ty::Never => write!(f, "Never"),
        }
    }
}

/// Where something was written in the **original** file (byte offsets),
/// for diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct At {
    pub start: usize,
    pub end: usize,
}

/// `record Name<T> { field: Type, ... }`.
#[derive(Clone, Debug, PartialEq)]
pub struct RecordDecl {
    pub name: String,
    pub generics: Vec<String>,
    /// In declaration order.
    pub fields: Vec<(String, Ty)>,
    pub public: bool,
    pub at: At,
}

/// One variant of an enum: `ok`, `failed(reason: Text)` or `pair(Int, Int)`
/// (positional fields are named `_0`, `_1`, ...).
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<(String, Ty)>,
    pub at: At,
}

/// `enum Name<T> { variant, variant(field: Type), ... }`.
#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: String,
    pub generics: Vec<String>,
    pub variants: Vec<Variant>,
    pub public: bool,
    pub at: At,
}

/// A function's signature as written. Unannotated parameters and results
/// are `None` (typed `Any`, or inferred inside the function).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FnSig {
    pub name: String,
    pub generics: Vec<(String, Vec<String>)>,
    pub params: Vec<Param>,
    pub ret: Option<Ty>,
    /// `coughs E1, E2`: the record or enum types this function may cough.
    pub coughs: Vec<Ty>,
    pub public: bool,
    pub at: At,
}

/// One parameter: its name, its type if written, and its ownership mode
/// (full).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Param {
    pub name: String,
    pub ty: Option<Ty>,
    pub mode: Mode,
}

/// How a parameter holds its value (full, #12). Mild and medium code is
/// always `Shared`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Shared,
    Owned,
    Borrow,
}

/// `interface Name { pull method(self, ...) -> T ... }`.
#[derive(Clone, Debug, PartialEq)]
pub struct InterfaceDecl {
    pub name: String,
    pub methods: Vec<FnSig>,
    /// The methods written with a body (#38, default methods): an `impl`
    /// that leaves one out gets the interface's.
    pub defaults: Vec<String>,
    pub public: bool,
    pub at: At,
}

/// `impl Interface for Type { pull ... }`: each method is lowered to a
/// function named by [`impl_fn_name`].
#[derive(Clone, Debug, PartialEq)]
pub struct ImplDecl {
    /// `impl<T: Show> Show for List<T>` (#38): the impl's type parameters
    /// and their bounds.
    pub generics: Vec<(String, Vec<String>)>,
    pub interface: String,
    pub for_ty: Ty,
    pub methods: Vec<FnSig>,
    pub at: At,
}

/// A layer: `measure for Path = fs.size` (or `into`, or a layer declared
/// with `layer name` at full). `function` is the CigScript expression the
/// layer reduces to, as written.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerDecl {
    pub layer: String,
    pub for_ty: Ty,
    pub function: String,
    pub at: At,
}

/// What lowering hands the checker. Every map is keyed by a byte offset in
/// the **lowered** text, at the start of the token the AST node's span
/// starts at (the `stick`/`roll` keyword of a declaration, the `pull`
/// keyword of a function, the `pack` keyword of a lambda).
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub records: BTreeMap<String, RecordDecl>,
    pub enums: BTreeMap<String, EnumDecl>,
    pub interfaces: BTreeMap<String, InterfaceDecl>,
    pub impls: Vec<ImplDecl>,
    pub layers: Vec<LayerDecl>,
    /// `pull` signatures, by the lowered offset of `pull`.
    pub fns: BTreeMap<usize, FnSig>,
    /// `stick x: T = ...` / `roll x: T = ...`, and every binding a `match`
    /// pattern makes, by the lowered offset of the statement's keyword.
    pub bindings: BTreeMap<usize, Ty>,
    /// Typed lambda parameters, by the lowered offset of `pack`.
    pub lambdas: BTreeMap<usize, Vec<Option<Ty>>>,
    /// Every name given to `move` (full), by the lowered offset of the name.
    pub moves: std::collections::BTreeSet<usize>,
    /// Every `parallel(...) for` loop (full), by the lowered offset of `for`.
    pub parallel_fors: std::collections::BTreeSet<usize>,
    /// The functions modules export, as this file reaches them: `plan.f`
    /// for `use tools.plan`, and `f` for `use tools.plan.{f}`.
    pub module_fns: BTreeMap<String, FnSig>,
    /// The interfaces, records and enums a module declares and this file
    /// uses: what the orphan rule (#38, `E1231`) does not let it `impl`
    /// together.
    pub foreign: std::collections::BTreeSet<String>,
}

/// The built-in `enum Result<T, E> { Ok(value: T), Err(error: E) }` (#2),
/// which every medium or full file has unless it declares a `Result` of
/// its own. `Ok(v)` and `Err(e)` are its variants, spelled bare.
pub fn result_decl() -> EnumDecl {
    EnumDecl {
        name: "Result".to_string(),
        generics: vec!["T".to_string(), "E".to_string()],
        variants: vec![
            Variant {
                name: "Ok".to_string(),
                fields: vec![("value".to_string(), Ty::Param("T".to_string()))],
                at: At::default(),
            },
            Variant {
                name: "Err".to_string(),
                fields: vec![("error".to_string(), Ty::Param("E".to_string()))],
                at: At::default(),
            },
        ],
        public: true,
        at: At::default(),
    }
}

/// The name of the lowered function that implements `method` of
/// `interface` for `ty` (a layer is an interface with one method).
pub fn impl_fn_name(interface: &str, ty: &Ty, method: &str) -> String {
    let t: String = ty
        .to_string()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    format!("_cg_impl_{interface}_{t}_{method}")
}

/// The built-in type names, and what each means. Anything else that
/// starts with a capital letter is a record, an enum or a type parameter.
pub const BUILTIN_TYPES: &[(&str, &str)] = &[
    ("Any", "any value; checked where it meets a typed place"),
    ("Null", "null"),
    ("Bool", "true or false"),
    ("Int", "a 64-bit integer"),
    ("Float", "a 64-bit float"),
    ("Text", "text"),
    ("Path", "text that names a file or folder"),
    ("Bytes", "raw bytes (full)"),
    ("List", "List<T>: a list of T"),
    (
        "ListView",
        "ListView<T>: a list read through, never written (covariant)",
    ),
    ("Map", "Map<V> or Map<Text, V>: text keys to V"),
    ("Channel", "Channel<T>: a channel of T (full)"),
    (
        "Result",
        "Result<T, E>: Ok(T) or Err(E), unless the file declares its own",
    ),
];
