// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Lowering: a `.cigar` file's constructs rewritten **in place** into
//! CigScript, keeping every line on its line (docs/LANGUAGE.md, "How a file
//! is checked and run"). cig's own lexer reads the file; a construct is
//! rewritten inside its own lines; lowered declarations and the run-time
//! prelude are `pull`s appended after the last line (cig hoists `pull`).
//! The result is ordinary CigScript, which cig parses, checks and runs.

mod bounds;
mod derive;
mod impls;
mod json;
mod keyed;
mod standard;
pub use impls::SELF;
pub use standard::{arith_op, ARITH};
pub mod edits;
pub mod ty;

use crate::features::Strength;
use crate::types::*;
use cigscript::diagnostics::{Diagnostic, Kind as DK};
use cigscript::syntax::lexer::tokenize;
use cigscript::syntax::span::Span;
use cigscript::syntax::token::{StrPart, Token, TokenKind as T};
use edits::{Applied, Edits};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// The run-time helpers, in CigScript, appended to every lowered file
/// that needs them.
pub const PRELUDE: &str = include_str!("prelude.cig");

/// The run-time prelude, without the built-in `Result` constructors when the
/// file declares a `Result` of its own (whose constructors it lowers itself).
fn prelude(user_result: bool) -> String {
    if !user_result {
        return PRELUDE.to_string();
    }
    match (PRELUDE.find(RESULT_BEGIN), PRELUDE.find(RESULT_END)) {
        (Some(a), Some(b)) if a < b => {
            let tail = b + RESULT_END.len();
            format!("{}{}", &PRELUDE[..a], &PRELUDE[tail..])
        }
        _ => PRELUDE.to_string(),
    }
}

const RESULT_BEGIN: &str = "# ===== the built-in Result: begin";
const RESULT_END: &str = "# ===== the built-in Result: end";

/// A lowered file.
pub struct Lowered {
    /// The CigScript text: the file rewritten, then its appended `pull`s.
    pub text: String,
    /// What the checker needs, keyed by offsets in `text`.
    pub program: Program,
    applied: Applied,
    /// Where the appended text starts: anything at or after it maps to
    /// `appended_anchor` in the original.
    appended_at: usize,
    /// The errors lowering found, with spans in the original file.
    pub diags: Vec<Diagnostic>,
    /// The modules this file uses, by path.
    pub modules: Vec<PathBuf>,
    /// Every `within` block, for `cigar explain --layers`.
    pub withins: Vec<WithinNote>,
}

/// A `within` block as written: where it is and what it was given.
#[derive(Clone, Debug, PartialEq)]
pub struct WithinNote {
    /// The `within` word.
    pub start: usize,
    /// The end of the head (just before the block's `{`).
    pub head_end: usize,
    /// The end of the block's `}`.
    pub end: usize,
    /// `retry`, `parallel` or `timeout`.
    pub context: String,
    /// The text of the context as written (`retry(3, backoff: 2s)`).
    pub head: String,
    /// `retry`: tries; `timeout`: the limit as written; `parallel`: threads.
    pub arg: String,
    /// `retry`: milliseconds between tries. `timeout`: the limit in milliseconds
    /// when it was written as a duration.
    pub ms: i64,
}

impl Lowered {
    /// The original byte offset a lowered offset came from.
    pub fn original(&self, lowered: usize) -> usize {
        if lowered >= self.appended_at {
            return self.applied.original(self.appended_at.saturating_sub(1));
        }
        self.applied.original(lowered)
    }

    /// Where the appended text (declarations, modules, the prelude) starts.
    pub fn appended_at(&self) -> usize {
        self.appended_at
    }

    /// Was the text at a lowered offset written in the file (not by the
    /// lowering)?
    pub fn copied(&self, lowered: usize) -> bool {
        lowered < self.appended_at && self.applied.copied(lowered)
    }
}

/// A module, checked and lowered: what a file that uses it needs.
#[derive(Clone, Debug, Default)]
pub struct Module {
    /// The dotted name it is used by: `tools.plan`.
    pub name: String,
    pub path: PathBuf,
    /// Its lowered body, which becomes the body of `_cg_mod_...()`.
    pub body: String,
    /// Its own lowered declarations (constructors, dispatchers), appended
    /// at the top level.
    pub decls: String,
    /// Its declarations and signatures.
    pub program: Program,
    /// Every name it exports with `pub`, and what each is.
    pub exports: Vec<(String, Export)>,
    /// Every name it declares at the top level, exported or not.
    pub declared: Vec<String>,
    /// Its own errors (spans in its own file), and the first one's text.
    pub errors: Vec<Diagnostic>,
}

/// What an exported name is.
#[derive(Clone, Debug, PartialEq)]
pub enum Export {
    Interface,
    Record,
    Enum,
    Function(FnSig),
    Value,
}

/// How to lower a file.
pub struct Options<'a> {
    pub strength: Strength,
    /// The file being lowered, for messages.
    pub file: Option<&'a Path>,
    /// The folder modules are found in: the folder of the file being
    /// checked or run, for that file and every module it reaches.
    pub root: Option<&'a Path>,
    /// The modules being lowered above this one, for cycles: (name, path).
    pub stack: Vec<(String, PathBuf)>,
    /// Every module the program uses, by its function name, shared by the
    /// file being checked and all its modules.
    pub modules: &'a std::cell::RefCell<BTreeMap<String, Module>>,
    /// The name of the file being lowered, for a cycle's message: its dotted
    /// module name, or the file's stem.
    pub name: String,
    /// Compiles a module (lower, type-check, resolve); `crate::check`
    /// supplies it.
    pub compile: &'a dyn Fn(&Path, &str, &str, &Options) -> Module,
}

/// One pattern of a `match` arm.
#[derive(Clone, Debug)]
enum Pat {
    Wild,
    Lit(String),
    Bind(String),
    Variant {
        enum_name: Option<String>,
        name: String,
        subs: Vec<Pat>,
    },
    Record {
        name: String,
        fields: Vec<(String, Pat)>,
    },
}

/// Where a `match` used as a value puts its result.
#[derive(Clone, Debug)]
enum Target {
    /// A statement: no value.
    Stmt,
    /// `<text>__cg_rN` after the match: `stick x = `, `snuff `, `x = `.
    Value {
        /// Token index where the statement starts (replaced up to `match`).
        stmt_start: usize,
        then: String,
        /// The declared type of a `stick`/`roll` it binds, if any.
        ty: Option<Ty>,
        name: Option<String>,
    },
}

struct L<'a> {
    src: &'a str,
    work: String,
    toks: Vec<Token>,
    opt: BTreeSet<usize>,
    e: Edits,
    prog: Program,
    /// Bindings written into edit texts: (edit seq, offset in its text, type).
    gen_bindings: Vec<(usize, usize, Ty)>,
    /// Keys at original offsets, converted at the end.
    fn_keys: Vec<(usize, FnSig)>,
    bind_keys: Vec<(usize, Ty)>,
    lambda_keys: Vec<(usize, Vec<Option<Ty>>)>,
    diags: Vec<Diagnostic>,
    appended: String,
    n: usize,
    strength: Strength,
    /// The enclosing functions' declared results, innermost last.
    rets: Vec<Option<(Ty, String)>>,
    show: bool,
    layers: BTreeSet<String>,
    /// Every `within` block seen.
    withins: Vec<WithinNote>,
    /// The deadlines of the `within timeout` blocks being lowered, by name.
    deadlines: Vec<(String, String)>,
    generic_fns: BTreeSet<String>,
    lines: Vec<usize>,
    opts: &'a Options<'a>,
    modules: Vec<PathBuf>,
    /// The lowered offsets (as original offsets, converted at the end) of
    /// every name given to `move`.
    move_keys: Vec<usize>,
    parallel_keys: Vec<usize>,
    /// The modules this file has bound (`use`), by function name.
    bound_modules: BTreeSet<String>,
    /// Records and enums of a module this file uses that are not `pub`:
    /// name -> module.
    private_types: BTreeMap<String, String>,
    /// Type parameters in scope (of the enclosing generic function).
    tparams: Vec<String>,
    /// The type parameters of the `impl` (or the default method's `Self`)
    /// whose methods are being lowered (#38).
    impl_generics: Vec<(String, Vec<String>)>,
    public_next: bool,
    record_names: BTreeSet<String>,
    enum_names: BTreeSet<String>,
    /// The built-in `Result<T, E>` is this file's (it was written about and
    /// the file declares no `Result` of its own): `Ok`, `Err` and `attempt`
    /// are its words.
    builtin_result: bool,
    /// The file declares an enum named `Result`: the built-in steps aside.
    user_result: bool,
    /// The functions being lowered, innermost last: does a `?` in it need
    /// the function to catch what `?` throws?
    try_fns: Vec<TryFn>,
    /// How many `try` blocks of the function being lowered enclose the
    /// statement.
    try_depth: usize,
    /// `?` positions already lowered (a range can be walked twice).
    tries_done: BTreeSet<usize>,
    /// The file declares a `pull attempt`: it is the file's, not the
    /// built-in's.
    user_attempt: bool,
}

/// A function body being lowered, for `?`.
struct TryFn {
    /// A `pull`: its body can catch a `?`. A `pack` cannot.
    pull: bool,
    used: bool,
}

struct Names<'b>(&'b BTreeSet<String>, &'b BTreeSet<String>);
impl ty::Names for Names<'_> {
    fn is_record(&self, n: &str) -> bool {
        self.0.contains(n)
    }
    fn is_enum(&self, n: &str) -> bool {
        self.1.contains(n)
    }
}

/// Lower `src` (a `.cigar` file whose `strength` declaration is already
/// blanked) at `opts.strength`.
pub fn lower(src: &str, opts: &Options) -> Lowered {
    let (work, toks, opt, raw) = match mask_raw(src) {
        Ok(x) => x,
        Err(d) => {
            let mut e = Edits::default();
            let applied = e.apply(src);
            let _ = &mut e;
            return Lowered {
                text: src.to_string(),
                program: Program::default(),
                appended_at: src.len(),
                applied,
                diags: vec![d],
                modules: Vec::new(),
                withins: Vec::new(),
            };
        }
    };
    // A raw identifier is written to cig as the name `mask` read it as.
    let raw_edits: Vec<(usize, usize, &str)> = raw
        .iter()
        .map(|&at| match work.as_bytes()[at + 1] {
            b'_' => (at + 1, at + 2, "_"),
            _ => (at, at + 2, "  "),
        })
        .collect();
    let mut lines = vec![0];
    for (i, b) in src.bytes().enumerate() {
        if b == b'\n' {
            lines.push(i + 1);
        }
    }
    let mut l = L {
        src,
        work,
        toks,
        opt,
        e: Edits::default(),
        prog: Program::default(),
        gen_bindings: Vec::new(),
        fn_keys: Vec::new(),
        bind_keys: Vec::new(),
        lambda_keys: Vec::new(),
        diags: Vec::new(),
        appended: String::new(),
        n: 0,
        strength: opts.strength,
        rets: Vec::new(),
        show: false,
        layers: ["measure", "into"].iter().map(|s| s.to_string()).collect(),
        withins: Vec::new(),
        deadlines: Vec::new(),
        generic_fns: BTreeSet::new(),
        lines,
        opts,
        modules: Vec::new(),
        move_keys: Vec::new(),
        parallel_keys: Vec::new(),
        bound_modules: BTreeSet::new(),
        private_types: BTreeMap::new(),
        tparams: Vec::new(),
        impl_generics: Vec::new(),
        public_next: false,
        record_names: BTreeSet::new(),
        enum_names: BTreeSet::new(),
        builtin_result: false,
        user_result: false,
        try_fns: Vec::new(),
        try_depth: 0,
        tries_done: BTreeSet::new(),
        user_attempt: false,
    };
    for (a, b, text) in raw_edits {
        l.e.replace(a, b, text);
    }
    l.reserved_names();
    l.declarations();
    l.show = !l.prog.records.is_empty() || !l.prog.enums.is_empty();
    let end = l.toks.len() - 1; // Eof
    l.stmts(0, end);
    l.finish()
}

/// cig's lexer has no token for a lone `?` (as in `Int?`): mask each one
/// it reports with a space, remembering where it was. Strings and comments
/// are cig's to judge, so a `?` inside one is never touched.
///
/// A raw identifier (`r#match`, #22 L9) is not cig's either: `#` starts a
/// comment there. It is rewritten to the same length before cig reads it, so
/// every position stays exact: a variable `r#match` becomes `r_match`, and
/// the word after a `.` or in front of a key's `:` becomes the bare word
/// (spaces where `r#` was), since a field or a key is never reserved.
#[allow(clippy::result_large_err)]
pub fn mask(src: &str) -> Result<(String, Vec<Token>, BTreeSet<usize>), Diagnostic> {
    mask_raw(src).map(|(w, t, o, _)| (w, t, o))
}

/// What `mask_raw` reads: the masked text, its tokens, where each `?` was,
/// and where each raw identifier (`r#word`) started in the source.
pub type Masked = (String, Vec<Token>, BTreeSet<usize>, Vec<usize>);

/// `mask`, and where each raw identifier (`r#word`) started in `src`.
#[allow(clippy::result_large_err)]
pub fn mask_raw(src: &str) -> Result<Masked, Diagnostic> {
    let mut work = src.to_string();
    let mut opt = BTreeSet::new();
    let mut raw = Vec::new();
    for _ in 0..100_000 {
        match tokenize(&work) {
            Ok(t) => match raw_identifiers(&work, &t) {
                Some((next, at)) => {
                    raw.extend(at);
                    work = next;
                }
                None => {
                    raw.sort_unstable();
                    return Ok((work, split_shifts(t), opt, raw));
                }
            },
            Err(d) => {
                let at = d.span.map(|s| s.start);
                match at {
                    Some(p) if work.as_bytes().get(p) == Some(&b'?') => {
                        opt.insert(p);
                        work.replace_range(p..p + 1, " ");
                    }
                    _ => return Err(d),
                }
            }
        }
    }
    Err(Diagnostic::new(DK::Syntax, "too many `?` to read"))
}

/// Rewrite the raw identifiers cig's lexer has just read as a name `r` and a
/// comment. `None` when there are none.
fn raw_identifiers(work: &str, tokens: &[Token]) -> Option<(String, Vec<usize>)> {
    let bytes = work.as_bytes();
    let start_of = |b: u8| b.is_ascii_alphabetic() || b == b'_';
    let mut out: Option<Vec<u8>> = None;
    let mut at = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        if !matches!(&t.kind, T::Ident(n) if n == "r") || t.span.end != t.span.start + 1 {
            continue;
        }
        let hash = t.span.end;
        if bytes.get(hash) != Some(&b'#') || !bytes.get(hash + 1).is_some_and(|b| start_of(*b)) {
            continue;
        }
        let mut end = hash + 1;
        while bytes
            .get(end)
            .is_some_and(|b| b.is_ascii_alphanumeric() || *b == b'_')
        {
            end += 1;
        }
        let after_dot = i > 0 && matches!(tokens[i - 1].kind, T::Dot | T::SafeDot);
        let rest = work[end..].trim_start_matches([' ', '\t']);
        let key = rest.starts_with(':')
            && i > 0
            && matches!(tokens[i - 1].kind, T::LBrace | T::Comma | T::Newline);
        let buf = out.get_or_insert_with(|| bytes.to_vec());
        if after_dot || key {
            buf[t.span.start] = b' ';
            buf[hash] = b' ';
        } else {
            buf[hash] = b'_';
        }
        at.push(t.span.start);
    }
    out.map(|b| {
        (
            String::from_utf8(b).expect("only ASCII bytes were replaced"),
            at,
        )
    })
}

/// `List<Map<Int>>` ends in `>>`, which cig reads as its pipe: in the
/// tokens lowering reads (never in the text it writes), it is two `>`.
fn split_shifts(toks: Vec<Token>) -> Vec<Token> {
    let mut out = Vec::with_capacity(toks.len());
    for t in toks {
        if t.kind == T::Pipe && t.span.end == t.span.start + 2 {
            let mut a = t.clone();
            a.kind = T::Gt;
            a.span.end = a.span.start + 1;
            let mut b = t;
            b.kind = T::Gt;
            b.span.start += 1;
            b.span.col += 1;
            out.push(a);
            out.push(b);
        } else {
            out.push(t);
        }
    }
    out
}

impl<'a> L<'a> {
    // ----- tokens --------------------------------------------------------------------

    fn k(&self, i: usize) -> &T {
        &self.toks[i.min(self.toks.len() - 1)].kind
    }
    fn sp(&self, i: usize) -> Span {
        self.toks[i.min(self.toks.len() - 1)].span
    }
    fn is_word(&self, i: usize, w: &str) -> bool {
        matches!(self.k(i), T::Ident(s) if s == w)
    }
    fn ident(&self, i: usize) -> Option<String> {
        match self.k(i) {
            T::Ident(s) => Some(s.clone()),
            _ => None,
        }
    }
    /// The index of the bracket that closes the one at `i`.
    fn matching(&self, i: usize) -> usize {
        let (open, close) = match self.k(i) {
            T::LParen => (T::LParen, T::RParen),
            T::LBracket => (T::LBracket, T::RBracket),
            _ => (T::LBrace, T::RBrace),
        };
        let mut depth = 0i32;
        let mut j = i;
        while j < self.toks.len() - 1 {
            let k = self.k(j);
            if *k == open {
                depth += 1;
            } else if *k == close {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            j += 1;
        }
        self.toks.len() - 1
    }
    fn is_open(k: &T) -> bool {
        matches!(k, T::LParen | T::LBracket | T::LBrace)
    }
    fn is_close(k: &T) -> bool {
        matches!(k, T::RParen | T::RBracket | T::RBrace)
    }
    /// The end of the statement starting at `i`: the first newline or `;`
    /// at depth 0, or `limit`.
    fn stmt_end(&self, i: usize, limit: usize) -> usize {
        let mut depth = 0i32;
        let mut j = i;
        while j < limit {
            let k = self.k(j);
            if Self::is_open(k) {
                depth += 1;
            } else if Self::is_close(k) {
                if depth == 0 {
                    return j;
                }
                depth -= 1;
            } else if depth == 0 && matches!(k, T::Newline | T::Semicolon) {
                // `else`, `ashtray`, `finally` and `unburn` may continue a
                // statement on the next line.
                let mut n = j;
                while matches!(self.k(n), T::Newline) {
                    n += 1;
                }
                if matches!(self.k(j), T::Newline)
                    && (matches!(self.k(n), T::Else | T::Ashtray | T::Finally)
                        || self.is_word(n, "unburn"))
                {
                    j = n;
                    continue;
                }
                return j;
            }
            j += 1;
        }
        limit
    }
    /// Can the token at `i` end an expression (so a `{` after it opens a
    /// block, not a map)?
    fn ends_expr(&self, i: usize) -> bool {
        matches!(
            self.k(i),
            T::Ident(_)
                | T::Int(_)
                | T::IntMinMagnitude
                | T::Float(_)
                | T::Str(_)
                | T::True
                | T::False
                | T::Null
                | T::RParen
                | T::RBracket
                | T::RBrace
        )
    }
    /// The first `{` at depth 0 in `[i, limit)` that opens a block.
    fn block_brace(&self, i: usize, limit: usize) -> Option<usize> {
        let mut depth = 0i32;
        let mut j = i;
        while j < limit {
            let k = self.k(j);
            if *k == T::LBrace && depth == 0 && j > i && self.ends_expr(j - 1) {
                return Some(j);
            }
            if *k == T::LBrace && depth == 0 && j == i {
                return Some(j);
            }
            if Self::is_open(k) {
                depth += 1;
            } else if Self::is_close(k) {
                depth -= 1;
            }
            j += 1;
        }
        None
    }

    // ----- diagnostics ---------------------------------------------------------------

    fn span_of(&self, start: usize, end: usize) -> Span {
        let line = self.lines.partition_point(|&s| s <= start);
        let col = start - self.lines[line - 1] + 1;
        Span::new(start, end.max(start), line as u32, col as u32)
    }
    fn err(&mut self, code: &'static str, msg: impl Into<String>, start: usize, end: usize) {
        let mut d = Diagnostic::new(DK::Check, msg.into()).at(self.span_of(start, end));
        d.code = code;
        self.diags.push(d);
    }
    fn err_hint(
        &mut self,
        code: &'static str,
        msg: impl Into<String>,
        start: usize,
        end: usize,
        hint: impl Into<String>,
    ) {
        let mut d = Diagnostic::new(DK::Check, msg.into())
            .at(self.span_of(start, end))
            .with_hint(hint);
        d.code = code;
        self.diags.push(d);
    }
    /// A type error from `ty::parse` ("E1202 ..."), at token `i`.
    fn ty_err(&mut self, i: usize, msg: String) {
        let (code, text) = split_code(&msg);
        let sp = self.sp(i);
        self.err(code, text, sp.start, sp.end);
    }

    fn parse_ty(&mut self, i: usize) -> Option<(Ty, usize)> {
        let names = Names(&self.record_names, &self.enum_names);
        match ty::parse(&self.toks, i, &self.work, &self.opt, &self.tparams, &names) {
            Ok((t, n)) => {
                // Sized numbers, bytes and channels are full's.
                if self.strength < Strength::Full {
                    if let Some(what) = full_only(&t) {
                        let (s, e) = (self.sp(i).start, self.ty_end(n));
                        self.err_hint(
                            "E1101",
                            format!(
                                "using {what} needs strength full; this file is {}",
                                self.strength
                            ),
                            s,
                            e,
                            "to use it, put `strength full` at the top of the file",
                        );
                    }
                }
                Some((t, n))
            }
            Err((at, m)) => {
                if let Some(module) = self
                    .ident(at)
                    .and_then(|n| self.private_types.get(&n).cloned())
                {
                    let n = self.ident(at).unwrap_or_default();
                    let sp = self.sp(at);
                    self.err_hint(
                        "E1303",
                        format!("`{n}` is in module `{module}` but not exported"),
                        sp.start,
                        sp.end,
                        format!("mark it `pub` in module `{module}`"),
                    );
                    return None;
                }
                let m = if m.starts_with('E') {
                    m
                } else {
                    format!("E1202 expected {m} here")
                };
                self.ty_err(at, m);
                None
            }
        }
    }

    /// The byte offset where a type that ends before token `n` ends: after
    /// its last token, and after any `?` written right after it.
    fn ty_end(&self, n: usize) -> usize {
        let mut at = self.sp(n - 1).end;
        while let Some(&q) = self.opt.range(at..).next() {
            if self.work[at..q].chars().all(|c| c == ' ' || c == '\t') {
                at = q + 1;
            } else {
                break;
            }
        }
        at
    }

    fn fresh(&mut self) -> usize {
        self.n += 1;
        self.n
    }

    // ----- names ---------------------------------------------------------------------

    /// `_cg_` and `__cg_` belong to the lowering.
    fn reserved_names(&mut self) {
        let mut bad = Vec::new();
        for t in &self.toks {
            if let T::Ident(n) = &t.kind {
                if n.starts_with("_cg_") || n.starts_with("__cg_") {
                    bad.push((n.clone(), t.span));
                }
            }
        }
        for (n, sp) in bad {
            self.err_hint(
                "E1104",
                format!("`{n}` starts with a prefix CigarScript keeps for itself"),
                sp.start,
                sp.end,
                "rename it: names starting `_cg_` or `__cg_` are the lowering's",
            );
        }
    }

    /// Is token `i` at the start of a statement?
    fn at_stmt_start(&self, i: usize) -> bool {
        i == 0
            || matches!(
                self.k(i - 1),
                T::Newline | T::Semicolon | T::LBrace | T::RBrace
            )
    }

    /// Read every record, enum and interface first, so a use before the
    /// declaration (in a function above it) still knows the name.
    fn declarations(&mut self) {
        let len = self.toks.len();
        // Names first, so field types can name each other.
        for i in 0..len.saturating_sub(1) {
            let start = if self.is_word(i, "pub") { i + 1 } else { i };
            if start != i + 1 && !self.at_stmt_start(i) {
                continue;
            }
            if start == i + 1 && !self.at_stmt_start(i) {
                continue;
            }
            if let Some(n) = self.ident(start + 1) {
                if self.is_word(start, "record") {
                    self.record_names.insert(n);
                } else if self.is_word(start, "enum") {
                    self.enum_names.insert(n);
                }
            }
        }
        self.user_result = self.enum_names.contains("Result");
        self.user_attempt = (0..len.saturating_sub(2))
            .any(|i| matches!(self.k(i), T::Pull) && self.is_word(i + 1, "attempt"));
        if !self.user_result && self.wants_result() {
            self.builtin_result = true;
            self.enum_names.insert("Result".to_string());
            self.prog.enums.insert("Result".to_string(), result_decl());
        }
        // Then the full declarations.
        let mut i = 0;
        while i < len.saturating_sub(1) {
            let public = self.is_word(i, "pub") && self.at_stmt_start(i);
            let s = if public { i + 1 } else { i };
            if (public || self.at_stmt_start(i)) && self.ident(s + 1).is_some() {
                if self.is_word(s, "record") {
                    if let Some(r) = self.read_record(s, public) {
                        if !self.declared_twice(&r.name, s + 1) {
                            self.prog.records.insert(r.name.clone(), r);
                        }
                    }
                } else if self.is_word(s, "enum") {
                    if let Some(e) = self.read_enum(s, public) {
                        if !self.declared_twice(&e.name, s + 1) {
                            self.prog.enums.insert(e.name.clone(), e);
                        }
                    }
                } else if self.is_word(s, "interface") {
                    if let Some(f) = self.read_interface(s, public) {
                        if !self.declared_twice(&f.name, s + 1) {
                            self.prog.interfaces.insert(f.name.clone(), f);
                        }
                    }
                } else if self.is_word(s, "layer") && self.strength == Strength::Full {
                    if let Some(n) = self.ident(s + 1) {
                        self.layers.insert(n);
                    }
                }
            }
            if self.is_word(i, "pull") || matches!(self.k(i), T::Pull) {
                // `pull name<T>(`: a generic function, whose type arguments
                // a call may write.
                if matches!(self.k(i + 2), T::Lt) {
                    if let Some(n) = self.ident(i + 1) {
                        self.generic_fns.insert(n);
                    }
                }
            }
            i += 1;
        }
        // The prelude's standard interfaces this file names (#38).
        self.standard_interfaces();
        // What `derive` writes (#38).
        self.derive_impls();
    }

    /// Does the file write about a `Result`: the word, `Ok(`, `Err(`,
    /// `attempt(`, or a `?` straight after a `)` or `]` (a call's `?`)?
    fn wants_result(&self) -> bool {
        // `derive(Json)` gives `from_json`, whose value is a `Result`.
        let named =
            |n: &str| n == "Result" || n == "Ok" || n == "Err" || n == "attempt" || n == "Json";
        let words = self.toks.iter().enumerate().any(|(i, t)| {
            matches!(&t.kind, T::Ident(n) if named(n))
                && !matches!(
                    self.toks[i.saturating_sub(1)].kind,
                    T::Dot | T::SafeDot | T::Pull
                )
        });
        words
            || self
                .opt
                .iter()
                .any(|&q| q > 0 && matches!(self.work.as_bytes()[q - 1], b')' | b']'))
    }

    /// A record, enum or interface named like one declared above it: E1214,
    /// naming the line of the first.
    fn declared_twice(&mut self, name: &str, at: usize) -> bool {
        let first = self
            .prog
            .records
            .get(name)
            .map(|r| r.at)
            .or_else(|| self.prog.enums.get(name).map(|e| e.at))
            .or_else(|| self.prog.interfaces.get(name).map(|i| i.at));
        let Some(first) = first else { return false };
        let line = self.lines.partition_point(|&s| s <= first.start);
        let sp = self.sp(at);
        self.err_hint(
            "E1214",
            format!("`{name}` is declared twice: a record, enum or interface name is one thing in a file"),
            sp.start,
            sp.end,
            format!("the first is on line {line}; rename one of them"),
        );
        true
    }

    /// `<A, B>` after a declaration's name: the names, and the index after.
    fn generic_params(&mut self, i: usize) -> (Vec<(String, Vec<String>)>, usize) {
        let mut out = Vec::new();
        if !matches!(self.k(i), T::Lt) {
            return (out, i);
        }
        let mut j = i + 1;
        while let Some(n) = self.ident(j) {
            j += 1;
            let mut bounds = Vec::new();
            if matches!(self.k(j), T::Colon) {
                // `T: A + B` (#38): see `bounds.rs`.
                (bounds, j) = self.bound_list(j + 1);
            }
            out.push((n, bounds));
            if matches!(self.k(j), T::Comma) {
                j += 1;
                continue;
            }
            break;
        }
        if matches!(self.k(j), T::Gt) {
            j += 1;
        } else {
            let sp = self.sp(j);
            self.err(
                "E1202",
                "a type parameter list closes with `>`",
                sp.start,
                sp.end,
            );
        }
        (out, j)
    }

    fn at_of(&self, i: usize, j: usize) -> At {
        At {
            start: self.sp(i).start,
            end: self.sp(j).end,
        }
    }

    /// `record Name<A> { field: Type, ... }` at token `s` (`record`).
    fn read_record(&mut self, s: usize, public: bool) -> Option<RecordDecl> {
        let name = self.ident(s + 1)?;
        let (gens, j) = self.generic_params(s + 2);
        self.no_bounds_on("record", &gens, s + 1);
        let (derives, mut j) = self.derive_list(j);
        if !matches!(self.k(j), T::LBrace) {
            let sp = self.sp(j);
            self.err(
                "E1202",
                format!("record `{name}` needs its fields in `{{ }}`"),
                sp.start,
                sp.end,
            );
            return None;
        }
        let close = self.matching(j);
        let saved = std::mem::replace(
            &mut self.tparams,
            gens.iter().map(|g| g.0.clone()).collect(),
        );
        let mut fields: Vec<(String, Ty)> = Vec::new();
        let mut defaults = Vec::new();
        j += 1;
        while j < close {
            if matches!(self.k(j), T::Newline | T::Comma) {
                j += 1;
                continue;
            }
            let Some(f) = self.ident(j) else {
                let sp = self.sp(j);
                self.err("E1202", "a field is `name: Type`", sp.start, sp.end);
                break;
            };
            if !matches!(self.k(j + 1), T::Colon) {
                let sp = self.sp(j);
                self.err(
                    "E1202",
                    format!("field `{f}` needs a type: `{f}: Type`"),
                    sp.start,
                    sp.end,
                );
                break;
            }
            match self.parse_ty(j + 2) {
                Some((t, n)) => {
                    if fields.iter().any(|(x, _)| *x == f) {
                        let sp = self.sp(j);
                        self.err(
                            "E1214",
                            format!("record `{name}` names field `{f}` twice"),
                            sp.start,
                            sp.end,
                        );
                    }
                    // `retries: Int = 3`: a field default (#38).
                    let n = if matches!(self.k(n), T::Assign) {
                        let (d, next) = self.field_default(&f, n + 1, close);
                        defaults.extend(d);
                        next
                    } else {
                        n
                    };
                    fields.push((f, t));
                    j = n;
                }
                None => break,
            }
        }
        self.tparams = saved;
        Some(RecordDecl {
            name,
            generics: gens.into_iter().map(|g| g.0).collect(),
            fields,
            derives,
            defaults,
            public,
            at: self.at_of(s, close),
        })
    }

    /// `enum Name<T> { a, b(x: T), c(Int, Text) }` at token `s` (`enum`).
    fn read_enum(&mut self, s: usize, public: bool) -> Option<EnumDecl> {
        let name = self.ident(s + 1)?;
        let (gens, j) = self.generic_params(s + 2);
        self.no_bounds_on("enum", &gens, s + 1);
        let (derives, mut j) = self.derive_list(j);
        if !matches!(self.k(j), T::LBrace) {
            let sp = self.sp(j);
            self.err(
                "E1202",
                format!("enum `{name}` needs its variants in `{{ }}`"),
                sp.start,
                sp.end,
            );
            return None;
        }
        let close = self.matching(j);
        let saved = std::mem::replace(
            &mut self.tparams,
            gens.iter().map(|g| g.0.clone()).collect(),
        );
        let mut variants: Vec<Variant> = Vec::new();
        j += 1;
        while j < close {
            if matches!(self.k(j), T::Newline | T::Comma) {
                j += 1;
                continue;
            }
            let Some(v) = self.ident(j) else {
                let sp = self.sp(j);
                self.err(
                    "E1202",
                    "a variant is a name, or `name(field: Type, ...)`",
                    sp.start,
                    sp.end,
                );
                break;
            };
            let vstart = j;
            let mut fields = Vec::new();
            j += 1;
            if matches!(self.k(j), T::LParen) {
                let pc = self.matching(j);
                j += 1;
                let mut pos = 0;
                while j < pc {
                    if matches!(self.k(j), T::Comma | T::Newline) {
                        j += 1;
                        continue;
                    }
                    let fname = if self.ident(j).is_some() && matches!(self.k(j + 1), T::Colon) {
                        let f = self.ident(j).unwrap();
                        j += 2;
                        f
                    } else {
                        format!("_{pos}")
                    };
                    pos += 1;
                    match self.parse_ty(j) {
                        Some((t, n)) => {
                            fields.push((fname, t));
                            j = n;
                        }
                        None => {
                            j = pc;
                        }
                    }
                }
                j = pc + 1;
            }
            if variants.iter().any(|x| x.name == v) {
                let sp = self.sp(vstart);
                self.err(
                    "E1214",
                    format!("enum `{name}` names variant `{v}` twice"),
                    sp.start,
                    sp.end,
                );
            }
            variants.push(Variant {
                name: v,
                fields,
                at: self.at_of(vstart, j.saturating_sub(1)),
            });
        }
        self.tparams = saved;
        Some(EnumDecl {
            name,
            generics: gens.into_iter().map(|g| g.0).collect(),
            variants,
            derives,
            public,
            at: self.at_of(s, close),
        })
    }

    /// `interface Name { pull m(self, x: T) -> R ... }`.
    fn read_interface(&mut self, s: usize, public: bool) -> Option<InterfaceDecl> {
        let name = self.ident(s + 1)?;
        let j = s + 2;
        if !matches!(self.k(j), T::LBrace) {
            return None;
        }
        let close = self.matching(j);
        let mut methods = Vec::new();
        let mut defaults = Vec::new();
        let mut k = j + 1;
        // `Self` names the implementing type in a signature (#38).
        let saved = self.tparams.clone();
        self.tparams.push(impls::SELF.to_string());
        while k < close {
            if matches!(self.k(k), T::Pull) {
                if let Some((sig, next)) = self.read_sig(k, false) {
                    k = next;
                    // A body: the method's default.
                    if matches!(self.k(k), T::LBrace) {
                        defaults.push(sig.name.clone());
                        k = self.matching(k) + 1;
                    }
                    methods.push(sig);
                    continue;
                }
            }
            k += 1;
        }
        self.tparams = saved;
        Some(InterfaceDecl {
            name,
            methods,
            defaults,
            public,
            at: self.at_of(s, close),
        })
    }

    /// A `pull` signature at token `p` (`pull`): returns it and the index
    /// after it (at the body's `{`, or the end of the line in an interface).
    /// When `edit` is set, its annotations are blanked.
    fn read_sig(&mut self, p: usize, edit: bool) -> Option<(FnSig, usize)> {
        let name = self.ident(p + 1)?;
        let (mut gens, mut j) = self.generic_params(p + 2);
        if edit && j > p + 2 {
            self.e
                .blank(self.src, self.sp(p + 2).start, self.sp(j - 1).end);
        }
        let saved = self.tparams.clone();
        self.tparams.extend(gens.iter().map(|g| g.0.clone()));
        if !matches!(self.k(j), T::LParen) {
            self.tparams = saved;
            return None;
        }
        let pc = self.matching(j);
        let mut params = Vec::new();
        j += 1;
        while j < pc {
            if matches!(self.k(j), T::Comma | T::Newline) {
                j += 1;
                continue;
            }
            let mut mode = Mode::Shared;
            if self.is_word(j, "owned") || self.is_word(j, "borrow") {
                if self.strength != Strength::Full {
                    // The gate reports it; keep reading.
                }
                mode = if self.is_word(j, "owned") {
                    Mode::Owned
                } else {
                    Mode::Borrow
                };
                if edit {
                    self.e.blank(self.src, self.sp(j).start, self.sp(j).end);
                }
                j += 1;
            }
            let Some(pname) = self.ident(j) else {
                j += 1;
                continue;
            };
            j += 1;
            let mut t = None;
            if matches!(self.k(j), T::Colon) {
                let colon = j;
                if let Some((ty, n)) = self.parse_ty(j + 1) {
                    if edit {
                        let end = self.ty_end(n);
                        self.e.blank(self.src, self.sp(colon).start, end);
                    }
                    t = Some(ty);
                    j = n;
                } else {
                    j = pc;
                }
            }
            params.push(Param {
                name: pname,
                ty: t,
                mode,
            });
        }
        j = pc + 1;
        let mut ret = None;
        if matches!(self.k(j), T::Minus) && matches!(self.k(j + 1), T::Gt) {
            let arrow = j;
            if let Some((t, n)) = self.parse_ty(j + 2) {
                if edit {
                    let end = self.ty_end(n);
                    self.e.blank(self.src, self.sp(arrow).start, end);
                }
                ret = Some(t);
                j = n;
            }
        }
        let mut coughs = Vec::new();
        if self.is_word(j, "coughs") {
            let kw = j;
            j += 1;
            let mut last = self.sp(kw).end;
            while let Some((t, n)) = self.parse_ty(j) {
                coughs.push(t);
                last = self.ty_end(n);
                j = n;
                if matches!(self.k(j), T::Comma) {
                    j += 1;
                } else {
                    break;
                }
            }
            if edit {
                self.e.blank(self.src, self.sp(kw).start, last);
            }
        }
        j = self.where_clause(j, edit, &mut gens);
        self.tparams = saved;
        let sig = FnSig {
            name,
            generics: gens,
            params,
            ret,
            coughs,
            public: false,
            at: self.at_of(p, pc),
        };
        Some((sig, j))
    }

    // ----- statements ----------------------------------------------------------------

    fn stmts(&mut self, mut i: usize, end: usize) {
        while i < end {
            if matches!(self.k(i), T::Newline | T::Semicolon) {
                i += 1;
                continue;
            }
            let next = self.stmt(i, end);
            i = next.max(i + 1);
        }
    }

    /// Process the block whose `{` is at `open`; return the index after its `}`.
    fn block(&mut self, open: usize) -> usize {
        let close = self.matching(open);
        self.stmts(open + 1, close);
        close + 1
    }

    fn stmt(&mut self, i: usize, end: usize) -> usize {
        let public = self.is_word(i, "pub");
        if public {
            let sp = self.sp(i);
            self.e.blank(self.src, sp.start, sp.end);
            self.public_next = true;
            return self.stmt(i + 1, end);
        }
        let public = std::mem::take(&mut self.public_next);
        if let Some(w) = self.ident(i) {
            let next_is_name = self.ident(i + 1).is_some();
            match w.as_str() {
                "record" if next_is_name => return self.record_stmt(i, end),
                "enum" if next_is_name => return self.enum_stmt(i, end),
                "interface" if next_is_name => return self.interface_stmt(i),
                "impl" if next_is_name || matches!(self.k(i + 1), T::Lt) => {
                    return self.impl_stmt(i, end)
                }
                "use" if next_is_name => return self.use_stmt(i, end),
                "layer" if next_is_name && self.strength == Strength::Full => {
                    let e = self.stmt_end(i, end);
                    self.e.blank(self.src, self.sp(i).start, self.sp(e - 1).end);
                    return e;
                }
                "match" => return self.match_at(i, end, Target::Stmt),
                "parallel" if matches!(self.k(i + 1), T::LParen) => {
                    let pc = self.matching(i + 1);
                    if matches!(self.k(pc + 1), T::For) {
                        self.e.blank(self.src, self.sp(i).start, self.sp(pc).end);
                        self.parallel_keys.push(self.sp(pc + 1).start);
                        return self.stmt(pc + 1, end);
                    }
                }
                "within" => return self.within_stmt(i, end),
                l if self.layers.contains(l) && matches!(self.k(i + 1), T::For) => {
                    return self.layer_stmt(i, end)
                }
                _ => {}
            }
        }
        match self.k(i).clone() {
            T::Stick | T::Roll => self.binding(i, end),
            T::Pull => {
                self.public_next = public;
                self.pull_stmt(i, end, None)
            }
            T::If => {
                let e = self.stmt_end(i, end);
                self.headed(i + 1, e)
            }
            T::While => {
                let e = self.stmt_end(i, end);
                self.deadline_tick(i + 1, e);
                self.headed(i + 1, e)
            }
            T::Else => {
                let e = self.stmt_end(i, end);
                self.headed(i + 1, e)
            }
            T::For => {
                let e = self.stmt_end(i, end);
                self.deadline_tick(i + 1, e);
                self.headed(i + 1, e)
            }
            T::Burn | T::Unlit | T::Try | T::Ashtray | T::Finally => {
                let e = self.stmt_end(i, end);
                self.headed(i + 1, e)
            }
            T::Exhale => {
                let e = self.stmt_end(i, end);
                self.exhale(i + 1, e);
                e
            }
            T::Snuff => {
                let e = self.stmt_end(i, end);
                if self.is_word(i + 1, "match") {
                    let target = Target::Value {
                        stmt_start: i,
                        then: "snuff ".into(),
                        ty: self.rets.last().cloned().flatten().map(|r| r.0),
                        name: None,
                    };
                    return self.match_at(i + 1, end, target);
                }
                if i + 1 < e {
                    if let Some(Some((t, f))) = self.rets.last().cloned() {
                        if ty::checks(&t) {
                            // `snuff e` becomes `stick __cg_vN = e; <guard>;
                            // snuff __cg_vN`: the result is checked where it
                            // is returned.
                            let n = self.fresh();
                            let v = format!("__cg_v{n}");
                            let sp = self.sp(i);
                            self.e.replace(sp.start, sp.end, format!("stick {v} ="));
                            self.e.close(
                                self.sp(e - 1).end,
                                format!("; {}; snuff {v}", guard(&v, &t, &format!("{f} returns"))),
                            );
                        }
                    }
                    self.expr(i + 1, e);
                }
                e
            }
            T::Cough | T::Chain => {
                let e = self.stmt_end(i, end);
                // A record or enum coughed is shown as one when nothing
                // catches it, and caught as itself.
                if self.cough_is_err() && matches!(self.k(i), T::Cough) && i + 1 < e {
                    // In a function that returns a `Result`, a cough hands
                    // the value back as its `Err`.
                    let sp = self.sp(i);
                    self.e.replace(sp.start, sp.end, "snuff _cg_cough_Err(");
                    self.e.close(self.sp(e - 1).end, ")");
                } else if self.show && matches!(self.k(i), T::Cough) && i + 1 < e {
                    self.e.open(self.sp(i + 1).start, "_cg_coughed(");
                    self.e.close(self.sp(e - 1).end, ")");
                }
                self.expr(i + 1, e);
                e
            }
            _ => {
                let e = self.stmt_end(i, end);
                // `a into b`
                let mut depth = 0i32;
                for j in i..e {
                    let k = self.k(j).clone();
                    if Self::is_open(&k) {
                        depth += 1;
                    } else if Self::is_close(&k) {
                        depth -= 1;
                    } else if depth == 0 && self.is_word(j, "into") {
                        self.e.open(self.sp(i).start, "_cg_layer_into(");
                        self.e.replace(self.sp(j).start, self.sp(j).end, ",");
                        self.e.close(self.sp(e - 1).end, ")");
                        self.expr(i, j);
                        self.expr(j + 1, e);
                        return e;
                    } else if depth == 0 && matches!(k, T::Assign) && self.is_word(j + 1, "match") {
                        let lhs = self.src[self.sp(i).start..self.sp(j).start]
                            .trim()
                            .to_string();
                        let target = Target::Value {
                            stmt_start: i,
                            then: format!("{lhs} = "),
                            ty: None,
                            name: None,
                        };
                        return self.match_at(j + 1, end, target);
                    }
                }
                self.expr(i, e);
                e
            }
        }
    }

    /// Is a `cough` here the `Err` of a function that returns the built-in
    /// `Result`? Not inside a `try` of the same function: that cough is the
    /// `try`'s to catch.
    fn cough_is_err(&self) -> bool {
        self.builtin_result
            && self.try_depth == 0
            && self.try_fns.last().is_some_and(|f| f.pull)
            && matches!(
                self.rets.last(),
                Some(Some((Ty::Enum(n, _), _))) if n == "Result"
            )
    }

    /// A statement with a header and blocks (`if`, `while`, `for`, `burn`,
    /// `try`, `else`...): the header's expressions, then every block.
    fn headed(&mut self, mut i: usize, e: usize) -> usize {
        while i < e {
            match self.k(i) {
                T::LBrace
                    if self.ends_expr(i.saturating_sub(1))
                        || matches!(
                            self.k(i.saturating_sub(1)),
                            T::Burn | T::Try | T::Finally | T::Else | T::Ashtray | T::Unlit
                        )
                        || self.is_word(i.saturating_sub(1), "unburn") =>
                {
                    // The body of a `try`: a `?` in it is caught by the
                    // `ashtray`, which hands it on.
                    let in_try = matches!(self.k(i.saturating_sub(1)), T::Try);
                    self.try_depth += usize::from(in_try);
                    i = self.block(i);
                    self.try_depth -= usize::from(in_try);
                }
                T::Ashtray
                    if self.show
                        && self.ident(i + 1).is_some()
                        && matches!(self.k(i + 2), T::LBrace) =>
                {
                    // `ashtray e {`: `e` is what was coughed, a record as itself.
                    let name = self.ident(i + 1).unwrap();
                    let n = self.fresh();
                    let sp = self.sp(i + 1);
                    self.e.replace(sp.start, sp.end, format!("__cg_c{n}"));
                    self.e.open(
                        self.sp(i + 2).end,
                        format!(" _cg_reraise(__cg_c{n}); stick {name} = _cg_caught(__cg_c{n});"),
                    );
                    i += 2;
                }
                T::If | T::While | T::For | T::Else | T::Ashtray | T::Finally | T::Burn => {
                    i += 1;
                }
                _ => {
                    // An expression up to the next block or the end.
                    let stop = self.block_brace(i, e).unwrap_or(e);
                    let stop = stop.max(i + 1);
                    self.expr(i, stop);
                    i = stop;
                }
            }
        }
        e
    }

    /// `stick x: T = e` / `roll x: T = e`.
    fn binding(&mut self, i: usize, end: usize) -> usize {
        let e = self.stmt_end(i, end);
        let Some(name) = self.ident(i + 1) else {
            self.expr(i + 1, e);
            return e;
        };
        let mut j = i + 2;
        let mut declared: Option<Ty> = None;
        let mut annotation = None;
        if matches!(self.k(j), T::Colon) {
            let colon = j;
            if let Some((t, n)) = self.parse_ty(j + 1) {
                annotation = Some((self.sp(colon).start, self.ty_end(n)));
                declared = Some(t);
                j = n;
            }
        }
        if !matches!(self.k(j), T::Assign) {
            if let Some((a, b)) = annotation {
                self.e.blank(self.src, a, b);
            }
            self.expr(j, e);
            return e;
        }
        if self.is_word(j + 1, "match") {
            let kw = if matches!(self.k(i), T::Stick) {
                "stick"
            } else {
                "roll"
            };
            let target = Target::Value {
                stmt_start: i,
                then: format!("{kw} {name} = "),
                ty: declared,
                name: Some(name),
            };
            return self.match_at(j + 1, end, target);
        }
        if let Some((a, b)) = annotation {
            self.e.blank(self.src, a, b);
        }
        // `stick ch = channel<T>()`: `ch` is a `Channel<T>`.
        if declared.is_none() && self.is_word(j + 1, "channel") && matches!(self.k(j + 2), T::Lt) {
            let d = self.close_angle(j + 2, e);
            if d + 3 == e
                && matches!(self.k(d + 1), T::LParen)
                && matches!(self.k(d + 2), T::RParen)
            {
                if let Some((t, _)) = self.parse_ty(j + 3) {
                    self.bind_keys
                        .push((self.sp(i).start, Ty::Channel(Box::new(t))));
                }
            }
        }
        if let Some(t) = &declared {
            self.bind_keys.push((self.sp(i).start, t.clone()));
            if ty::checks(t) && j + 1 < e {
                // Checked on its own line, so a value that does not fit
                // coughs where the binding is written.
                self.e
                    .close(self.sp(e - 1).end, format!("; {}", guard(&name, t, &name)));
            }
        }
        if self.is_word(j + 1, "parallel") {
            if let Some(done) = self.parallel_value(j + 1, e) {
                return done;
            }
        }
        self.expr(j + 1, e);
        e
    }

    /// `stick v = parallel(threads: n) for x in xs collect { ... }`, or
    /// `reduce(op) { ... }` in place of `collect` (#22 S8). A turn's value is
    /// its body's last line; `collect` gives the values back in loop order,
    /// and `reduce` combines them with `+`, `*`, `min` or `max`, whose order
    /// does not change the result, so the value is the sequential one by
    /// construction. Lowered to a function called where it stands:
    /// `(pack() { roll acc = []; for x in xs { acc.push(<last>) }; snuff acc })()`.
    /// An empty loop collects `[]` and reduces to `null`.
    fn parallel_value(&mut self, p: usize, e: usize) -> Option<usize> {
        if !matches!(self.k(p + 1), T::LParen) {
            return None;
        }
        let pc = self.matching(p + 1);
        if !matches!(self.k(pc + 1), T::For) || !matches!(self.k(pc + 3), T::In) {
            return None;
        }
        let iter = pc + 4;
        // The word that ends the iterable at depth 0.
        let mut depth = 0i32;
        let mut k = iter;
        let mut found: Option<(usize, usize, Option<String>)> = None;
        while k < e {
            let kk = self.k(k);
            if Self::is_open(kk) {
                depth += 1;
            } else if Self::is_close(kk) {
                depth -= 1;
            } else if depth == 0 && self.is_word(k, "collect") && matches!(self.k(k + 1), T::LBrace)
            {
                found = Some((k, k + 1, None));
                break;
            } else if depth == 0 && self.is_word(k, "reduce") && matches!(self.k(k + 1), T::LParen)
            {
                let rc = self.matching(k + 1);
                if matches!(self.k(rc + 1), T::LBrace) {
                    let op = match (rc == k + 3, self.k(k + 2)) {
                        (true, T::Plus) => "+".to_string(),
                        (true, T::Star) => "*".to_string(),
                        (true, _) if self.is_word(k + 2, "min") => "min".to_string(),
                        (true, _) if self.is_word(k + 2, "max") => "max".to_string(),
                        _ => {
                            self.err_hint(
                                "E1522",
                                "`reduce` takes `+`, `*`, `min` or `max`",
                                self.sp(k).start,
                                self.sp(rc).end,
                                "the turns finish in any order, so only an operator whose order does not change the result can combine them; `collect` keeps every value in loop order",
                            );
                            String::new()
                        }
                    };
                    found = Some((k, rc + 1, Some(op)));
                    break;
                }
            }
            k += 1;
        }
        let (kw, open, op) = found?;
        let close = self.matching(open);
        if close + 1 != e {
            return None;
        }
        // The body's statements; the last one is the turn's value.
        let mut last = None;
        let mut i = open + 1;
        while i < close {
            if matches!(self.k(i), T::Newline | T::Semicolon) {
                i += 1;
                continue;
            }
            let en = self.stmt_end(i, close);
            last = Some((i, en));
            i = en.max(i + 1);
        }
        let what = if op.is_some() { "reduce" } else { "collect" };
        let Some((ls, le)) = last else {
            self.err_hint(
                "E1522",
                format!("a `parallel … {what}` turn needs a value"),
                self.sp(open).start,
                self.sp(close).end,
                "end the body with the turn's value, as its last line",
            );
            return Some(e);
        };
        if matches!(
            self.k(ls),
            T::Stick
                | T::Roll
                | T::If
                | T::For
                | T::While
                | T::Burn
                | T::Unlit
                | T::Try
                | T::Exhale
                | T::Snuff
                | T::Cough
                | T::Break
                | T::Continue
                | T::Pull
        ) {
            self.err_hint(
                "E1522",
                format!("the last line of a `parallel … {what}` turn is its value, and this one is a statement"),
                self.sp(ls).start,
                self.sp(le - 1).end,
                "end the body with an expression: the value this turn gives back",
            );
            return Some(e);
        }
        let n = self.fresh();
        let acc = format!("__cg_p{n}");
        let init = if op.is_some() { "null" } else { "[]" };
        self.e.replace(
            self.sp(p).start,
            self.sp(pc).end,
            format!("(pack() {{ roll {acc} = {init}; "),
        );
        self.parallel_keys.push(self.sp(pc + 1).start);
        self.expr(iter, kw);
        self.e
            .blank(self.src, self.sp(kw).start, self.sp(open - 1).end);
        self.stmts(open + 1, ls);
        match op.as_deref() {
            None => {
                self.e.open(self.sp(ls).start, format!("{acc}.push("));
                self.e.close(self.sp(le - 1).end, ")");
            }
            Some(op) => {
                let v = format!("__cg_v{n}");
                let combine = match op {
                    "*" => format!("{acc} * {v}"),
                    "min" => format!("math.min({acc}, {v})"),
                    "max" => format!("math.max({acc}, {v})"),
                    _ => format!("{acc} + {v}"),
                };
                self.e.open(self.sp(ls).start, format!("stick {v} = "));
                self.e.close(
                    self.sp(le - 1).end,
                    format!("; if {acc} == null {{ {acc} = {v} }} else {{ {acc} = {combine} }}"),
                );
            }
        }
        self.expr(ls, le);
        self.e
            .close(self.sp(close).end, format!("; snuff {acc} }})()"));
        Some(e)
    }

    /// `pull name<T>(a: T, b) -> R coughs E { body }`. `impl_of` names the
    /// interface and type when the function is an `impl` method.
    fn pull_stmt(&mut self, p: usize, end: usize, impl_of: Option<(&str, &Ty)>) -> usize {
        let Some((mut sig, j)) = self.read_sig(p, true) else {
            let e = self.stmt_end(p, end);
            self.expr(p + 1, e);
            return e;
        };
        sig.public = std::mem::take(&mut self.public_next);
        // The name a message shows: `size`, not the lowered impl name.
        let shown = sig.name.clone();
        if let Some((iface, t)) = impl_of {
            // The impl's type parameters are the method's too.
            let mut gens = self.impl_generics.clone();
            gens.append(&mut sig.generics);
            sig.generics = gens;
            let lowered = impl_fn_name(iface, t, &sig.name);
            let sp = self.sp(p + 1);
            self.e.replace(sp.start, sp.end, lowered.clone());
            for prm in sig.params.iter_mut() {
                if prm.name == "self" && prm.ty.is_none() {
                    prm.ty = Some(t.clone());
                }
            }
            sig.name = lowered;
        }
        if !matches!(self.k(j), T::LBrace) {
            let e = self.stmt_end(p, end);
            return e;
        }
        // Typed parameters are checked as the body starts, on the line of
        // the `pull`, so a value that does not fit coughs there.
        let mut checks = String::new();
        for prm in &sig.params {
            if let Some(t) = &prm.ty {
                if ty::checks(t) && prm.name != "self" {
                    let what = format!("{shown}: {}", prm.name);
                    checks.push_str(&format!(" {};", guard(&prm.name, t, &what)));
                }
            }
        }
        if !checks.is_empty() {
            self.e.open(self.sp(j).end, checks);
        }
        let saved = self.tparams.clone();
        self.tparams
            .extend(sig.generics.iter().map(|g| g.0.clone()));
        self.rets.push(sig.ret.clone().map(|r| (r, shown.clone())));
        self.fn_keys.push((self.sp(p).start, sig));
        self.try_fns.push(TryFn {
            pull: true,
            used: false,
        });
        let depth = std::mem::take(&mut self.try_depth);
        let after = self.block(j);
        self.try_depth = depth;
        self.rets.pop();
        self.tparams = saved;
        if self.try_fns.pop().is_some_and(|f| f.used) {
            // A `?` in the body throws the `Err` out through here: the
            // function catches it and gives it back as its result.
            let n = self.fresh();
            self.e.open(self.sp(j).end, " try {");
            self.e.close(
                self.sp(after - 1).start,
                format!(
                    " }} ashtray __cg_p{n} {{ if _cg_is_prop(__cg_p{n}) {{ snuff __cg_p{n}.result }}; cough __cg_p{n} }} "
                ),
            );
        }
        after
    }

    fn record_stmt(&mut self, i: usize, _end: usize) -> usize {
        let name = self.ident(i + 1).unwrap_or_default();
        let (_, j) = {
            let mut k = i + 2;
            if matches!(self.k(k), T::Lt) {
                while k < self.toks.len() - 1 && !matches!(self.k(k), T::Gt) {
                    k += 1;
                }
                k += 1;
            }
            ((), self.past_derive(k))
        };
        let close = if matches!(self.k(j), T::LBrace) {
            self.matching(j)
        } else {
            j
        };
        let start = i;
        self.e
            .blank(self.src, self.sp(start).start, self.sp(close).end);
        if let Some(r) = self.prog.records.get(&name).cloned() {
            let order: Vec<String> = r.fields.iter().map(|(f, _)| format!("\"{f}\"")).collect();
            let descs: Vec<String> = r
                .fields
                .iter()
                .map(|(f, t)| format!("{f}: {}", ty::descriptor(t)))
                .collect();
            // A field left out takes its default (#38).
            let fill: String = r
                .defaults
                .iter()
                .map(|(f, v, _)| format!("if not f.has(\"{f}\") {{ f[\"{f}\"] = {v} }}; "))
                .collect();
            self.appended.push_str(&format!(
                "pull _cg_new_{name}(f) {{ {fill}snuff _cg_record(\"{name}\", [{}], f, {{{}}}) }}\n",
                order.join(", "),
                descs.join(", ")
            ));
        }
        close + 1
    }

    fn enum_stmt(&mut self, i: usize, _end: usize) -> usize {
        let name = self.ident(i + 1).unwrap_or_default();
        let mut j = i + 2;
        if matches!(self.k(j), T::Lt) {
            while j < self.toks.len() - 1 && !matches!(self.k(j), T::Gt) {
                j += 1;
            }
            j += 1;
        }
        let j = self.past_derive(j);
        let close = if matches!(self.k(j), T::LBrace) {
            self.matching(j)
        } else {
            j
        };
        self.e.blank(self.src, self.sp(i).start, self.sp(close).end);
        if let Some(en) = self.prog.enums.get(&name).cloned() {
            for v in &en.variants {
                let params: Vec<String> = v.fields.iter().map(|(f, _)| f.clone()).collect();
                let mut body = format!("{{__enum: \"{name}\", __variant: \"{}\"", v.name);
                for (f, t) in &v.fields {
                    if ty::checks(t) {
                        body.push_str(&format!(
                            ", {f}: _cg_check({f}, {}, \"{name}.{} {f}\")",
                            ty::descriptor(t),
                            v.name
                        ));
                    } else {
                        body.push_str(&format!(", {f}: {f}"));
                    }
                }
                body.push('}');
                self.appended.push_str(&format!(
                    "pull _cg_{name}_{}({}) {{ snuff {body} }}\n",
                    v.name,
                    params.join(", ")
                ));
            }
            // By name: `Shape.rect(h: 4.0, w: 3.0)`.
            for v in en.variants.iter().filter(|v| !v.fields.is_empty()) {
                let order: Vec<String> = v.fields.iter().map(|(f, _)| format!("\"{f}\"")).collect();
                let args: Vec<String> = v.fields.iter().map(|(f, _)| format!("f.{f}")).collect();
                self.appended.push_str(&format!(
                    "pull _cg_named_{name}_{v}(f) {{ _cg_named_ok(\"{name}\", \"{v}\", [{}], f); snuff _cg_{name}_{v}({}) }}\n",
                    order.join(", "),
                    args.join(", "),
                    v = v.name,
                ));
            }
        }
        close + 1
    }

    /// `impl Interface for Type { pull ... }`: the methods stay where they
    /// are, renamed; the header and the closing brace go.
    fn impl_stmt(&mut self, i: usize, end: usize) -> usize {
        // `impl<T: Show> Show for List<T> where ...` (#38): see `impls.rs`.
        let (mut gens, n) = self.impl_params(i);
        let iface = self.ident(n).unwrap_or_default();
        if !matches!(self.k(n + 1), T::For) {
            let sp = self.sp(i);
            self.err(
                "E1311",
                "an implementation is `impl Interface for Type { ... }`",
                sp.start,
                sp.end,
            );
            return self.stmt_end(i, end);
        }
        let saved_tparams = self.tparams.clone();
        self.tparams.extend(gens.iter().map(|g| g.0.clone()));
        let parsed = self.parse_ty(n + 2);
        let Some((t, j)) = parsed else {
            self.tparams = saved_tparams;
            return self.stmt_end(i, end);
        };
        let j = self.where_clause(j, false, &mut gens);
        if !matches!(self.k(j), T::LBrace) {
            self.tparams = saved_tparams;
            return self.stmt_end(i, end);
        }
        let saved_gens = std::mem::replace(&mut self.impl_generics, gens.clone());
        let close = self.matching(j);
        self.e.blank(self.src, self.sp(i).start, self.sp(j).end);
        self.e
            .blank(self.src, self.sp(close).start, self.sp(close).end);
        let mut methods = Vec::new();
        let mut k = j + 1;
        while k < close {
            if matches!(self.k(k), T::Pull) {
                let before = self.fn_keys.len();
                let method = self.ident(k + 1).unwrap_or_default();
                k = self.pull_stmt(k, close, Some((&iface, &t)));
                if let Some((_, s)) = self.fn_keys.get(before) {
                    // The interface's name for it; the lowered name is the
                    // function's.
                    let mut m = s.clone();
                    m.name = method;
                    methods.push(m);
                }
                continue;
            }
            k += 1;
        }
        self.impl_generics = saved_gens;
        self.tparams = saved_tparams;
        self.prog.impls.push(ImplDecl {
            generics: gens,
            interface: iface,
            for_ty: t,
            methods,
            at: self.at_of(i, close),
            builtin: false,
        });
        close + 1
    }

    /// `measure for Path = fs.size`: one line, a layer for a type.
    fn layer_stmt(&mut self, i: usize, end: usize) -> usize {
        let e = self.stmt_end(i, end);
        let layer = self.ident(i).unwrap_or_default();
        let Some((t, j)) = self.parse_ty(i + 2) else {
            return e;
        };
        if !matches!(self.k(j), T::Assign) || j + 1 >= e {
            let sp = self.sp(i);
            self.err(
                "E1402",
                format!("a layer is `{layer} for Type = function`"),
                sp.start,
                sp.end,
            );
            return e;
        }
        let function = self.src[self.sp(j + 1).start..self.sp(e - 1).end]
            .trim()
            .to_string();
        self.e.blank(self.src, self.sp(i).start, self.sp(e - 1).end);
        let fname = impl_fn_name(&layer, &t, &layer);
        let call = if layer == "into" {
            format!("pull {fname}(v, target) {{ snuff ({function})(target, v) }}\n")
        } else {
            format!("pull {fname}(v) {{ snuff ({function})(v) }}\n")
        };
        self.appended.push_str(&call);
        self.prog.layers.push(LayerDecl {
            layer,
            for_ty: t,
            function,
            at: self.at_of(i, e - 1),
        });
        e
    }

    /// `within retry(3, backoff: 2s) { ... }` and `within parallel(...) { ... }`.
    fn within_stmt(&mut self, i: usize, end: usize) -> usize {
        let e = self.stmt_end(i, end);
        let ctx = self.ident(i + 1).unwrap_or_default();
        let Some(open) = self.block_brace(i + 1, e) else {
            self.expr(i + 1, e);
            return e;
        };
        let close = self.matching(open);
        let mut note = WithinNote {
            start: self.sp(i).start,
            head_end: self.sp(open).start,
            end: self.sp(close).end,
            context: ctx.clone(),
            head: self.src[self.sp(i + 1).start..self.sp(open).start]
                .trim()
                .to_string(),
            arg: String::new(),
            ms: 0,
        };
        if matches!(self.k(i + 2), T::LParen) {
            let pc = self.matching(i + 2);
            let first_end = (i + 3..pc)
                .find(|&j| matches!(self.k(j), T::Comma))
                .unwrap_or(pc);
            if i + 3 < first_end {
                note.arg = self.src[self.sp(i + 3).start..self.sp(first_end - 1).end]
                    .trim()
                    .to_string();
            }
            if ctx == "parallel" {
                if let Some(a) = (i + 3..pc).find(|&j| self.is_word(j, "threads")) {
                    note.arg = self.src[self.sp(a + 2).start..self.sp(pc - 1).end]
                        .trim()
                        .to_string();
                }
            }
        }
        match ctx.as_str() {
            "parallel" => {
                self.e
                    .replace(self.sp(i).start, self.sp(open).start, "if true ");
            }
            "timeout" => {
                self.within_timeout(i, open, close, &mut note);
            }
            "retry" => {
                // retry(n) or retry(n, backoff: 2s)
                let args_open = i + 2;
                let mut count = "3".to_string();
                let mut backoff_ms: i64 = 0;
                if matches!(self.k(args_open), T::LParen) {
                    let pc = self.matching(args_open);
                    let mut a = args_open + 1;
                    if a < pc && !self.is_word(a, "backoff") {
                        let mut b = a;
                        while b < pc && !matches!(self.k(b), T::Comma) {
                            b += 1;
                        }
                        count = self.src[self.sp(a).start..self.sp(b - 1).end]
                            .trim()
                            .to_string();
                        a = b;
                    }
                    while a < pc {
                        if self.is_word(a, "backoff") && matches!(self.k(a + 1), T::Colon) {
                            backoff_ms = self.duration_ms(a + 2);
                        }
                        a += 1;
                    }
                }
                // The block runs inside a loop of the lowering's own: a
                // `break` or `continue` there would leave the retry, not the
                // loop the reader sees around it.
                let mut loops: Vec<(usize, usize)> = Vec::new();
                for k in open + 1..close {
                    if matches!(self.k(k), T::For | T::While) {
                        if let Some(b) = self.block_brace(k + 1, close) {
                            loops.push((b, self.matching(b)));
                        }
                    }
                }
                for k in open + 1..close {
                    if matches!(self.k(k), T::Break | T::Continue)
                        && !loops.iter().any(|(a, b)| *a < k && k < *b)
                    {
                        let sp = self.sp(k);
                        let word = if matches!(self.k(k), T::Break) {
                            "break"
                        } else {
                            "continue"
                        };
                        self.err_hint(
                            "E1403",
                            format!("`{word}` cannot leave a `within retry` block"),
                            sp.start,
                            sp.end,
                            "put the loop inside the block, or set a flag and test it after the block",
                        );
                    }
                }
                let n = self.fresh();
                note.ms = backoff_ms;
                // The last failure is coughed again on the `within` line.
                self.e.replace(
                    self.sp(i).start,
                    self.sp(open).start,
                    format!(
                        "roll __cg_t{n} = 0; pull __cg_f{n}(e) {{ cough e }}; while true {{ try "
                    ),
                );
                self.e.replace(
                    self.sp(close).start,
                    self.sp(close).end,
                    format!(
                        "; break }} ashtray __cg_e{n} {{ __cg_t{n} += 1; if __cg_t{n} >= ({count}) {{ __cg_f{n}(__cg_e{n}) }}; time.sleep_ms({backoff_ms}) }} }}"
                    ),
                );
            }
            other => {
                let sp = self.sp(i + 1);
                self.err_hint(
                    "E1102",
                    format!("`within {other}` is not a context this version has: a user-declared context is full-strength CigarScript, not built yet (otmof-ops/cigar-script#13)"),
                    sp.start,
                    sp.end,
                    "`within retry(n)`, `within parallel(...)` and `within timeout(d)` are built",
                );
            }
        }
        self.withins.push(note);
        let is_timeout = ctx == "timeout" && self.deadlines.last().is_some();
        self.stmts(open + 1, close);
        if is_timeout {
            self.deadlines.pop();
        }
        e
    }

    /// `within timeout(2s) { ... }`: a deadline, taken when the block starts,
    /// checked at the top of every `for` and `while` pass in the block and
    /// once more when it ends. Past it is a cough, `E1404`, so the burns in
    /// the block roll back with the run (or are retried by a `within retry`
    /// around it). A single call that never returns is not interrupted.
    fn within_timeout(&mut self, i: usize, open: usize, close: usize, note: &mut WithinNote) {
        let ok = matches!(self.k(i + 2), T::LParen)
            && matches!(self.k(self.matching(i + 2)), T::RParen)
            && self.matching(i + 2) > i + 3;
        if !ok {
            let sp = self.sp(i + 1);
            self.err_hint(
                "E1405",
                "`within timeout` needs a limit: `within timeout(2s) { ... }`".to_string(),
                sp.start,
                self.sp(open).start,
                "a duration (`500ms`, `2s`, `1m`) or a number of milliseconds",
            );
            return;
        }
        let pc = self.matching(i + 2);
        let literal = matches!(self.k(i + 3), T::Int(_))
            && (pc == i + 4 || (pc == i + 5 && self.ident(i + 4).is_some()));
        let limit = if literal {
            note.ms = self.duration_ms(i + 3);
            note.ms.to_string()
        } else {
            format!(
                "({})",
                self.src[self.sp(i + 3).start..self.sp(pc - 1).end].trim()
            )
        };
        let n = self.fresh();
        let name = format!("__cg_d{n}");
        let shown = note.arg.replace('"', "'");
        self.e.replace(
            self.sp(i).start,
            self.sp(open).start,
            format!("roll {name} = time.now_ms() + {limit}; if true "),
        );
        self.e.open(
            self.sp(close).start,
            format!("; {}", self.deadline_check(&name, &shown)),
        );
        self.deadlines.push((name, shown));
    }

    /// The statement that coughs `E1404` once a deadline has passed.
    fn deadline_check(&self, name: &str, shown: &str) -> String {
        format!(
            "if time.now_ms() > {name} {{ cough {{code: \"E1404\", message: \"E1404 `within timeout({shown})` ran out of time\"}} }}"
        )
    }

    /// At the top of a loop's body inside a `within timeout`: the same check.
    fn deadline_tick(&mut self, i: usize, e: usize) {
        if self.deadlines.is_empty() {
            return;
        }
        let Some(open) = self.block_brace(i, e) else {
            return;
        };
        let checks: String = self
            .deadlines
            .iter()
            .map(|(n, s)| format!("{}; ", self.deadline_check(n, s)))
            .collect();
        self.e.open(self.sp(open).end, checks);
    }

    /// `2s`, `500ms`, `1m`: milliseconds.
    fn duration_ms(&self, i: usize) -> i64 {
        let n = match self.k(i) {
            T::Int(v) => *v,
            _ => return 0,
        };
        // The unit is the identifier right after, with nothing between.
        if let Some(u) = self.ident(i + 1) {
            if self.sp(i + 1).start == self.sp(i).end {
                return match u.as_str() {
                    "ms" => n,
                    "s" => n * 1000,
                    "m" => n * 60_000,
                    _ => n,
                };
            }
        }
        n
    }

    /// `exhale a, b`: each value shown as CigarScript shows it.
    fn exhale(&mut self, i: usize, e: usize) {
        if !self.show || i >= e {
            self.expr(i, e);
            return;
        }
        let mut depth = 0i32;
        let mut start = i;
        let mut j = i;
        while j <= e {
            let at_end = j == e;
            let k = if at_end { T::Eof } else { self.k(j).clone() };
            if !at_end && Self::is_open(&k) {
                depth += 1;
            } else if !at_end && Self::is_close(&k) {
                depth -= 1;
            } else if at_end || (depth == 0 && matches!(k, T::Comma)) {
                if start < j {
                    self.e.open(self.sp(start).start, "_cg_show(");
                    self.e.close(self.sp(j - 1).end, ")");
                }
                start = j + 1;
            }
            j += 1;
        }
        self.expr(i, e);
    }

    // ----- expressions ---------------------------------------------------------------

    /// The end (exclusive token index) of a postfix expression at `i`: a
    /// primary, then `.name`, `?.name`, `(...)`, `[...]`.
    fn postfix_end(&self, i: usize, limit: usize) -> usize {
        let mut j = i;
        match self.k(j) {
            T::LParen | T::LBracket | T::LBrace => j = self.matching(j) + 1,
            T::Pack => {
                return limit;
            }
            _ => j += 1,
        }
        // `Name { ... }` (a record literal) is one primary.
        if let Some(n) = self.ident(i) {
            if self.record_names.contains(&n) && matches!(self.k(j), T::LBrace) {
                j = self.matching(j) + 1;
            }
        }
        while j < limit {
            match self.k(j) {
                T::Dot | T::SafeDot => j += 2,
                T::LParen | T::LBracket => j = self.matching(j) + 1,
                _ => break,
            }
        }
        j.min(limit)
    }

    /// Expression-level constructs in `[i, e)`.
    fn expr(&mut self, i: usize, e: usize) {
        let mut j = i;
        while j < e {
            if j > i {
                self.tries_in_gap(i, j);
            }
            let k = self.k(j).clone();
            match &k {
                T::Pack => {
                    j = self.lambda(j, e);
                    continue;
                }
                T::Str(parts) => {
                    for p in parts {
                        if let StrPart::Interp { span, .. } = p {
                            if self.show {
                                self.e.open(span.start, "_cg_show(");
                                self.e.close(span.end, ")");
                            }
                            self.interpolation(span.start, span.end);
                        }
                    }
                }
                T::Ident(name) => {
                    let name = name.clone();
                    if let Some(module) = self.private_types.get(&name).cloned() {
                        if !self.record_names.contains(&name)
                            && !self.enum_names.contains(&name)
                            && matches!(self.k(j + 1), T::LBrace | T::Dot | T::Lt)
                        {
                            let sp = self.sp(j);
                            self.err_hint(
                                "E1303",
                                format!("`{name}` is in module `{module}` but not exported"),
                                sp.start,
                                sp.end,
                                format!("mark it `pub` in module `{module}`"),
                            );
                        }
                    }
                    // `Set.new()`, `Set.of(xs)` (#38).
                    if self.set_constructor(j) {
                        j += 3;
                        continue;
                    }
                    // `Point.default()` (#38).
                    if (self.record_names.contains(&name) || self.enum_names.contains(&name))
                        && (self.type_default(j, &name) || self.type_from_json(j, &name))
                    {
                        j += 3;
                        continue;
                    }
                    // A record literal: `Name { ... }` or `Name<T> { ... }`.
                    if self.record_names.contains(&name) {
                        let mut b = j + 1;
                        if matches!(self.k(b), T::Lt) {
                            let d = self.close_angle(j + 1, e);
                            if matches!(self.k(d + 1), T::LBrace) {
                                self.e.blank(self.src, self.sp(j + 1).start, self.sp(d).end);
                                b = d + 1;
                            }
                        }
                        if matches!(self.k(b), T::LBrace) {
                            let close = self.matching(b);
                            self.e.replace(
                                self.sp(j).start,
                                self.sp(j).end,
                                format!("_cg_new_{name}("),
                            );
                            self.e.close(self.sp(close).end, ")");
                            self.shorthand_fields(b, close);
                            j = b + 1;
                            continue;
                        }
                    }
                    // The built-in `Result`: `Ok(v)`, `Err(e)`, `attempt(call)`.
                    if self.builtin_result
                        && matches!(self.k(j + 1), T::LParen)
                        && !matches!(self.k(j.wrapping_sub(1)), T::Dot | T::SafeDot | T::Pull)
                    {
                        if name == "Ok" || name == "Err" {
                            j = self.variant_ref("Result", j, j, e);
                            continue;
                        }
                        if name == "attempt" && !self.user_attempt {
                            self.e.replace(
                                self.sp(j).start,
                                self.sp(j + 1).end,
                                "_cg_attempt(pack() => ",
                            );
                            j += 2;
                            continue;
                        }
                    }
                    // An enum value: `Name.variant` or `Name.variant(...)`.
                    if self.enum_names.contains(&name)
                        && matches!(self.k(j + 1), T::Dot)
                        && self.ident(j + 2).is_some()
                    {
                        j = self.enum_ref(j, e);
                        continue;
                    }
                    // A layer: `measure x`.
                    if self.layers.contains(&name)
                        && name != "into"
                        && j + 1 < e
                        && self.starts_operand(j + 1)
                        && !matches!(self.k(j.wrapping_sub(1)), T::Dot | T::SafeDot)
                    {
                        let end = self.postfix_end(j + 1, e);
                        self.e.replace(
                            self.sp(j).start,
                            self.sp(j).end,
                            format!("_cg_layer_{name}("),
                        );
                        self.e.close(self.sp(end - 1).end, ")");
                        j += 1;
                        continue;
                    }
                    if name == "move"
                        && (self.ident(j + 1).is_some() || matches!(self.k(j + 1), T::Pack))
                        && (self.strength == Strength::Full
                            || (self.strength == Strength::Medium && self.ident(j + 1).is_some()))
                    {
                        // `move x` gives `x` away; `move pack ...` is a
                        // function value that owns what it captures. At
                        // medium the gate has let through only `f(move x)`,
                        // a value given to a full function (#22 D3).
                        let sp = self.sp(j);
                        self.e.blank(self.src, sp.start, sp.end);
                        self.move_keys.push(self.sp(j + 1).start);
                    }
                    if name == "channel" && matches!(self.k(j + 1), T::Lt) {
                        let d = self.close_angle(j + 1, e);
                        if matches!(self.k(d + 1), T::LParen) {
                            self.e
                                .replace(self.sp(j).start, self.sp(d).end, "_cg_channel");
                            j = d + 1;
                            continue;
                        }
                    }
                    // A generic function's explicit type arguments.
                    if self.generic_fns.contains(&name) && matches!(self.k(j + 1), T::Lt) {
                        let d = self.close_angle(j + 1, e);
                        if matches!(self.k(d + 1), T::LParen) {
                            self.e.blank(self.src, self.sp(j + 1).start, self.sp(d).end);
                        }
                    }
                    if name == "str"
                        && self.show
                        && matches!(self.k(j + 1), T::LParen)
                        && !matches!(self.k(j.wrapping_sub(1)), T::Dot | T::SafeDot)
                    {
                        self.e.replace(self.sp(j).start, self.sp(j).end, "_cg_show");
                    }
                    if name == "json" && matches!(self.k(j + 1), T::Dot) {
                        if let Some(f) = self.ident(j + 2) {
                            if f == "stringify" && matches!(self.k(j + 3), T::LParen) && self.show {
                                let pc = self.matching(j + 3);
                                let mut c = j + 4;
                                let mut depth = 0i32;
                                while c < pc {
                                    let kk = self.k(c);
                                    if Self::is_open(kk) {
                                        depth += 1;
                                    } else if Self::is_close(kk) {
                                        depth -= 1;
                                    } else if depth == 0 && matches!(kk, T::Comma) {
                                        break;
                                    }
                                    c += 1;
                                }
                                if c > j + 4 {
                                    self.e.open(self.sp(j + 4).start, "_cg_plain(");
                                    self.e.close(self.sp(c - 1).end, ")");
                                }
                            }
                            if (f == "parse" || f == "load") && matches!(self.k(j + 3), T::Lt) {
                                j = self.typed_read(j, j + 3, e, false);
                                continue;
                            }
                        }
                    }
                    if name == "csv"
                        && matches!(self.k(j + 1), T::Dot)
                        && self.is_word(j + 2, "read")
                        && matches!(self.k(j + 3), T::Lt)
                    {
                        j = self.typed_read(j, j + 3, e, true);
                        continue;
                    }
                    // After a `.` or as a key, `match` is a name (#22 D1).
                    let a_name = matches!(self.k(j.wrapping_sub(1)), T::Dot | T::SafeDot)
                        || (matches!(self.k(j + 1), T::Colon)
                            && matches!(
                                self.k(j.wrapping_sub(1)),
                                T::LBrace | T::Comma | T::Newline
                            ));
                    if name == "match" && !a_name {
                        let sp = self.sp(j);
                        self.err_hint(
                            "E1212",
                            "a `match` here is neither a statement nor a value bound by itself",
                            sp.start,
                            sp.end,
                            "bind it first: `stick x = match ...`, or make it a statement",
                        );
                    }
                }
                _ => {}
            }
            j += 1;
        }
        self.tries_in_gap(i, e.max(i + 1));
    }

    /// A `?` written between token `j - 1` and token `j`, which cig's lexer
    /// never saw: the `?` of `expr?`, which sends an `Err` out of the
    /// function. It is `_cg_try(expr)`; the function around it catches what
    /// `_cg_try` throws (`finish_try`).
    fn tries_in_gap(&mut self, lo: usize, j: usize) {
        if j == 0 || j > self.toks.len() - 1 {
            return;
        }
        let from = self.sp(j - 1).end;
        let to = self.sp(j).start;
        if to <= from {
            return;
        }
        let qs: Vec<usize> = self.opt.range(from..to).copied().collect();
        for q in qs {
            if !self.tries_done.insert(q) {
                continue;
            }
            let start = self.operand_start(lo, j - 1);
            self.e.open(self.sp(start).start, "_cg_try(");
            self.e.close(from, ")");
            self.e.blank(self.src, q, q + 1);
            if let Some(f) = self.try_fns.last_mut() {
                f.used = f.used || f.pull;
            }
        }
    }

    /// The first token of the postfix chain that ends at token `j`
    /// (`a.b(c)[0]`), not before `lo`.
    fn operand_start(&self, lo: usize, mut j: usize) -> usize {
        loop {
            match self.k(j) {
                T::RParen | T::RBracket => {
                    let mut depth = 0i32;
                    let mut o = j;
                    loop {
                        let kk = self.k(o);
                        if Self::is_close(kk) {
                            depth += 1;
                        } else if Self::is_open(kk) {
                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                        if o <= lo {
                            return j;
                        }
                        o -= 1;
                    }
                    j = o;
                    if j > lo && (self.ends_expr(j - 1) && !matches!(self.k(j - 1), T::RBrace)) {
                        j -= 1;
                        continue;
                    }
                    return j;
                }
                T::RBrace | T::Comma | T::Newline => return j,
                _ => {}
            }
            if j >= lo + 2 && matches!(self.k(j - 1), T::Dot | T::SafeDot) {
                j -= 2;
                continue;
            }
            return j;
        }
    }

    /// The expression inside `${...}` in a string, lowered like any other:
    /// cig's lexer keeps it inside the string token, so it is read here.
    fn interpolation(&mut self, start: usize, end: usize) {
        let Ok(mut toks) = tokenize(&self.work[start..end]) else {
            return;
        };
        for t in toks.iter_mut() {
            shift(t, start);
        }
        let saved = std::mem::replace(&mut self.toks, toks);
        let n = self.toks.len() - 1;
        self.expr(0, n);
        self.toks = saved;
    }

    /// The first token in `[i, limit)` at bracket depth 0 that `stop` picks;
    /// `limit` when there is none.
    fn depth0(&self, i: usize, limit: usize, stop: impl Fn(&T) -> bool) -> usize {
        let mut depth = 0i32;
        let mut b = i;
        while b < limit {
            let kk = self.k(b);
            if Self::is_open(kk) {
                depth += 1;
            } else if Self::is_close(kk) {
                depth -= 1;
            } else if depth == 0 && stop(kk) {
                return b;
            }
            b += 1;
        }
        limit
    }

    /// The `>` that closes the `<` at `lt`, counting the ones between
    /// (`id<List<Map<Int>>>`); `limit` when there is none.
    fn close_angle(&self, lt: usize, limit: usize) -> usize {
        let mut depth = 0i32;
        let mut d = lt;
        while d < limit {
            match self.k(d) {
                T::Lt => depth += 1,
                T::Gt => {
                    depth -= 1;
                    if depth == 0 {
                        return d;
                    }
                }
                T::Newline | T::Semicolon | T::LBrace | T::RBrace => return limit,
                _ => {}
            }
            d += 1;
        }
        limit
    }

    /// Does an operand start at `i` (for a prefix layer like `measure x`)?
    fn starts_operand(&self, i: usize) -> bool {
        matches!(
            self.k(i),
            T::Ident(_)
                | T::Int(_)
                | T::Float(_)
                | T::Str(_)
                | T::LParen
                | T::LBracket
                | T::LBrace
                | T::Null
                | T::True
                | T::False
        )
    }

    /// `json.parse<T>(text)` / `csv.read<T>(path)`: the value read, made a `T`.
    fn typed_read(&mut self, j: usize, lt: usize, e: usize, rows: bool) -> usize {
        let Some((t, after)) = self.parse_ty(lt + 1) else {
            return lt + 1;
        };
        if !matches!(self.k(after), T::Gt) || !matches!(self.k(after + 1), T::LParen) {
            return after;
        }
        let pc = self.matching(after + 1);
        self.e
            .blank(self.src, self.sp(lt).start, self.sp(after).end);
        let d = ty::descriptor(&t);
        if rows {
            self.e.open(self.sp(j).start, format!("_cg_rows_as({d}, "));
        } else {
            self.e.open(self.sp(j).start, format!("_cg_as({d}, "));
        }
        self.e.close(self.sp(pc).end, ")");
        let _ = e;
        after + 2
    }

    /// `{ from, to: x }` in a record literal: `from` alone is `from: from`.
    fn shorthand_fields(&mut self, open: usize, close: usize) {
        let mut j = open + 1;
        let mut depth = 0i32;
        let mut entry_start = true;
        while j < close {
            let k = self.k(j).clone();
            if Self::is_open(&k) {
                depth += 1;
            } else if Self::is_close(&k) {
                depth -= 1;
            }
            if depth == 0 {
                if entry_start {
                    if let Some(n) = self.ident(j) {
                        if matches!(self.k(j + 1), T::Comma | T::RBrace | T::Newline) {
                            let sp = self.sp(j);
                            self.e.replace(sp.start, sp.end, format!("{n}: {n}"));
                        }
                    }
                    if !matches!(k, T::Newline) {
                        entry_start = false;
                    }
                }
                if matches!(k, T::Comma | T::Newline) {
                    entry_start = true;
                }
            }
            j += 1;
        }
    }

    /// `Enum.variant` or `Enum.variant(args)` at `j`: the constructor call.
    fn enum_ref(&mut self, j: usize, e: usize) -> usize {
        let en = self.ident(j).unwrap();
        self.variant_ref(&en, j, j + 2, e)
    }

    /// `Enum.variant(...)` with the enum's name at `j` and the variant at
    /// `vi` (`j + 2`), or the bare `Ok(...)`/`Err(...)` of the built-in
    /// `Result`, where both are the same token.
    fn variant_ref(&mut self, en: &str, j: usize, vi: usize, e: usize) -> usize {
        let en = en.to_string();
        let v = self.ident(vi).unwrap();
        let decl = self.prog.enums.get(&en).cloned();
        let Some(decl) = decl else { return vi + 1 };
        let Some(var) = decl.variants.iter().find(|x| x.name == v).cloned() else {
            let sp = self.sp(vi);
            let names: Vec<&str> = decl.variants.iter().map(|x| x.name.as_str()).collect();
            self.err_hint(
                "E1207",
                format!("`{en}` has no variant `{v}`"),
                sp.start,
                sp.end,
                format!("its variants are {}", names.join(", ")),
            );
            return vi + 1;
        };
        if matches!(self.k(vi + 1), T::LParen) {
            let pc = self.matching(vi + 1);
            // Each argument's first token, and whether it is `name: value`.
            let mut starts: Vec<usize> = Vec::new();
            let mut depth = 0i32;
            let mut arg_start = true;
            for a in vi + 2..pc {
                let k = self.k(a).clone();
                if depth == 0 && arg_start && !matches!(k, T::Newline) {
                    starts.push(a);
                    arg_start = false;
                }
                if Self::is_open(&k) {
                    depth += 1;
                } else if Self::is_close(&k) {
                    depth -= 1;
                } else if depth == 0 && matches!(k, T::Comma) {
                    arg_start = true;
                }
            }
            let named: Vec<bool> = starts
                .iter()
                .map(|&a| self.ident(a).is_some() && matches!(self.k(a + 1), T::Colon))
                .collect();
            let fields: Vec<String> = var.fields.iter().map(|f| f.0.clone()).collect();
            let plural = |n: usize| if n == 1 { "" } else { "s" };
            if !named.is_empty() && named.iter().all(|n| *n) {
                // By name, in any order: `_cg_named_E_v({h: 4.0, w: 3.0})`.
                for (&a, _) in starts.iter().zip(&named) {
                    let name = self.ident(a).unwrap();
                    if !fields.contains(&name) {
                        let sp = self.sp(a);
                        self.err_hint(
                            "E1208",
                            format!("`{en}.{v}` has no field `{name}`"),
                            sp.start,
                            sp.end,
                            format!("its fields are {}", fields.join(", ")),
                        );
                    }
                }
                for f in &fields {
                    if !starts
                        .iter()
                        .any(|&a| self.ident(a).as_deref() == Some(f.as_str()))
                    {
                        let sp = self.sp(vi);
                        self.err_hint(
                            "E1208",
                            format!("`{en}.{v}` is missing the field `{f}`"),
                            sp.start,
                            sp.end,
                            format!("its fields are {}", fields.join(", ")),
                        );
                    }
                }
                self.e.replace(
                    self.sp(j).start,
                    self.sp(vi + 1).end,
                    format!("_cg_named_{en}_{v}({{"),
                );
                self.e.replace(self.sp(pc).start, self.sp(pc).end, "})");
            } else if named.iter().any(|n| *n) {
                let sp = self.sp(vi);
                self.err_hint(
                    "E1208",
                    format!("`{en}.{v}` gives some fields by name and some by place"),
                    sp.start,
                    sp.end,
                    format!("name every field ({}), or none", fields.join(", ")),
                );
            } else {
                self.e
                    .replace(self.sp(j).start, self.sp(vi).end, format!("_cg_{en}_{v}"));
                let count = starts.len();
                if count != fields.len() {
                    let sp = self.sp(vi);
                    self.err(
                        "E1208",
                        format!(
                            "`{en}.{v}` has {} field{}, and {count} {} given",
                            fields.len(),
                            plural(fields.len()),
                            if count == 1 { "is" } else { "are" }
                        ),
                        sp.start,
                        sp.end,
                    );
                }
            }
            self.expr(vi + 2, pc);
            pc + 1
        } else {
            if !var.fields.is_empty() {
                let sp = self.sp(vi);
                self.err(
                    "E1208",
                    format!(
                        "`{en}.{v}` carries {} field{}: give them as `{en}.{v}(...)`",
                        var.fields.len(),
                        if var.fields.len() == 1 { "" } else { "s" }
                    ),
                    sp.start,
                    sp.end,
                );
            }
            self.e
                .replace(self.sp(j).start, self.sp(vi).end, format!("_cg_{en}_{v}()"));
            let _ = e;
            vi + 1
        }
    }

    /// `pack(a: Int, b) => e` or `pack(a) { ... }`.
    fn lambda(&mut self, p: usize, e: usize) -> usize {
        if !matches!(self.k(p + 1), T::LParen) {
            return p + 1;
        }
        let pc = self.matching(p + 1);
        let mut tys = Vec::new();
        let mut any = false;
        let mut j = p + 2;
        while j < pc {
            if matches!(self.k(j), T::Comma | T::Newline) {
                j += 1;
                continue;
            }
            if self.ident(j).is_none() {
                j += 1;
                continue;
            }
            j += 1;
            if matches!(self.k(j), T::Colon) {
                let colon = j;
                if let Some((t, n)) = self.parse_ty(j + 1) {
                    let end = self.ty_end(n);
                    self.e.blank(self.src, self.sp(colon).start, end);
                    tys.push(Some(t));
                    any = true;
                    j = n;
                    continue;
                }
                j = pc;
            }
            tys.push(None);
        }
        if any {
            self.lambda_keys.push((self.sp(p).start, tys));
        }
        let after = pc + 1;
        if matches!(self.k(after), T::LBrace) {
            self.rets.push(None);
            self.try_fns.push(TryFn {
                pull: false,
                used: false,
            });
            let depth = std::mem::take(&mut self.try_depth);
            let next = self.block(after);
            self.try_depth = depth;
            self.try_fns.pop();
            self.rets.pop();
            return next;
        }
        let _ = e;
        after
    }

    // ----- match ---------------------------------------------------------------------

    /// A pattern in `[i, end)`.
    fn pattern(&mut self, i: usize, end: usize) -> Option<Pat> {
        if i >= end {
            return None;
        }
        let k = self.k(i).clone();
        match &k {
            T::Ident(n) if n == "_" && i + 1 == end => Some(Pat::Wild),
            T::Int(_) | T::Float(_) | T::Str(_) | T::True | T::False | T::Null if i + 1 == end => {
                Some(Pat::Lit(
                    self.src[self.sp(i).start..self.sp(i).end].to_string(),
                ))
            }
            T::Minus if i + 2 == end && matches!(self.k(i + 1), T::Int(_) | T::Float(_)) => Some(
                Pat::Lit(self.src[self.sp(i).start..self.sp(i + 1).end].to_string()),
            ),
            T::Ident(n) => {
                let n = n.clone();
                // Enum.variant(...)
                if self.enum_names.contains(&n) && matches!(self.k(i + 1), T::Dot) {
                    let v = self.ident(i + 2)?;
                    let subs = if matches!(self.k(i + 3), T::LParen) {
                        let pc = self.matching(i + 3);
                        self.sub_patterns(i + 4, pc)
                    } else {
                        Vec::new()
                    };
                    return Some(Pat::Variant {
                        enum_name: Some(n),
                        name: v,
                        subs,
                    });
                }
                // Record { field: p, other }
                if self.record_names.contains(&n) && matches!(self.k(i + 1), T::LBrace) {
                    let close = self.matching(i + 1);
                    let mut fields = Vec::new();
                    let mut a = i + 2;
                    while a < close {
                        if matches!(self.k(a), T::Comma | T::Newline) {
                            a += 1;
                            continue;
                        }
                        let Some(f) = self.ident(a) else { break };
                        if matches!(self.k(a + 1), T::Colon) {
                            let mut b = a + 2;
                            let mut depth = 0i32;
                            while b < close {
                                let kk = self.k(b);
                                if Self::is_open(kk) {
                                    depth += 1;
                                } else if Self::is_close(kk) {
                                    depth -= 1;
                                } else if depth == 0 && matches!(kk, T::Comma | T::Newline) {
                                    break;
                                }
                                b += 1;
                            }
                            let p = self.pattern(a + 2, b).unwrap_or(Pat::Wild);
                            fields.push((f, p));
                            a = b;
                        } else {
                            fields.push((f.clone(), Pat::Bind(f)));
                            a += 1;
                        }
                    }
                    return Some(Pat::Record { name: n, fields });
                }
                // variant(...) or a bare name
                if matches!(self.k(i + 1), T::LParen) {
                    let pc = self.matching(i + 1);
                    let subs = self.sub_patterns(i + 2, pc);
                    return Some(Pat::Variant {
                        enum_name: None,
                        name: n,
                        subs,
                    });
                }
                if i + 1 == end {
                    return Some(Pat::Bind(n));
                }
                None
            }
            _ => None,
        }
    }

    fn sub_patterns(&mut self, mut a: usize, pc: usize) -> Vec<Pat> {
        let mut out = Vec::new();
        while a < pc {
            if matches!(self.k(a), T::Comma | T::Newline) {
                a += 1;
                continue;
            }
            let mut b = a;
            let mut depth = 0i32;
            while b < pc {
                let kk = self.k(b);
                if Self::is_open(kk) {
                    depth += 1;
                } else if Self::is_close(kk) {
                    depth -= 1;
                } else if depth == 0 && matches!(kk, T::Comma) {
                    break;
                }
                b += 1;
            }
            out.push(self.pattern(a, b).unwrap_or(Pat::Wild));
            a = b;
        }
        out
    }

    /// The enum a set of patterns is about: named explicitly, or the one
    /// enum whose variants every bare name and variant pattern names.
    fn match_enum(&self, pats: &[&Pat]) -> Option<String> {
        for p in pats {
            if let Pat::Variant {
                enum_name: Some(e), ..
            } = p
            {
                return Some(e.clone());
            }
        }
        let names: Vec<&str> = pats
            .iter()
            .filter_map(|p| match p {
                Pat::Variant { name, .. } => Some(name.as_str()),
                Pat::Bind(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        if !pats.iter().any(|p| matches!(p, Pat::Variant { .. })) {
            // Only bare names: an enum only when every one is a variant.
            let cands: Vec<&EnumDecl> = self
                .prog
                .enums
                .values()
                .filter(|e| {
                    !names.is_empty()
                        && names
                            .iter()
                            .all(|n| e.variants.iter().any(|v| v.name == *n))
                })
                .collect();
            let cands = self.without_builtin(cands);
            return if cands.len() == 1 {
                Some(cands[0].name.clone())
            } else {
                None
            };
        }
        let cands: Vec<&EnumDecl> = self
            .prog
            .enums
            .values()
            .filter(|e| {
                pats.iter().all(|p| match p {
                    Pat::Variant { name, .. } => e.variants.iter().any(|v| v.name == *name),
                    _ => true,
                })
            })
            .collect();
        let cands = self.without_builtin(cands);
        if cands.len() == 1 {
            Some(cands[0].name.clone())
        } else {
            None
        }
    }

    /// When a file's own enum shares variant names with the built-in
    /// `Result` (`Ok`), a bare pattern means its own: `Result.Ok(x)` says
    /// the other.
    fn without_builtin<'e>(&self, cands: Vec<&'e EnumDecl>) -> Vec<&'e EnumDecl> {
        if self.builtin_result && cands.len() > 1 {
            cands.into_iter().filter(|e| e.name != "Result").collect()
        } else {
            cands
        }
    }

    /// The test a pattern makes of `subject`, and its bindings.
    fn pat_code(
        &self,
        p: &Pat,
        subject: &str,
        en: Option<&str>,
        field_ty: Option<&Ty>,
        tests: &mut Vec<String>,
        binds: &mut Vec<(String, String, Ty)>,
    ) {
        match p {
            Pat::Wild => {}
            Pat::Lit(l) => tests.push(format!("{subject} == {l}")),
            Pat::Bind(n) => {
                // A bare name that is a variant of the enum matched.
                if let Some(e) = en {
                    if let Some(d) = self.prog.enums.get(e) {
                        if d.variants.iter().any(|v| v.name == *n) {
                            tests.push(format!("_cg_variant({subject}, \"{e}\", \"{n}\")"));
                            return;
                        }
                    }
                }
                binds.push((
                    n.clone(),
                    subject.to_string(),
                    field_ty.cloned().unwrap_or(Ty::Any),
                ));
            }
            Pat::Variant {
                enum_name,
                name,
                subs,
            } => {
                let e = enum_name
                    .clone()
                    .or_else(|| en.map(str::to_string))
                    .unwrap_or_default();
                tests.push(format!("_cg_variant({subject}, \"{e}\", \"{name}\")"));
                let fields: Vec<(String, Ty)> = self
                    .prog
                    .enums
                    .get(&e)
                    .and_then(|d| d.variants.iter().find(|v| v.name == *name))
                    .map(|v| v.fields.clone())
                    .unwrap_or_default();
                for (idx, sp) in subs.iter().enumerate() {
                    let (fname, fty) = fields
                        .get(idx)
                        .cloned()
                        .unwrap_or((format!("_{idx}"), Ty::Any));
                    let sub_subject = format!("{subject}.{fname}");
                    let sub_en = match &fty {
                        Ty::Enum(n, _) => Some(n.clone()),
                        _ => None,
                    };
                    self.pat_code(
                        sp,
                        &sub_subject,
                        sub_en.as_deref(),
                        Some(&fty),
                        tests,
                        binds,
                    );
                }
            }
            Pat::Record { name, fields } => {
                tests.push(format!("_cg_is_record({subject}, \"{name}\")"));
                let decl = self.prog.records.get(name).cloned();
                for (f, sp) in fields {
                    let fty = decl
                        .as_ref()
                        .and_then(|d| d.fields.iter().find(|x| x.0 == *f))
                        .map(|x| x.1.clone())
                        .unwrap_or(Ty::Any);
                    let sub_en = match &fty {
                        Ty::Enum(n, _) => Some(n.clone()),
                        _ => None,
                    };
                    self.pat_code(
                        sp,
                        &format!("{subject}.{f}"),
                        sub_en.as_deref(),
                        Some(&fty),
                        tests,
                        binds,
                    );
                }
            }
        }
    }

    /// `match` at token `m`. Returns the index after the statement.
    fn match_at(&mut self, m: usize, end: usize, target: Target) -> usize {
        let e = self.stmt_end(m, end);
        let Some(open) = self.block_brace(m + 1, e) else {
            let sp = self.sp(m);
            self.err(
                "E1212",
                "a match is `match value { pattern => result ... }`",
                sp.start,
                sp.end,
            );
            return e;
        };
        // `match P { x: 1 } { ... }` reads the record's braces as the arms.
        if self
            .ident(open - 1)
            .is_some_and(|n| self.record_names.contains(&n))
        {
            let sp = self.sp(open - 1);
            self.err_hint(
                "E1212",
                "a record written right after `match` needs parentheses",
                sp.start,
                sp.end,
                "write `match (P { ... }) { ... }`, or bind it first",
            );
            return self.stmt_end(m, end).max(self.matching(open) + 1);
        }
        let close = self.matching(open);
        let n = self.fresh();
        let (msub, done, res) = (
            format!("__cg_m{n}"),
            format!("__cg_d{n}"),
            format!("__cg_r{n}"),
        );
        let is_value = matches!(target, Target::Value { .. });

        // The arms.
        struct Arm {
            pat_start: usize,
            pat_end: usize,
            guard: Option<(usize, usize)>,
            arrow: usize,
            body: (usize, usize),
            block: bool,
            sep: Option<usize>,
        }
        let mut arms: Vec<Arm> = Vec::new();
        let mut j = open + 1;
        while j < close {
            if matches!(self.k(j), T::Newline | T::Comma | T::Semicolon) {
                j += 1;
                continue;
            }
            let pat_start = j;
            let mut depth = 0i32;
            let mut arrow = None;
            let mut guard_at = None;
            let mut q = j;
            while q < close {
                let kk = self.k(q);
                if Self::is_open(kk) {
                    depth += 1;
                } else if Self::is_close(kk) {
                    depth -= 1;
                } else if depth == 0 && matches!(kk, T::If) && guard_at.is_none() {
                    guard_at = Some(q);
                } else if depth == 0 && matches!(kk, T::FatArrow) {
                    arrow = Some(q);
                    break;
                }
                q += 1;
            }
            let Some(arrow) = arrow else {
                let sp = self.sp(j);
                self.err(
                    "E1212",
                    "each arm of a match is `pattern => result`",
                    sp.start,
                    sp.end,
                );
                return close + 1;
            };
            let pat_end = guard_at.unwrap_or(arrow);
            let guard = guard_at.map(|g| (g + 1, arrow));
            let (body, block) = if matches!(self.k(arrow + 1), T::LBrace) {
                let bc = self.matching(arrow + 1);
                ((arrow + 1, bc + 1), true)
            } else {
                // An arm ends at the line's end, or at a comma before the
                // next arm. `exhale a, b` keeps its commas when no other arm
                // follows on the line.
                let line_end = self.depth0(arrow + 1, close, |k| matches!(k, T::Newline));
                let arm_after =
                    self.depth0(arrow + 1, line_end, |k| matches!(k, T::FatArrow)) < line_end;
                let b = if matches!(self.k(arrow + 1), T::Exhale) && !arm_after {
                    line_end
                } else {
                    self.depth0(arrow + 1, close, |k| matches!(k, T::Newline | T::Comma))
                };
                ((arrow + 1, b), false)
            };
            let sep = if matches!(self.k(body.1), T::Comma) {
                Some(body.1)
            } else {
                None
            };
            arms.push(Arm {
                pat_start,
                pat_end,
                guard,
                arrow,
                body,
                block,
                sep,
            });
            j = body.1;
        }

        // Patterns, the enum, exhaustiveness.
        let pats: Vec<Option<Pat>> = arms
            .iter()
            .map(|a| (a.pat_start, a.pat_end))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|(s, en)| self.pattern(s, en))
            .collect();
        for (a, p) in arms.iter().zip(&pats) {
            if p.is_none() {
                let sp = self.sp(a.pat_start);
                self.err(
                    "E1212",
                    "this pattern is not one CigarScript reads: `_`, a literal, a name, `variant(...)`, `Enum.variant(...)` or `Record { field }`",
                    sp.start,
                    sp.end,
                );
            }
        }
        let pats: Vec<Pat> = pats.into_iter().map(|p| p.unwrap_or(Pat::Wild)).collect();
        let refs: Vec<&Pat> = pats.iter().collect();
        let en = self.match_enum(&refs);
        self.exhaustive(
            m,
            &arms.iter().map(|a| a.guard.is_some()).collect::<Vec<_>>(),
            &pats,
            en.as_deref(),
        );

        // The header.
        match &target {
            Target::Stmt => {
                self.e
                    .replace(self.sp(m).start, self.sp(m).end, format!("stick {msub} ="));
            }
            Target::Value { stmt_start, .. } => {
                self.e.replace(
                    self.sp(*stmt_start).start,
                    self.sp(m).end,
                    format!("roll {res} = null; stick {msub} ="),
                );
            }
        }
        self.e.replace(
            self.sp(open).start,
            self.sp(open).end,
            format!("; roll {done} = false;"),
        );

        let scrut_ty: Option<Ty> = en.as_ref().map(|e| Ty::Enum(e.clone(), Vec::new()));
        for (a, p) in arms.iter().zip(&pats) {
            let mut tests = Vec::new();
            let mut binds = Vec::new();
            self.pat_code(
                p,
                &msub,
                en.as_deref(),
                scrut_ty.as_ref(),
                &mut tests,
                &mut binds,
            );
            let test = if tests.is_empty() {
                "true".to_string()
            } else {
                tests.join(" and ")
            };
            let mut head = format!("if not {done} and {test} {{ ");
            let mut offsets = Vec::new();
            for (name, subj, t) in &binds {
                offsets.push((head.len(), t.clone()));
                head.push_str(&format!("stick {name} = {subj}; "));
            }
            let assign = if is_value {
                format!("{res} = ")
            } else {
                String::new()
            };
            if let Some((gs, ge)) = a.guard {
                head.push_str("if (");
                let seq = self
                    .e
                    .replace(self.sp(a.pat_start).start, self.sp(gs - 1).end, head);
                for (off, t) in offsets {
                    self.gen_bindings.push((seq, off, t));
                }
                self.e.replace(
                    self.sp(a.arrow).start,
                    self.sp(a.arrow).end,
                    format!(") {{ {done} = true; {assign}"),
                );
                self.expr(gs, ge);
            } else {
                head.push_str(&format!("{done} = true; {assign}"));
                let seq = self
                    .e
                    .replace(self.sp(a.pat_start).start, self.sp(a.arrow).end, head);
                for (off, t) in offsets {
                    self.gen_bindings.push((seq, off, t));
                }
            }
            let closing = if a.guard.is_some() { "} }" } else { "}" };
            if a.block {
                if is_value {
                    let sp = self.sp(a.body.0);
                    self.err(
                        "E1213",
                        "a match used as a value needs an expression in each arm, not a block",
                        sp.start,
                        sp.end,
                    );
                }
                let (bo, bc) = (a.body.0, a.body.1 - 1);
                self.e.replace(self.sp(bo).start, self.sp(bo).end, " ");
                self.e.replace(self.sp(bc).start, self.sp(bc).end, closing);
                self.stmts(bo + 1, bc);
            } else {
                if a.body.0 < a.body.1 {
                    self.e
                        .close(self.sp(a.body.1 - 1).end, format!(" {closing}"));
                    if self.is_word(a.body.0, "match") {
                        let sp = self.sp(a.body.0);
                        self.err_hint(
                            "E1212",
                            "a match inside a match arm: bind it first",
                            sp.start,
                            sp.end,
                            "`stick inner = match ...` above, then use `inner`",
                        );
                    }
                    // A statement in an arm (exhale, burn, snuff...) is a
                    // statement; anything else is an expression.
                    if matches!(
                        self.k(a.body.0),
                        T::Exhale | T::Snuff | T::Cough | T::Burn | T::Break | T::Continue
                    ) || matches!(self.k(a.body.0), T::Ident(_))
                        && self.has_assign(a.body.0, a.body.1)
                    {
                        self.stmts(a.body.0, a.body.1);
                    } else {
                        self.expr(a.body.0, a.body.1);
                    }
                } else {
                    self.e.close(self.sp(a.arrow).end, format!(" {closing}"));
                }
            }
            if let Some(s) = a.sep {
                self.e.replace(self.sp(s).start, self.sp(s).end, ";");
            }
        }

        // The scrutinee.
        self.expr(m + 1, open);

        // The end.
        match target {
            Target::Stmt => {
                self.e
                    .replace(self.sp(close).start, self.sp(close).end, " ");
            }
            Target::Value { then, ty, name, .. } => {
                let text = match (&ty, &name) {
                    (Some(t), Some(nm)) if ty::checks(t) => {
                        format!("; {then}{res}; {}", guard(nm, t, nm))
                    }
                    (Some(t), None) if ty::checks(t) => {
                        let f = self
                            .rets
                            .last()
                            .cloned()
                            .flatten()
                            .map(|r| r.1)
                            .unwrap_or_default();
                        format!("; {}; {then}{res}", guard(&res, t, &format!("{f} returns")))
                    }
                    _ => format!("; {then}{res}"),
                };
                let seq = self
                    .e
                    .replace(self.sp(close).start, self.sp(close).end, text);
                if let (Some(t), Some(_)) = (ty, name) {
                    self.gen_bindings.push((seq, 2, t));
                }
            }
        }
        e.max(close + 1)
    }

    fn has_assign(&self, a: usize, b: usize) -> bool {
        (a..b).any(|j| {
            matches!(
                self.k(j),
                T::Assign | T::PlusAssign | T::MinusAssign | T::StarAssign | T::SlashAssign
            )
        })
    }

    /// A record pattern that tests nothing: each field `_` or a name that
    /// is not a variant of the field's enum.
    fn irrefutable_record(&self, name: &str, fields: &[(String, Pat)]) -> bool {
        let Some(r) = self.prog.records.get(name) else {
            return false;
        };
        fields.iter().all(|(f, p)| match p {
            Pat::Wild => true,
            Pat::Bind(n) => {
                let fty = r.fields.iter().find(|x| x.0 == *f).map(|x| &x.1);
                !matches!(fty, Some(Ty::Enum(e, _)) if self.prog.enums.get(e).is_some_and(|d| d.variants.iter().any(|v| v.name == *n)))
            }
            _ => false,
        })
    }

    fn exhaustive(&mut self, m: usize, guarded: &[bool], pats: &[Pat], en: Option<&str>) {
        let unguarded: Vec<&Pat> = pats
            .iter()
            .zip(guarded)
            .filter(|(_, g)| !**g)
            .map(|(p, _)| p)
            .collect();
        let catch_all = unguarded.iter().any(|p| match p {
            Pat::Wild => true,
            Pat::Bind(n) => !en
                .and_then(|e| self.prog.enums.get(e))
                .is_some_and(|d| d.variants.iter().any(|v| v.name == *n)),
            // `Move { from, to }`: every field bound, nothing tested.
            Pat::Record { name, fields } => self.irrefutable_record(name, fields),
            _ => false,
        });
        if catch_all {
            return;
        }
        let irrefutable = |subs: &[Pat]| subs.iter().all(|s| matches!(s, Pat::Wild | Pat::Bind(_)));
        let sp = self.sp(m);
        if let Some(e) = en {
            let Some(d) = self.prog.enums.get(e).cloned() else {
                return;
            };
            let mut missing = Vec::new();
            for v in &d.variants {
                let covered = unguarded.iter().any(|p| match p {
                    Pat::Variant { name, subs, .. } => *name == v.name && irrefutable(subs),
                    Pat::Bind(n) => *n == v.name,
                    _ => false,
                });
                if !covered {
                    missing.push(if v.fields.is_empty() {
                        v.name.clone()
                    } else {
                        format!("{}(...)", v.name)
                    });
                }
            }
            if !missing.is_empty() {
                self.err_hint(
                    "E1211",
                    format!(
                        "this match does not cover every `{e}`: missing {}",
                        missing.join(", ")
                    ),
                    sp.start,
                    sp.end,
                    "add an arm for each, or `_ => ...` for the rest",
                );
            }
            return;
        }
        let lits: Vec<&str> = unguarded
            .iter()
            .filter_map(|p| match p {
                Pat::Lit(l) => Some(l.as_str()),
                _ => None,
            })
            .collect();
        if lits.contains(&"true") && lits.contains(&"false") {
            return;
        }
        let msg = if lits.contains(&"true") {
            "this match does not cover every value: missing false".to_string()
        } else if lits.contains(&"false") {
            "this match does not cover every value: missing true".to_string()
        } else {
            "this match does not cover every value: it needs a `_ => ...` arm for the rest"
                .to_string()
        };
        self.err_hint(
            "E1211",
            msg,
            sp.start,
            sp.end,
            "add an arm for each missing value, or `_ => ...` for the rest",
        );
    }

    // ----- modules -------------------------------------------------------------------

    /// `use tools.plan` / `use tools.plan.{Move, plan_moves}`.
    fn use_stmt(&mut self, i: usize, end: usize) -> usize {
        let e = self.stmt_end(i, end);
        let mut parts = Vec::new();
        let mut names: Vec<String> = Vec::new();
        let mut j = i + 1;
        while j < e {
            match self.k(j) {
                T::Ident(s) => {
                    parts.push(s.clone());
                    j += 1;
                }
                T::Dot => {
                    if matches!(self.k(j + 1), T::LBrace) {
                        let close = self.matching(j + 1);
                        for x in j + 2..close {
                            if let Some(n) = self.ident(x) {
                                names.push(n);
                            }
                        }
                        j = close + 1;
                    } else {
                        j += 1;
                    }
                }
                _ => j += 1,
            }
        }
        let at = (self.sp(i).start, self.sp(e - 1).end);
        if parts.is_empty() {
            self.err(
                "E1301",
                "`use` names a module: `use tools.plan`",
                at.0,
                at.1,
            );
            return e;
        }
        let fname = format!("_cg_mod_{}", parts.join("_"));
        let local = parts.last().unwrap().clone();
        let shown = parts.join(".");
        // `use tools.plan` and `use tools.plan.{Move}` in one file bind
        // `plan` once.
        let mut text = if self.bound_modules.insert(fname.clone()) {
            format!("stick {local} = {fname}()")
        } else {
            String::new()
        };
        let mut typed: Vec<(usize, Ty)> = Vec::new();
        if let Some(m) = self.load_module(&parts, at) {
            for (n, k) in &m.exports {
                if let Export::Function(sig) = k {
                    self.prog
                        .module_fns
                        .insert(format!("{local}.{n}"), sig.clone());
                }
            }
            for n in &names {
                match m.exports.iter().find(|(x, _)| x == n).map(|x| &x.1) {
                    // A type: its constructor is global, nothing to bind.
                    Some(Export::Interface | Export::Record | Export::Enum) => {}
                    Some(Export::Function(sig)) => {
                        self.prog.module_fns.insert(n.clone(), sig.clone());
                        let off = text.len() + 2;
                        text.push_str(&format!("; stick {n} = {local}.{n}"));
                        if sig.generics.is_empty() {
                            let ps = sig
                                .params
                                .iter()
                                .map(|p| p.ty.clone().unwrap_or(Ty::Any))
                                .collect();
                            typed.push((
                                off,
                                Ty::Fn(ps, Box::new(sig.ret.clone().unwrap_or(Ty::Any))),
                            ));
                        }
                    }
                    Some(Export::Value) => text.push_str(&format!("; stick {n} = {local}.{n}")),
                    None => {
                        let (code, msg, hint) = if m.declared.contains(n) {
                            (
                                "E1303",
                                format!("`{n}` is in module `{shown}` but not exported"),
                                format!("mark it `pub` in {}", m.path.display()),
                            )
                        } else {
                            (
                                "E1304",
                                format!("module `{shown}` has no `{n}`"),
                                format!("it exports {}", export_list(&m)),
                            )
                        };
                        self.err_hint(code, msg, at.0, at.1, hint);
                    }
                }
            }
        }
        // What a second `use` of a module binds starts with "; ".
        let (text, cut) = match text.strip_prefix("; ") {
            Some(t) => (t.to_string(), 2),
            None => (text, 0),
        };
        let seq = if text.is_empty() {
            self.e.blank(self.src, at.0, at.1)
        } else {
            self.e.replace(at.0, at.1, text)
        };
        for (off, t) in typed {
            self.gen_bindings.push((seq, off - cut, t));
        }
        e
    }

    /// Find, check and lower a module (once per program), and make its
    /// records and enums types here.
    fn load_module(&mut self, parts: &[String], at: (usize, usize)) -> Option<Module> {
        let shown = parts.join(".");
        let fname = format!("_cg_mod_{}", parts.join("_"));
        let root = self
            .opts
            .root
            .map(Path::to_path_buf)
            .or_else(|| self.opts.file.and_then(Path::parent).map(Path::to_path_buf));
        let Some(root) = root else {
            self.err_hint(
                "E1301",
                format!("module `{shown}` cannot be found: this file has no folder"),
                at.0,
                at.1,
                "a module is found from the folder of the file being checked or run",
            );
            return None;
        };
        let mut path = root.clone();
        for p in parts {
            path.push(p);
        }
        path.set_extension("cigar");
        let canon = std::fs::canonicalize(&path).ok();
        if let Some(c) = &canon {
            let mut chain: Vec<(String, PathBuf)> = self.opts.stack.clone();
            if let Some(f) = self.opts.file {
                chain.push((
                    self.opts.name.clone(),
                    std::fs::canonicalize(f).unwrap_or(f.to_path_buf()),
                ));
            }
            if let Some(pos) = chain.iter().position(|(_, p)| p == c) {
                let mut cycle: Vec<String> = chain[pos..].iter().map(|(n, _)| n.clone()).collect();
                cycle.push(shown.clone());
                self.err_hint(
                    "E1302",
                    format!("modules use each other in a cycle: {}", cycle.join(" -> ")),
                    at.0,
                    at.1,
                    "move what both need into a third module",
                );
                return None;
            }
        }
        let known = self.opts.modules.borrow().get(&fname).cloned();
        let m = match known {
            Some(m) => m,
            None => {
                let src = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(_) => {
                        self.err_hint(
                            "E1301",
                            format!("no module `{shown}`: {} is not there", path.display()),
                            at.0,
                            at.1,
                            "a module is a .cigar file, named by its path from the folder of the file being checked or run",
                        );
                        return None;
                    }
                };
                let mut stack = self.opts.stack.clone();
                if let Some(f) = self.opts.file {
                    stack.push((
                        self.opts.name.clone(),
                        std::fs::canonicalize(f).unwrap_or(f.to_path_buf()),
                    ));
                }
                let sub = Options {
                    strength: self.strength,
                    file: Some(&path),
                    root: Some(&root),
                    stack,
                    modules: self.opts.modules,
                    compile: self.opts.compile,
                    name: shown.clone(),
                };
                let m = (self.opts.compile)(&path, &shown, &src, &sub);
                self.opts
                    .modules
                    .borrow_mut()
                    .insert(fname.clone(), m.clone());
                m
            }
        };
        if let Some(cycle) = m.errors.iter().find(|d| d.code == "E1302") {
            let msg = cycle.message.clone();
            self.err_hint(
                "E1302",
                msg,
                at.0,
                at.1,
                "move what both need into a third module",
            );
        } else if let Some(first) = m.errors.first() {
            let msg = format!("module `{shown}` does not check: {}", first.message);
            self.err_hint(
                "E1301",
                msg,
                at.0,
                at.1,
                format!("`cigar check {}` shows where", m.path.display()),
            );
        }
        // A module's `pub` interfaces can be bounds and `impl`s here (for a
        // type this file declares: the orphan rule, #38).
        for (n, i) in &m.program.interfaces {
            if i.public {
                self.prog.foreign.insert(n.clone());
                // A module's interface named like a standard one is the
                // module's: the prelude's goes, with its impls.
                if self.prog.standard.remove(n) {
                    self.prog.interfaces.remove(n);
                    self.prog
                        .impls
                        .retain(|im| !(im.builtin && &im.interface == n));
                }
                self.prog
                    .interfaces
                    .entry(n.clone())
                    .or_insert_with(|| i.clone());
            }
        }
        for n in m.program.records.keys().chain(m.program.enums.keys()) {
            self.prog.foreign.insert(n.clone());
        }
        // A module's `pub` records and enums are types here; the others stay
        // its own (their values can still reach this file, untyped).
        for (n, r) in &m.program.records {
            if r.public {
                self.record_names.insert(n.clone());
            } else {
                self.private_types.insert(n.clone(), shown.clone());
            }
            self.prog
                .records
                .entry(n.clone())
                .or_insert_with(|| r.clone());
        }
        for (n, en) in &m.program.enums {
            if en.public {
                self.enum_names.insert(n.clone());
            } else {
                self.private_types.insert(n.clone(), shown.clone());
            }
            self.prog
                .enums
                .entry(n.clone())
                .or_insert_with(|| en.clone());
        }
        if !self.prog.records.is_empty() || !self.prog.enums.is_empty() {
            self.show = true;
        }
        self.modules.push(m.path.clone());
        Some(m)
    }

    // ----- finish --------------------------------------------------------------------

    fn finish(mut self) -> Lowered {
        self.fill_defaults();
        if let Some((a, b)) = self.e.overlap() {
            let mut d = Diagnostic::new(
                DK::Internal,
                format!(
                    "the lowering wrote over its own work at {}..{} and {}..{}: a bug, please report it",
                    a.start, a.end, b.start, b.end
                ),
            );
            d.code = "E1104";
            self.diags.push(d);
        }
        let applied = self.e.apply(self.src);
        let mut text = applied.text.clone();
        let appended_at = text.len();
        if !text.ends_with('\n') {
            text.push('\n');
        }
        // This file's own declarations: what a file using it as a module
        // takes along.
        text.push_str(APPENDED_MARK);
        text.push('\n');
        text.push_str(&self.appended);
        text.push_str(&self.dispatchers());
        // The program's: records by name, every module, the prelude.
        text.push_str(PROGRAM_MARK);
        text.push('\n');
        let records: Vec<String> = self
            .prog
            .records
            .keys()
            .map(|n| format!("{n}: _cg_new_{n}"))
            .collect();
        text.push_str(&format!(
            "pull _cg_records() {{ snuff {{{}}} }}\n",
            records.join(", ")
        ));
        // `derive(Json)` (#38): what each type reads, and the reader.
        if json::any(&self.prog) {
            text.push_str(&json::specs(&self.prog));
            text.push_str(json::JSON);
        }
        // Each record's field types, for the path a boundary failure names.
        let rdescs: Vec<String> = self
            .prog
            .records
            .iter()
            .map(|(n, r)| {
                let fields: Vec<String> = r
                    .fields
                    .iter()
                    .map(|(f, t)| format!("{f}: {}", ty::descriptor(t)))
                    .collect();
                format!("{n}: {{{}}}", fields.join(", "))
            })
            .collect();
        text.push_str(&format!(
            "pull _cg_rdescs() {{ snuff {{{}}} }}\n",
            rdescs.join(", ")
        ));
        for (fname, m) in self.opts.modules.borrow().iter() {
            let exports: Vec<String> = m
                .exports
                .iter()
                .filter(|(_, k)| matches!(k, Export::Function(_) | Export::Value))
                .map(|(n, _)| format!("{n}: {n}"))
                .collect();
            text.push_str(&format!(
                "pull {fname}() {{\n{}\nsnuff {{{}}}\n}}\n{}",
                m.body,
                exports.join(", "),
                m.decls
            ));
        }
        text.push_str(PRELUDE_MARK);
        text.push('\n');
        text.push_str(&prelude(self.user_result));
        let mut prog = std::mem::take(&mut self.prog);
        for (o, s) in self.fn_keys {
            if let Some(l) = applied.lowered(o) {
                prog.fns.insert(l, s);
            }
        }
        for (o, t) in self.bind_keys {
            if let Some(l) = applied.lowered(o) {
                prog.bindings.insert(l, t);
            }
        }
        for (o, t) in self.lambda_keys {
            if let Some(l) = applied.lowered(o) {
                prog.lambdas.insert(l, t);
            }
        }
        for o in self.move_keys {
            if let Some(l) = applied.lowered(o) {
                prog.moves.insert(l);
            }
        }
        for o in self.parallel_keys {
            if let Some(l) = applied.lowered(o) {
                prog.parallel_fors.insert(l);
            }
        }
        for (seq, off, t) in self.gen_bindings {
            if let Some(s) = applied.edit_start(seq) {
                prog.bindings.insert(s + off, t);
            }
        }
        Lowered {
            text,
            program: prog,
            applied,
            appended_at,
            diags: self.diags,
            modules: self.modules,
            withins: self.withins,
        }
    }

    /// `pull _cg_dispatch_Iface_m(self, ...)`, for a receiver whose type the
    /// checker could not tell: picks the implementation by the value's tag,
    /// or by its kind for text, numbers and lists, and otherwise calls the
    /// method the value has itself.
    fn dispatchers(&self) -> String {
        let mut out = String::new();
        for (iname, iface) in &self.prog.interfaces {
            // A module's interface has its dispatcher in the module.
            if self.prog.foreign.contains(iname) {
                continue;
            }
            // A method with no `self` (`Default.default`) is called on a
            // type, never picked by a value.
            for m in iface
                .methods
                .iter()
                .filter(|m| m.params.first().is_some_and(|p| p.name == "self"))
            {
                let params: Vec<String> = m
                    .params
                    .iter()
                    .filter(|p| p.name != "self")
                    .map(|p| p.name.clone())
                    .collect();
                let args = if params.is_empty() {
                    String::new()
                } else {
                    format!(", {}", params.join(", "))
                };
                let mut body = String::new();
                // Only the impls that define the method (one that does not
                // is E1311, reported by the checker).
                for imp in
                    self.prog.impls.iter().filter(|i| {
                        i.interface == *iname && i.methods.iter().any(|x| x.name == m.name)
                    })
                {
                    let f = impl_fn_name(iname, &imp.for_ty, &m.name);
                    let test = impls::impl_test(&imp.for_ty, "self");
                    body.push_str(&format!("  if {test} {{ snuff {f}(self{args}) }}\n"));
                }
                let plain = params.join(", ");
                out.push_str(&format!(
                    "pull _cg_dispatch_{iname}_{}(self{args}) {{\n{body}  snuff self.{}({plain})\n}}\n",
                    m.name, m.name
                ));
            }
        }
        out
    }
}

/// A run-time test that the value named `v` is of type `t`, as an `impl`
/// or a layer picks one.
pub fn type_test(t: &Ty, v: &str) -> String {
    match t {
        Ty::Record(n, _) => format!("_cg_is_record({v}, \"{n}\")"),
        Ty::Enum(n, _) => format!("_cg_is_enum({v}, \"{n}\")"),
        Ty::Text | Ty::Path => format!("type_of({v}) == \"string\""),
        Ty::Int | Ty::Sized(_) => format!("type_of({v}) == \"int\""),
        Ty::Float => format!("type_of({v}) == \"float\""),
        Ty::Bool => format!("type_of({v}) == \"bool\""),
        Ty::List(_) => format!("type_of({v}) == \"list\""),
        Ty::Map(_) => format!("type_of({v}) == \"map\" and not _cg_is_record({v}, null) and not _cg_is_enum({v}, null)"),
        _ => "false".to_string(),
    }
}

/// The `if` lines of a dispatcher for `method` of `iface`, over the impls
/// that define it, called with `self` and `args` (already joined, with a
/// leading ", " when there are any).
pub fn dispatch_lines(prog: &Program, iface: &str, method: &str, args: &str) -> String {
    let mut body = String::new();
    for imp in prog
        .impls
        .iter()
        .filter(|i| i.interface == iface && i.methods.iter().any(|x| x.name == method))
    {
        let f = impl_fn_name(iface, &imp.for_ty, method);
        let test = impls::impl_test(&imp.for_ty, "self");
        body.push_str(&format!("if {test} {{ snuff {f}(self{args}) }}; "));
    }
    body
}

/// Marks the start of a file's own lowered declarations (a comment, so cig
/// ignores it).
pub const APPENDED_MARK: &str = "# --- cigar: lowered declarations ---";
/// Marks the start of what the whole program shares: records by name, the
/// modules, the prelude.
pub const PROGRAM_MARK: &str = "# --- cigar: the program ---";
/// Marks where the prelude starts.
pub const PRELUDE_MARK: &str = "# --- cigar: prelude ---";

/// What a module exports, for a message.
fn export_list(m: &Module) -> String {
    if m.exports.is_empty() {
        return "nothing (mark names `pub` to export them)".to_string();
    }
    let names: Vec<&str> = m.exports.iter().map(|(n, _)| n.as_str()).collect();
    names.join(", ")
}

/// A module's exports and every top-level name it declares, from its
/// source and what lowering it found.
pub fn module_names(src: &str, program: &Program) -> (Vec<(String, Export)>, Vec<String>) {
    let mut exports = Vec::new();
    let mut declared = Vec::new();
    for (n, r) in &program.records {
        declared.push(n.clone());
        if r.public {
            exports.push((n.clone(), Export::Record));
        }
    }
    for (n, e) in &program.enums {
        declared.push(n.clone());
        if e.public {
            exports.push((n.clone(), Export::Enum));
        }
    }
    for (n, i) in &program.interfaces {
        declared.push(n.clone());
        if i.public {
            exports.push((n.clone(), Export::Interface));
        }
    }
    for s in program.fns.values() {
        if s.name.starts_with("_cg_") {
            continue;
        }
        declared.push(s.name.clone());
        if s.public {
            exports.push((s.name.clone(), Export::Function(s.clone())));
        }
    }
    for line in src.lines() {
        let t = line.trim_start();
        for (kw, public) in [
            ("pub stick ", true),
            ("pub roll ", true),
            ("stick ", false),
            ("roll ", false),
        ] {
            if let Some(rest) = t.strip_prefix(kw) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() && !declared.contains(&name) {
                    declared.push(name.clone());
                    if public {
                        exports.push((name, Export::Value));
                    }
                }
            }
        }
    }
    (exports, declared)
}

/// Move a token read from a piece of the file to where the piece sits.
fn shift(t: &mut Token, by: usize) {
    t.span.start += by;
    t.span.end += by;
    if let T::Str(parts) = &mut t.kind {
        for p in parts.iter_mut() {
            if let StrPart::Interp { span, .. } = p {
                span.start += by;
                span.end += by;
            }
        }
    }
}

/// A check that `var` fits `t`, as a statement: it coughs (E1210, or E1510
/// for a sized number out of range) where it is written.
pub fn guard(var: &str, t: &Ty, what: &str) -> String {
    let d = ty::descriptor(t);
    format!("if not _cg_fits({var}, {d}) {{ cough _cg_why({var}, {d}, \"{what}\") }}")
}

/// The full-strength part of a type, if it has one.
fn full_only(t: &Ty) -> Option<&'static str> {
    match t {
        Ty::Sized(_) => Some("sized numbers"),
        Ty::Bytes => Some("raw bytes"),
        Ty::Channel(_) => Some("channels"),
        Ty::List(a) | Ty::ListView(a) | Ty::Map(a) | Ty::Opt(a) => full_only(a),
        Ty::Fn(ps, r) => ps.iter().find_map(full_only).or_else(|| full_only(r)),
        Ty::Record(_, a) | Ty::Enum(_, a) => a.iter().find_map(full_only),
        _ => None,
    }
}

/// "E1203 message" -> ("E1203", "message").
fn split_code(m: &str) -> (&'static str, String) {
    const CODES: &[&str] = &[
        "E1104", "E1201", "E1202", "E1203", "E1204", "E1205", "E1206", "E1207", "E1208", "E1209",
        "E1211", "E1212", "E1213", "E1214", "E1215", "E1216", "E1217", "E1218", "E1219", "E1301",
        "E1302", "E1303", "E1304", "E1311", "E1312", "E1313", "E1401", "E1402", "E1403", "E1404",
        "E1405", "E1501", "E1502",
    ];
    for c in CODES {
        if let Some(rest) = m.strip_prefix(c) {
            return (c, rest.trim_start().to_string());
        }
    }
    ("E1202", m.to_string())
}
