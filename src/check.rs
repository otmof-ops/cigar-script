// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Checking one file: the pipeline cigscript's `check` sits inside.
//!
//! A `.cig` file goes straight to cigscript's parse + check. A `.cigar`
//! file has its `strength` declaration read and blanked, and the blanked
//! source scanned for features above the effective strength. If the scan
//! finds anything, we stop there. A mild file then goes to cigscript as it
//! is. A medium or full file is lowered to CigScript (docs/LANGUAGE.md,
//! "How a file is checked and run"):
//!
//! 1. lowering rewrites its constructs in place, keeping every line;
//! 2. cig's parser reads the lowered text;
//! 3. the type checker and, at full, the ownership checker read cig's AST;
//! 4. what the types decided (layers, `impl`s, range checks) is applied;
//! 5. cig's own check reads the result, exactly as `cig check` would.
//!
//! Every diagnostic is reported where it is in the file as written.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use cigscript::check::Report;
use cigscript::diagnostics::{Diagnostic, Kind as DK};
use cigscript::syntax::span::Span;

use crate::features::Strength;
use crate::lower::{self, Module};
use crate::{gate, own, resolve, strength, typeck, types};

pub use crate::typeck::lint::Deny;

/// Which language the file is: `.cig` is always mild CigScript; `.cigar`
/// has a declared strength.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Cig,
    Cigar,
}

impl Kind {
    pub fn from_path(path: &Path) -> Option<Kind> {
        match path.extension().and_then(|e| e.to_str()) {
            Some("cig") => Some(Kind::Cig),
            Some("cigar") => Some(Kind::Cigar),
            _ => None,
        }
    }
}

/// The outcome of a check: the effective strength, the CigScript cig
/// checks (and runs), the source as written, and all the diagnostics.
pub struct Checked {
    pub kind: Kind,
    pub strength: Strength,
    /// What cigscript saw: the file with its declaration blanked (mild), or
    /// lowered (medium and full). This is what `cigar run` runs.
    pub source_as_cigscript_saw_it: String,
    /// The file as written. Every diagnostic's span and line are measured
    /// against this.
    pub source_as_written: String,
    pub errors: Vec<Diagnostic>,
    pub warnings: Vec<Diagnostic>,
    /// What the deeper checker finds in a `.cig` file, which keeps
    /// CigScript's exact acceptance: each error it would give at strength
    /// medium, as a hint (issue #1). Always empty for a `.cigar` file.
    pub hints: Vec<Diagnostic>,
    /// The declarations and signatures lowering found (medium and full).
    pub program: types::Program,
    /// Where each place in `source_as_cigscript_saw_it` was written, when the
    /// file was lowered (medium and full).
    pub map: Option<SourceMap>,
}

/// From the CigScript a file was lowered to, back to the file as written.
pub struct SourceMap {
    lowered: lower::Lowered,
    resolved: crate::lower::edits::Applied,
    /// Which layer each `into` and `measure` chose.
    pub layers: Vec<typeck::LayerNote>,
}

impl SourceMap {
    /// The offset in the file as written that an offset in the lowered text
    /// came from; `None` for text appended after the file (declarations,
    /// modules, the prelude).
    pub fn original(&self, p: usize) -> Option<usize> {
        let l = self.resolved.original(p);
        (l < self.lowered.appended_at()).then(|| self.lowered.original(l))
    }

    /// Was the text at an offset in the lowered text written in the file,
    /// rather than by the lowering?
    pub fn written(&self, p: usize) -> bool {
        self.resolved.copied(p) && self.lowered.copied(self.resolved.original(p))
    }
}

impl SourceMap {
    /// Every `within` block in the file as written.
    pub fn withins(&self) -> &[lower::WithinNote] {
        &self.lowered.withins
    }
}

impl Checked {
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Check one file that has no path: its modules cannot be found.
///
/// `source` is the file's whole content. `kind` is which language the
/// file is. `override_strength`, when set, forces the effective strength
/// (for `--strength X`); a `.cig` file ignores it, because `.cig` is
/// always mild.
pub fn check_source(kind: Kind, source: &str, override_strength: Option<Strength>) -> Checked {
    check_file_source(kind, source, override_strength, None)
}

/// Check one file, read from `file`: its modules are found from its folder.
pub fn check_file_source(
    kind: Kind,
    source: &str,
    override_strength: Option<Strength>,
    file: Option<&Path>,
) -> Checked {
    check_file_source_deny(kind, source, override_strength, file, Deny::default())
}

/// `check_file_source`, with some warnings denied (`--deny any`,
/// `--deny warnings`): a `.cig` file is never held to them.
pub fn check_file_source_deny(
    kind: Kind,
    source: &str,
    override_strength: Option<Strength>,
    file: Option<&Path>,
    deny: Deny,
) -> Checked {
    let modules = RefCell::new(BTreeMap::new());
    let root = file.map(|f| match f.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    });
    let name = file
        .and_then(|f| f.file_stem())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "this file".to_string());
    let env = Env {
        file,
        root: root.as_deref(),
        stack: Vec::new(),
        modules: &modules,
        name,
        // A `.cig` file is never held to a warning (#1): its deeper check
        // is a hint, and the file is not judged by what is denied.
        deny: if kind == Kind::Cig {
            Deny::default()
        } else {
            deny
        },
    };
    match kind {
        Kind::Cig => check_cig(source, &env),
        Kind::Cigar => check_cigar(source, override_strength, &env),
    }
}

/// Where a file sits in the program being checked.
struct Env<'a> {
    file: Option<&'a Path>,
    root: Option<&'a Path>,
    stack: Vec<(String, PathBuf)>,
    modules: &'a RefCell<BTreeMap<String, Module>>,
    name: String,
    deny: Deny,
}

/// What a `.cig` file says in a hint, ahead of what the deeper checker said.
pub const HINT_PREFIX: &str = "this would be an error at strength medium: ";

fn check_cig(source: &str, env: &Env) -> Checked {
    let (errors, warnings) = parse_and_check(source);
    let hints = medium_hints(source, &errors, env);
    Checked {
        kind: Kind::Cig,
        strength: Strength::Mild,
        source_as_cigscript_saw_it: source.to_string(),
        source_as_written: source.to_string(),
        errors,
        warnings,
        hints,
        program: types::Program::default(),
        map: None,
    }
}

/// A `.cig` file never gets a new error (issue #1): the file is checked
/// again as medium CigarScript, and each error that finds, which CigScript
/// did not give, comes back as a hint at the place it is. The file is not
/// changed by it, and what runs is what `cig run` runs.
fn medium_hints(source: &str, mild_errors: &[Diagnostic], env: &Env) -> Vec<Diagnostic> {
    // A file CigScript cannot read has its error already; nothing deeper to say.
    if !mild_errors.is_empty() {
        return Vec::new();
    }
    let deeper = check_cigar(source, Some(Strength::Medium), env);
    deeper
        .errors
        .into_iter()
        .map(|mut d| {
            d.message = format!("{HINT_PREFIX}{}", d.message);
            d
        })
        .collect()
}

fn check_cigar(source: &str, override_strength: Option<Strength>, env: &Env) -> Checked {
    let done = |strength, text: String, mut errors: Vec<Diagnostic>, warnings, program, map| {
        // One list in the order the findings sit in the file: a late
        // declaration's E1103 among the features it came after.
        errors.sort_by_key(|d: &Diagnostic| (d.line.unwrap_or(0), d.col.unwrap_or(0)));
        Checked {
            kind: Kind::Cigar,
            strength,
            source_as_cigscript_saw_it: text,
            source_as_written: source.to_string(),
            errors,
            warnings,
            hints: Vec::new(),
            program,
            map,
        }
    };
    // Reading the declaration can itself fail at lex time; a lex error
    // is reported as the only finding.
    let open = crate::phases::begin();
    let read = strength::read(source);
    open.end("lex");
    let decl = match read {
        Ok(d) => d,
        Err(d) => {
            return done(
                Strength::Mild,
                source.to_string(),
                vec![d],
                Vec::new(),
                Default::default(),
                None,
            )
        }
    };
    let effective = override_strength.unwrap_or(decl.strength);
    // Any E1103 the reader raised comes first (they are at the top of
    // the file by definition).
    let mut errors = decl.errors;
    // What the gate warns of (W1234) goes with the file wherever it ends.
    let mut gate_warnings = Vec::new();
    // A mild file may `use` a module (#7, DECISIONS 20): the module is
    // checked at its own strength, and what the file passes it is checked
    // when it runs. `use` is the one medium word a mild file may write.
    let mut mild_uses_modules = false;
    let open = crate::phases::begin();
    let scanned = gate::scan_all(&decl.blanked, effective);
    open.end("gate");
    match scanned {
        Ok(found) => {
            for d in found.errors {
                if effective == Strength::Mild && is_modules_finding(&d) {
                    mild_uses_modules = true;
                } else {
                    errors.push(d);
                }
            }
            gate_warnings = placed(found.warnings, source);
        }
        Err(d) => errors.push(d),
    }
    if !errors.is_empty() {
        return done(
            effective,
            decl.blanked,
            errors,
            gate_warnings,
            Default::default(),
            None,
        );
    }
    if effective == Strength::Mild && !mild_uses_modules {
        // Mild is CigScript: the blanked source goes to cig as it is.
        let (errors, mut warnings) = crate::phases::phase("cig", || parse_and_check(&decl.blanked));
        warnings.extend(gate_warnings);
        warnings.sort_by_key(|d: &Diagnostic| (d.line.unwrap_or(0), d.col.unwrap_or(0)));
        return done(
            effective,
            decl.blanked,
            errors,
            warnings,
            Default::default(),
            None,
        );
    }

    // 1. Lower.
    let opts = lower::Options {
        strength: effective,
        file: env.file,
        root: env.root,
        stack: env.stack.clone(),
        modules: env.modules,
        name: env.name.clone(),
        compile: &compile_module,
    };
    let lowered = crate::phases::phase("lower", || lower::lower(&decl.blanked, &opts));
    if !lowered.diags.is_empty() {
        let errors = placed(lowered.diags, source);
        return done(
            effective,
            lowered.text,
            errors,
            Vec::new(),
            lowered.program,
            None,
        );
    }
    let program = lowered.program.clone();

    // 2. cig reads the lowered text.
    let open = crate::phases::begin();
    let parsed = cigscript::syntax::parse(&lowered.text);
    open.end("cig");
    let ast = match parsed {
        Ok(a) => a,
        Err(d) => {
            let late = d.span.is_some_and(|s| s.start >= lowered.appended_at());
            let d = mapped(d, &|p| lowered.original(p));
            let d = if late { internal(d) } else { d };
            return done(
                effective,
                lowered.text,
                placed(vec![d], source),
                Vec::new(),
                program,
                None,
            );
        }
    };

    // 3. Types and ownership.
    let original = |p: usize| lowered.original(p);
    let out = crate::phases::phase("typeck", || {
        typeck::check_all_strength(&ast, &lowered.program, &original, source, Some(effective))
    });
    let mut errors = out.errors;
    let mut warnings = out.warnings;
    // What a strict checker says that is not a type error (#22, L1-L4).
    let written = |p: usize| lowered.copied(p);
    let found = crate::phases::phase("typeck", || {
        typeck::lint::run(
            &ast,
            &lowered.program,
            &lowered.text,
            effective,
            env.deny,
            &original,
            &written,
        )
    });
    errors.extend(found.errors);
    warnings.extend(found.warnings);
    if effective == Strength::Full {
        errors.extend(crate::phases::phase("own", || {
            own::check(&ast, &lowered.program, &original)
        }));
    } else if effective == Strength::Medium && own::crosses(&lowered.program) {
        // A medium file's calls into full functions (#22 D3).
        errors.extend(crate::phases::phase("own", || {
            own::boundary(&ast, &lowered.program, &original)
        }));
    }

    // 4. What the types decided.
    let applied =
        crate::phases::phase("typeck", || resolve::apply(&lowered.text, &out.resolutions));
    let text = applied.text.clone();

    // 5. cig's own check, of what will run.
    let back = |p: usize| lowered.original(applied.original(p));
    let appended = |p: usize| applied.original(p) >= lowered.appended_at();
    let open = crate::phases::begin();
    let parsed = cigscript::syntax::parse(&text);
    open.end("cig");
    match parsed {
        Err(d) => {
            let at = d.span.map(|s| s.start).unwrap_or(0);
            let d = mapped(d, &back);
            errors.push(if appended(at) { internal(d) } else { d });
        }
        Ok(ast2) => {
            let rep: Report = crate::phases::phase("cig", || cigscript::check::check(&ast2, &[]));
            let modules_failed = env.modules.borrow().values().any(|m| !m.errors.is_empty());
            for d in rep.errors {
                let at = d.span.map(|s| s.start);
                match at {
                    // In the appended text: a module's (reported as its
                    // own), or the lowering's, which is a bug.
                    Some(p) if appended(p) => {
                        if !modules_failed {
                            errors.push(internal(mapped(d, &back)));
                        }
                    }
                    _ => {
                        let d = mapped(d, &back);
                        // cig and CigarScript can see the same fault (an
                        // assignment to a stick): report it once, as ours.
                        let start = d.span.map(|s| s.start);
                        if !errors
                            .iter()
                            .any(|e| e.span.map(|s| s.start) == start && start.is_some())
                        {
                            errors.push(d);
                        }
                    }
                }
            }
            for w in rep.warnings {
                // A warning about text the lowering wrote is CigarScript's
                // to give (it gives its own, where it applies).
                let generated = w.span.is_some_and(|s| {
                    appended(s.start)
                        || !applied.copied(s.start)
                        || !lowered.copied(applied.original(s.start))
                });
                if !generated {
                    warnings.push(mapped(w, &back));
                }
            }
        }
    }
    let mut errors = placed(errors, source);
    let mut warnings = placed(warnings, source);
    warnings.extend(gate_warnings);
    if env.deny.warnings {
        // `--deny warnings`: what would only be said is an error.
        for mut w in warnings.drain(..) {
            let note = "an error because of --deny warnings";
            w.hint = Some(match w.hint.take() {
                Some(h) => format!("{h} ({note})"),
                None => note.to_string(),
            });
            errors.push(w);
        }
    }
    warnings.sort_by_key(|d: &Diagnostic| (d.line.unwrap_or(0), d.col.unwrap_or(0)));
    let map = SourceMap {
        lowered,
        resolved: applied,
        layers: out.layers,
    };
    done(effective, text, errors, warnings, program, Some(map))
}

/// Is this the gate's finding for the word `use`? A mild file may write
/// it (modules mix across strengths), so the finding is not an error there.
fn is_modules_finding(d: &Diagnostic) -> bool {
    use crate::features::{Detect, FEATURES};
    d.code == "E1101"
        && FEATURES
            .iter()
            .filter(|f| matches!(f.detect, Detect::Word("use")))
            .any(|f| d.message.starts_with(&format!("using {} needs", f.name)))
}

/// Compile a module: what `use` needs from it. Its errors are its own,
/// measured against its own file.
fn compile_module(path: &Path, name: &str, src: &str, opts: &lower::Options) -> Module {
    let env = Env {
        file: Some(path),
        root: opts.root,
        stack: opts.stack.clone(),
        modules: opts.modules,
        name: name.to_string(),
        deny: Deny::default(),
    };
    let c = check_cigar(src, None, &env);
    let text = &c.source_as_cigscript_saw_it;
    let (body, decls) = match (
        text.find(lower::APPENDED_MARK),
        text.find(lower::PROGRAM_MARK),
    ) {
        (Some(a), Some(p)) if a <= p => (
            text[..a].to_string(),
            text[a + lower::APPENDED_MARK.len()..p].to_string(),
        ),
        _ => (text.clone(), String::new()),
    };
    let (exports, declared) = lower::module_names(src, &c.program);
    Module {
        name: name.to_string(),
        path: path.to_path_buf(),
        body,
        decls,
        program: c.program,
        exports,
        declared,
        errors: c.errors,
    }
}

fn parse_and_check(source: &str) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    match cigscript::syntax::parse(source) {
        Ok(program) => {
            let rep: Report = cigscript::check::check(&program, &[]);
            (rep.errors, rep.warnings)
        }
        Err(d) => (vec![d], Vec::new()),
    }
}

/// A diagnostic's span moved from the text cig read to the file as written.
fn mapped(mut d: Diagnostic, back: &dyn Fn(usize) -> usize) -> Diagnostic {
    if let Some(s) = d.span {
        let (a, b) = (back(s.start), back(s.end));
        d.span = Some(Span::new(a, b.max(a), s.line, s.col));
    }
    d
}

/// Where a bug in cigar is reported. When `ciginstaller` has a `bug`
/// command, `ciginstaller bug cigar` becomes the way and this stays the
/// fallback.
pub const ISSUES_URL: &str = "https://github.com/otmof-ops/cigar-script/issues";

/// cig refused CigScript the lowering made: a bug in CigarScript, said so.
fn internal(d: Diagnostic) -> Diagnostic {
    let mut i = Diagnostic::new(
        DK::Internal,
        format!(
            "CigarScript lowered this file into CigScript that cig refuses ({} {}): a bug in cigar, please report it at {ISSUES_URL}",
            d.code, d.message
        ),
    )
    .with_hint("report it at the issues page above, with the file and this message");
    i.code = "E1104";
    i.span = d.span;
    i.line = d.line;
    i.col = d.col;
    i
}

/// Every diagnostic's line and column, from its span in the file as written.
fn placed(mut ds: Vec<Diagnostic>, source: &str) -> Vec<Diagnostic> {
    let starts: Vec<usize> = std::iter::once(0)
        .chain(source.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    for d in ds.iter_mut() {
        if let Some(s) = d.span {
            let start = s.start.min(source.len());
            let line = starts.partition_point(|&b| b <= start);
            let from = starts[line - 1];
            let col = source
                .get(from..start)
                .map(|t| t.chars().count())
                .unwrap_or(0)
                + 1;
            d.span = Some(Span::new(
                start,
                s.end.min(source.len()).max(start),
                line as u32,
                col as u32,
            ));
            d.line = Some(line as u32);
            d.col = Some(col as u32);
        }
    }
    ds
}
