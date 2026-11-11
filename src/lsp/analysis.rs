// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! What the server knows about one open document at one version: the
//! checker's findings, its types, and a lexical reading of the names.

use std::collections::{BTreeMap, HashMap};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use cigscript::diagnostics::{Diagnostic, Kind as DK};
use cigscript::errors;
use cigscript::syntax::token::TokenKind as T;
use serde_json::{json, Value};

use super::lex::{keywords, words_named, Decl, DeclKind, Lex, Target};
use crate::check::{check_file_source, Checked, Kind};
use crate::explain::explain_at;
use crate::features::Strength;
use crate::types::{FnSig, Ty};

/// Where to read a module's text: an open document's, else the disk's.
pub type Reader<'a> = &'a dyn Fn(&Path) -> Option<String>;

/// A place in a file: `path` is `None` for the document itself.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Loc {
    pub path: Option<PathBuf>,
    pub start: usize,
    pub end: usize,
}

pub struct Analysis {
    pub path: Option<PathBuf>,
    pub text: String,
    pub starts: Vec<usize>,
    pub checked: Checked,
    pub lex: Lex,
    /// What the checker decided each `stick`/`roll` is, by the offset of its
    /// keyword in the file as written.
    bind: HashMap<usize, Ty>,
}

pub fn line_starts(text: &str) -> Vec<usize> {
    let mut v = vec![0];
    v.extend(text.match_indices('\n').map(|(i, _)| i + 1));
    v
}

/// A byte offset as a line and a UTF-16 column.
pub fn to_pos(text: &str, starts: &[usize], off: usize) -> (u32, u32) {
    let mut off = off.min(text.len());
    while !text.is_char_boundary(off) {
        off -= 1;
    }
    let line = starts.partition_point(|&s| s <= off) - 1;
    let col = text[starts[line]..off].encode_utf16().count();
    (line as u32, col as u32)
}

/// A line and a UTF-16 column as a byte offset (clamped to the line).
pub fn to_off(text: &str, starts: &[usize], line: u32, col: u32) -> usize {
    let Some(&s) = starts.get(line as usize) else {
        return text.len();
    };
    let end = starts.get(line as usize + 1).copied().unwrap_or(text.len());
    let mut units = 0u32;
    for (i, c) in text[s..end].char_indices() {
        if c == '\n' || c == '\r' || units >= col {
            return s + i;
        }
        units += c.len_utf16() as u32;
    }
    end
}

pub fn range(text: &str, starts: &[usize], s: usize, e: usize) -> Value {
    let (sl, sc) = to_pos(text, starts, s);
    let (el, ec) = to_pos(text, starts, e.max(s));
    json!({"start": {"line": sl, "character": sc}, "end": {"line": el, "character": ec}})
}

impl Analysis {
    pub fn build(path: Option<&Path>, kind: Kind, text: &str) -> Analysis {
        let checked = catch_unwind(AssertUnwindSafe(|| {
            check_file_source(kind, text, None, path)
        }))
        .unwrap_or_else(|_| {
            let d = Diagnostic::new(
                DK::Internal,
                "the checker stopped on this file; this is a bug in cigar, not in your file",
            );
            Checked {
                kind,
                strength: Strength::Mild,
                source_as_cigscript_saw_it: text.to_string(),
                source_as_written: text.to_string(),
                errors: vec![d],
                warnings: Vec::new(),
                hints: Vec::new(),
                program: Default::default(),
                map: None,
            }
        });
        let tokens = crate::lower::mask(text)
            .map(|(_, t, _)| t)
            .unwrap_or_default();
        let lex = Lex::new(text, tokens);
        let mut bind = HashMap::new();
        if let Some(map) = &checked.map {
            for (k, ty) in &checked.program.bindings {
                if let Some(o) = map.original(*k) {
                    bind.insert(o, ty.clone());
                }
            }
        }
        Analysis {
            path: path.map(Path::to_path_buf),
            text: text.to_string(),
            starts: line_starts(text),
            checked,
            lex,
            bind,
        }
    }

    // ---- diagnostics ----

    /// Every finding as an LSP diagnostic: errors, warnings (W12xx), and a
    /// `.cig` file's hints.
    pub fn diagnostics(&self) -> Vec<Value> {
        let all = self
            .checked
            .errors
            .iter()
            .map(|d| (d, 1))
            .chain(self.checked.warnings.iter().map(|d| (d, 2)))
            .chain(self.checked.hints.iter().map(|d| (d, 4)));
        all.map(|(d, sev)| {
            let (s, e) = self.span_of(d);
            let mut message = d.message.clone();
            if let Some(h) = &d.hint {
                message.push_str("\nhint: ");
                message.push_str(h);
            }
            json!({
                "range": range(&self.text, &self.starts, s, e),
                "severity": sev,
                "code": d.code,
                "source": "cigar",
                "message": message,
            })
        })
        .collect()
    }

    /// Where a finding is, in the file as written.
    fn span_of(&self, d: &Diagnostic) -> (usize, usize) {
        if let Some(sp) = d.span {
            let s = sp.start.min(self.text.len());
            return (s, sp.end.max(s).min(self.text.len()));
        }
        match (d.line, d.col) {
            (Some(l), c) if l >= 1 => {
                let line = (l - 1) as usize;
                let Some(&ls) = self.starts.get(line) else {
                    return (self.text.len(), self.text.len());
                };
                let le = self
                    .starts
                    .get(line + 1)
                    .copied()
                    .unwrap_or(self.text.len());
                let body = &self.text[ls..le];
                let col = c.unwrap_or(1).max(1) as usize - 1;
                let s = ls
                    + body
                        .char_indices()
                        .nth(col)
                        .map(|(i, _)| i)
                        .unwrap_or_else(|| body.trim_end_matches(['\n', '\r']).len());
                let e = body[s - ls..]
                    .char_indices()
                    .nth(1)
                    .map(|(i, _)| s + i)
                    .unwrap_or(s);
                (s, e)
            }
            _ => (0, 0),
        }
    }

    // ---- hover ----

    /// The markdown to show at `off`, and the range it is about.
    pub fn hover(&self, off: usize) -> Option<(String, (usize, usize))> {
        let mut parts: Vec<String> = Vec::new();
        let mut at = (off, off);
        let all = self
            .checked
            .errors
            .iter()
            .chain(&self.checked.warnings)
            .chain(&self.checked.hints);
        for d in all {
            let (s, e) = self.span_of(d);
            if s <= off && off < e.max(s + 1) {
                if let Some(page) = kind_of_no(d.code) {
                    if !parts.contains(&page) {
                        parts.push(page);
                        at = (s, e);
                    }
                }
            }
        }
        if let Some(map) = &self.checked.map {
            if let Some(x) = explain_at(&self.text, map, off) {
                let mut s = format!("{}\n", x.headline);
                if !x.context.is_empty() {
                    s.push_str(&format!("\ninside: {}\n", x.context.join(" > ")));
                }
                s.push_str("\n```\n");
                for step in &x.steps {
                    s.push_str(&format!("{:<7}{}\n", step.level, step.text));
                }
                s.push_str("```");
                parts.push(s);
                if let Some(l) = self
                    .checked
                    .map
                    .as_ref()
                    .and_then(|m| m.layers.iter().find(|l| l.start <= off && off < l.end))
                {
                    at = (l.start, l.end);
                }
            }
        }
        if let Some(i) = self.lex.token_at(off) {
            let t = &self.lex.tokens[i];
            let name = self.lex.name(i).unwrap_or("");
            if let Some(page) = errors::lookup(name).and_then(|_| kind_of_no(name)) {
                if !parts.contains(&page) {
                    parts.push(page);
                }
            }
            if let Some(text) = self.describe_token(i) {
                parts.push(format!("```cigar\n{text}\n```"));
            }
            if parts.len() == 1 || at == (off, off) {
                at = (t.span.start, t.span.end);
            }
        }
        if parts.is_empty() {
            None
        } else {
            Some((parts.join("\n\n---\n\n"), at))
        }
    }

    fn describe_token(&self, i: usize) -> Option<String> {
        match self.normalize(self.lex.target_at(i)?, i) {
            Target::Decl(d) => self.describe_decl(d),
            Target::ModuleFn(parts, name) => {
                let local = parts.last()?;
                let p = &self.checked.program;
                let sig = p
                    .module_fns
                    .get(&format!("{local}.{name}"))
                    .or_else(|| p.module_fns.get(&name))?;
                Some(sig_text(sig))
            }
            Target::Module(parts) => Some(format!("use {}", parts.join("."))),
            Target::Member(chain, name) => {
                let ty = self.chain_ty(self.first_off(i - 1, chain.len())?, &chain)?;
                let f = self.field_ty(&ty, &name)?;
                Some(format!("(field) {name}: {f}"))
            }
        }
    }

    fn describe_decl(&self, d: usize) -> Option<String> {
        let decl = &self.lex.decls[d];
        let p = &self.checked.program;
        match decl.kind {
            DeclKind::Pull => Some(match self.sig_of(d) {
                Some(s) => sig_text(s),
                None => {
                    let ps: Vec<&str> = self
                        .lex
                        .decls
                        .iter()
                        .filter(|x| x.kind == DeclKind::Param && x.owner == Some(d))
                        .map(|x| x.name.as_str())
                        .collect();
                    format!("pull {}({})", decl.name, ps.join(", "))
                }
            }),
            DeclKind::Record => {
                let fields = match p.records.get(&decl.name) {
                    Some(r) => r
                        .fields
                        .iter()
                        .map(|(n, t)| format!("{n}: {t}"))
                        .collect::<Vec<_>>(),
                    None => self.owned(d).map(|x| x.name.clone()).collect(),
                };
                Some(format!("record {} {{ {} }}", decl.name, fields.join(", ")))
            }
            DeclKind::Enum => {
                let vs: Vec<String> = self.owned(d).map(|x| x.name.clone()).collect();
                Some(format!("enum {} {{ {} }}", decl.name, vs.join(", ")))
            }
            DeclKind::Stick | DeclKind::Roll => {
                let ty = self.ty_of_decl(d)?;
                let kw = if decl.kind == DeclKind::Stick {
                    "stick"
                } else {
                    "roll"
                };
                Some(format!("{kw} {}: {ty}", decl.name))
            }
            DeclKind::Param => {
                let ty = self.ty_of_decl(d)?;
                Some(format!("(parameter) {}: {ty}", decl.name))
            }
            DeclKind::Field => {
                let owner = &self.lex.decls[decl.owner?];
                let ty = self.field_ty(&Ty::Record(owner.name.clone(), Vec::new()), &decl.name)?;
                Some(format!("(field) {}: {ty}", decl.name))
            }
            _ => None,
        }
    }

    fn owned(&self, d: usize) -> impl Iterator<Item = &Decl> {
        self.lex.decls.iter().filter(move |x| x.owner == Some(d))
    }

    /// The signature the checker holds for the `pull` declared at `d`.
    fn sig_of(&self, d: usize) -> Option<&FnSig> {
        let decl = &self.lex.decls[d];
        let mut named = self
            .checked
            .program
            .fns
            .values()
            .filter(|s| s.name == decl.name);
        let first = named.next()?;
        let rest: Vec<&FnSig> = named.collect();
        if rest.is_empty() {
            return Some(first);
        }
        std::iter::once(first)
            .chain(rest)
            .find(|s| s.at.start <= decl.start && decl.start < s.at.end.max(s.at.start + 1))
    }

    fn ty_of_decl(&self, d: usize) -> Option<Ty> {
        let decl = &self.lex.decls[d];
        match decl.kind {
            DeclKind::Stick | DeclKind::Roll => self.bind.get(&decl.kw).cloned(),
            DeclKind::Param => {
                let sig = self.sig_of(decl.owner?)?;
                sig.params.get(decl.index)?.ty.clone()
            }
            _ => None,
        }
    }

    fn field_ty(&self, ty: &Ty, name: &str) -> Option<Ty> {
        let ty = match ty {
            Ty::Opt(inner) => inner,
            t => t,
        };
        match ty {
            Ty::Record(rn, _) => self
                .checked
                .program
                .records
                .get(rn)?
                .fields
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, t)| t.clone()),
            _ => None,
        }
    }

    /// Where the first name of a chain of `len` names before the dot at
    /// token `dot` starts.
    fn first_off(&self, dot: usize, len: usize) -> Option<usize> {
        let first = dot.checked_sub(2 * (len - 1) + 1)?;
        Some(self.lex.tokens.get(first)?.span.start)
    }

    /// The type of `a.b`, the first name seen from `off`.
    fn chain_ty(&self, off: usize, chain: &[String]) -> Option<Ty> {
        let d = self.lex.resolve(off, chain.first()?)?;
        let mut ty = self.ty_of_decl(d)?;
        for f in &chain[1..] {
            ty = self.field_ty(&ty, f)?;
        }
        Some(ty)
    }

    /// A `Member` the types can place becomes the field's declaration.
    fn normalize(&self, t: Target, tok: usize) -> Target {
        if let Target::Member(chain, name) = &t {
            if let Some(Ty::Record(rn, _)) = self
                .first_off(tok - 1, chain.len())
                .and_then(|o| self.chain_ty(o, chain))
                .map(|t| match t {
                    Ty::Opt(i) => *i,
                    t => t,
                })
            {
                let found = self.lex.decls.iter().position(|d| {
                    d.kind == DeclKind::Field
                        && d.name == *name
                        && d.owner.is_some_and(|o| self.lex.decls[o].name == rn)
                });
                if let Some(f) = found {
                    return Target::Decl(f);
                }
            }
        }
        t
    }

    // ---- definition and references ----

    fn module_lex(&self, parts: &[String], read: Reader) -> Option<(PathBuf, Lex)> {
        let root = self.path.as_ref()?.parent()?;
        let mut path = root.to_path_buf();
        for p in parts {
            path.push(p);
        }
        path.set_extension("cigar");
        let text = read(&path)?;
        let tokens = crate::lower::mask(&text).map(|(_, t, _)| t).ok()?;
        Some((path, Lex::new(&text, tokens)))
    }

    fn target(&self, off: usize) -> Option<(Target, usize)> {
        let i = self.lex.token_at(off)?;
        let t = self.lex.target_at(i)?;
        Some((self.normalize(t, i), i))
    }

    pub fn definition(&self, off: usize, read: Reader) -> Vec<Loc> {
        let Some((target, _)) = self.target(off) else {
            return Vec::new();
        };
        match target {
            Target::Decl(d) => {
                let d = &self.lex.decls[d];
                vec![Loc {
                    path: None,
                    start: d.start,
                    end: d.end,
                }]
            }
            Target::Module(parts) => match self.module_lex(&parts, read) {
                Some((path, _)) => vec![Loc {
                    path: Some(path),
                    start: 0,
                    end: 0,
                }],
                None => Vec::new(),
            },
            Target::ModuleFn(parts, name) => match self.module_lex(&parts, read) {
                Some((path, lex)) => {
                    let (start, end) = lex
                        .top_level(&name)
                        .map(|d| (lex.decls[d].start, lex.decls[d].end))
                        .unwrap_or((0, 0));
                    vec![Loc {
                        path: Some(path),
                        start,
                        end,
                    }]
                }
                None => Vec::new(),
            },
            Target::Member(..) => Vec::new(),
        }
    }

    pub fn references(&self, off: usize, include_decl: bool, read: Reader) -> Vec<Loc> {
        let Some((target, _)) = self.target(off) else {
            return Vec::new();
        };
        let name = match &target {
            Target::Decl(d) => self.lex.decls[*d].name.clone(),
            Target::ModuleFn(_, n) | Target::Member(_, n) => n.clone(),
            Target::Module(p) => p.last().cloned().unwrap_or_default(),
        };
        let mut out: Vec<Loc> = Vec::new();
        for i in words_named(&self.lex, &name) {
            let Some(t) = self.lex.target_at(i) else {
                continue;
            };
            if self.normalize(t, i) != target {
                continue;
            }
            if !include_decl
                && matches!(target, Target::Decl(d) if self.lex.declared_at(i) == Some(d))
            {
                continue;
            }
            let sp = &self.lex.tokens[i].span;
            out.push(Loc {
                path: None,
                start: sp.start,
                end: sp.end,
            });
        }
        if let Target::ModuleFn(parts, n) = &target {
            if let Some((path, lex)) = self.module_lex(parts, read) {
                if let Some(d) = lex.top_level(n) {
                    for i in words_named(&lex, n) {
                        let here = lex.declared_at(i) == Some(d);
                        if here && !include_decl {
                            continue;
                        }
                        if here || lex.target_at(i) == Some(Target::Decl(d)) {
                            let sp = &lex.tokens[i].span;
                            out.push(Loc {
                                path: Some(path.clone()),
                                start: sp.start,
                                end: sp.end,
                            });
                        }
                    }
                }
            }
        }
        out.sort();
        out.dedup();
        out
    }

    // ---- completion ----

    pub fn is_dot_completion(&self, off: usize) -> Option<usize> {
        let tokens = &self.lex.tokens;
        let is_dot = |i: usize| matches!(tokens[i].kind, T::Dot | T::SafeDot);
        match self.lex.token_at(off) {
            Some(i) => (i > 0 && is_dot(i - 1)).then_some(i - 1),
            None => {
                let k = tokens
                    .partition_point(|t| t.span.end <= off)
                    .checked_sub(1)?;
                (is_dot(k) && tokens[k].span.end == off).then_some(k)
            }
        }
    }

    pub fn complete(&self, off: usize, read: Reader) -> Vec<Value> {
        if let Some(dot) = self.is_dot_completion(off) {
            return self.complete_members(dot, read);
        }
        let mut items: Vec<Value> = Vec::new();
        for k in keywords(self.checked.strength) {
            items.push(json!({"label": k, "kind": 14}));
        }
        let mut seen: BTreeMap<&str, usize> = BTreeMap::new();
        for d in self.lex.visible_at(off) {
            let x = &self.lex.decls[d];
            let slot = seen.entry(x.name.as_str()).or_insert(d);
            if self.lex.decls[*slot].scope.0 < x.scope.0 {
                *slot = d;
            }
        }
        let mut chosen: Vec<usize> = seen.into_values().collect();
        chosen.sort();
        for d in chosen {
            let x = &self.lex.decls[d];
            let kind = match x.kind {
                DeclKind::Pull | DeclKind::Import(_) => 3,
                DeclKind::Record => 22,
                DeclKind::Enum => 13,
                DeclKind::Module(_) => 9,
                _ => 6,
            };
            let mut item = json!({"label": x.name, "kind": kind});
            if let Some(detail) = self.describe_decl(d) {
                item["detail"] = json!(detail);
            }
            items.push(item);
        }
        items
    }

    fn complete_members(&self, dot: usize, read: Reader) -> Vec<Value> {
        let chain = self.lex.receiver_chain(dot);
        match self.first_off(dot, chain.len().max(1)) {
            Some(off) if !chain.is_empty() => self.complete_chain(&chain, off, read),
            _ => Vec::new(),
        }
    }

    /// What can follow `a.b.`: a module's public names, or a record's fields.
    /// `off` is where the first name is seen from.
    pub fn complete_chain(&self, chain: &[String], off: usize, read: Reader) -> Vec<Value> {
        let Some(first) = chain.first() else {
            return Vec::new();
        };
        let off = off.min(self.text.len());
        if chain.len() == 1 {
            if let Some(d) = self.lex.resolve(off, first) {
                if let DeclKind::Module(parts) = &self.lex.decls[d].kind {
                    return self.module_members(parts, read);
                }
            }
        }
        let Some(ty) = self.chain_ty(off, chain) else {
            return Vec::new();
        };
        let ty = match ty {
            Ty::Opt(i) => *i,
            t => t,
        };
        match &ty {
            Ty::Record(rn, _) => match self.checked.program.records.get(rn) {
                Some(r) => r
                    .fields
                    .iter()
                    .map(|(n, t)| json!({"label": n, "kind": 5, "detail": t.to_string()}))
                    .collect(),
                None => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    fn module_members(&self, parts: &[String], read: Reader) -> Vec<Value> {
        let Some((_, lex)) = self.module_lex(parts, read) else {
            return Vec::new();
        };
        lex.decls
            .iter()
            .filter(|d| {
                d.public && matches!(d.kind, DeclKind::Pull | DeclKind::Record | DeclKind::Enum)
            })
            .map(|d| {
                let kind = match d.kind {
                    DeclKind::Pull => 3,
                    DeclKind::Record => 22,
                    _ => 13,
                };
                json!({"label": d.name, "kind": kind})
            })
            .collect()
    }
}

/// `E303 title`, its kind of no, what it means and what to type next: the
/// registry's words, as `cig explain` gives them. A code cigar adds (the
/// `E11xx` to `E15xx` ranges) has no page in cig's registry yet: it gets its
/// range's kind and meaning, which is what the registry does say of it.
pub fn kind_of_no(code: &str) -> Option<String> {
    if let Some(e) = errors::lookup(code) {
        let no = e.no_entry();
        return Some(format!(
            "**{} {}** ({}, {} family, since {})\n\nkind of no: {} ({}; {})\n\n{}\n\nwhat to type next: {}",
            e.code,
            e.title,
            e.kind().as_str(),
            e.family(),
            e.since,
            no.hear,
            no.whose,
            no.manual,
            e.meaning,
            e.fix
        ));
    }
    let n = errors::number(code)?;
    let r = errors::registry().ranges.iter().find(|r| r.holds(n))?;
    Some(format!(
        "**{code}** ({}, {} family)\n\n{}\n\nThe message says all of it: this code has no page in `cig explain` yet.",
        r.kind.as_str(),
        r.family,
        r.meaning
    ))
}

fn sig_text(s: &FnSig) -> String {
    let ps: Vec<String> = s
        .params
        .iter()
        .map(|p| match &p.ty {
            Some(t) => format!("{}: {t}", p.name),
            None => p.name.clone(),
        })
        .collect();
    let mut out = format!("pull {}({})", s.name, ps.join(", "));
    if let Some(r) = &s.ret {
        out.push_str(&format!(" -> {r}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_are_utf16_units() {
        let text = "a\u{e9}\u{1F600}b\nxy";
        let starts = line_starts(text);
        // `b` is after 1 + 1 + 2 UTF-16 units.
        let b = text.find('b').unwrap();
        assert_eq!(to_pos(text, &starts, b), (0, 4));
        assert_eq!(to_off(text, &starts, 0, 4), b);
        assert_eq!(to_pos(text, &starts, text.len()), (1, 2));
        // Past the end of a line clamps to it; past the file clamps to the end.
        assert_eq!(to_off(text, &starts, 0, 99), text.find('\n').unwrap());
        assert_eq!(to_off(text, &starts, 9, 0), text.len());
    }
}
