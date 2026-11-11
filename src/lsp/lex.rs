// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! A lexical reading of one file for the editor: where each name is
//! declared, where it can be seen, and what a name at a position means.
//!
//! This is not a parser. The tokens are cigscript's own (`lower::mask`, the
//! same lexer the checker reads a file with); this walks them once, tracking
//! braces, and notes the words that declare a name. Everything about types
//! comes from the checker, never from here.

use std::collections::BTreeSet;

use cigscript::syntax::token::{Token, TokenKind as T};

use crate::gate::name_only;

/// What kind of name a declaration makes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeclKind {
    Stick,
    Roll,
    Pull,
    Record,
    Enum,
    Variant,
    Field,
    Param,
    ForVar,
    Catch,
    /// `use tools.plan` binds `plan`.
    Module(Vec<String>),
    /// `use tools.plan.{f}` binds `f`.
    Import(Vec<String>),
}

/// One declared name.
#[derive(Clone, Debug)]
pub struct Decl {
    pub name: String,
    pub kind: DeclKind,
    /// The name's own token.
    pub start: usize,
    pub end: usize,
    /// The keyword that declares it (`stick`, `pull`, `record`), or the name.
    pub kw: usize,
    /// Where the name can be used.
    pub scope: (usize, usize),
    pub public: bool,
    /// A field or variant's type, or a parameter's function.
    pub owner: Option<usize>,
    /// A parameter's position.
    pub index: usize,
}

impl Decl {
    /// Can a bare name reach it? Fields and variants are reached through
    /// their owner.
    pub fn visible(&self) -> bool {
        !matches!(self.kind, DeclKind::Field | DeclKind::Variant)
    }
}

/// What a name at a place refers to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Decl(usize),
    /// A function of a `use`d module, by the module's path and its name.
    ModuleFn(Vec<String>, String),
    Module(Vec<String>),
    /// `receiver.name`, the receiver written as a chain of names.
    Member(Vec<String>, String),
}

/// One file's tokens and declarations.
pub struct Lex {
    pub tokens: Vec<Token>,
    pub decls: Vec<Decl>,
    name_only: Vec<bool>,
}

struct Frame {
    start: usize,
    decls: Vec<usize>,
}

fn kind(tokens: &[Token], i: usize) -> Option<&T> {
    tokens.get(i).map(|t| &t.kind)
}

fn word(tokens: &[Token], i: usize) -> Option<&str> {
    match kind(tokens, i) {
        Some(T::Ident(s)) => Some(s),
        _ => None,
    }
}

impl Lex {
    pub fn new(text: &str, tokens: Vec<Token>) -> Lex {
        let decls = scan(text, &tokens);
        let name_only = name_only(&tokens);
        Lex {
            tokens,
            decls,
            name_only,
        }
    }

    /// The token a cursor is on, or just after the end of.
    pub fn token_at(&self, off: usize) -> Option<usize> {
        let i = self.tokens.partition_point(|t| t.span.start <= off);
        let mut best = None;
        // Zero-width tokens (a newline, the end) can sit between a word and
        // the cursor at its end, so look back past them.
        for k in (0..i).rev() {
            let t = &self.tokens[k];
            if t.span.end < off && t.span.end > t.span.start {
                break;
            }
            if matches!(t.kind, T::Ident(_)) && t.span.start <= off && off <= t.span.end {
                // One the cursor is inside beats one it only touches.
                if best.is_none() || off < t.span.end {
                    best = Some(k);
                }
            }
        }
        best
    }

    pub fn name(&self, i: usize) -> Option<&str> {
        word(&self.tokens, i)
    }

    fn after_dot(&self, i: usize) -> bool {
        i > 0 && matches!(self.tokens[i - 1].kind, T::Dot | T::SafeDot)
    }

    /// The declaration a bare name at `off` means.
    pub fn resolve(&self, off: usize, name: &str) -> Option<usize> {
        if let Some(d) = self
            .decls
            .iter()
            .position(|d| d.start <= off && off <= d.end && d.name == name)
        {
            return Some(d);
        }
        self.visible_at(off)
            .filter(|&d| self.decls[d].name == name)
            .max_by_key(|&d| (self.decls[d].scope.0, self.decls[d].start))
    }

    /// Every declaration that can be used at `off`.
    pub fn visible_at(&self, off: usize) -> impl Iterator<Item = usize> + '_ {
        self.decls.iter().enumerate().filter_map(move |(i, d)| {
            (d.visible() && d.scope.0 <= off && off < d.scope.1).then_some(i)
        })
    }

    /// The names written before a `.`, outermost first: `a.b` before the
    /// dot in `a.b.c`. `dot` is the dot's token index.
    pub fn receiver_chain(&self, dot: usize) -> Vec<String> {
        let mut out = Vec::new();
        let mut i = dot;
        while i > 0 {
            let Some(w) = self.name(i - 1) else { break };
            out.push(w.to_string());
            if i >= 2 && matches!(self.tokens[i - 2].kind, T::Dot | T::SafeDot) {
                i -= 2;
            } else {
                break;
            }
        }
        out.reverse();
        out
    }

    /// What the name token `i` refers to, as far as the words alone say.
    pub fn target_at(&self, i: usize) -> Option<Target> {
        let name = self.name(i)?.to_string();
        if self.after_dot(i) {
            let chain = self.receiver_chain(i - 1);
            if chain.is_empty() {
                return None;
            }
            if chain.len() == 1 {
                let first = self.tokens[i - 2].span.start;
                if let Some(d) = self.resolve(first, &chain[0]) {
                    if let DeclKind::Module(parts) = &self.decls[d].kind {
                        return Some(Target::ModuleFn(parts.clone(), name));
                    }
                }
            }
            return Some(Target::Member(chain, name));
        }
        if self.name_only[i] {
            return None;
        }
        let d = self.resolve(self.tokens[i].span.start, &name)?;
        Some(match &self.decls[d].kind {
            DeclKind::Import(parts) => Target::ModuleFn(parts.clone(), name),
            DeclKind::Module(parts) => Target::Module(parts.clone()),
            _ => Target::Decl(d),
        })
    }

    /// The index of a declaration whose name token is token `i`.
    pub fn declared_at(&self, i: usize) -> Option<usize> {
        let t = &self.tokens[i].span;
        self.decls
            .iter()
            .position(|d| d.start == t.start && d.end == t.end)
    }

    /// A top-level function, record or enum of this name.
    pub fn top_level(&self, name: &str) -> Option<usize> {
        self.decls.iter().position(|d| {
            d.name == name && matches!(d.kind, DeclKind::Pull | DeclKind::Record | DeclKind::Enum)
        })
    }

    /// Is this token a name being declared or merely used? Used for the
    /// "include the declaration" choice of a references request.
    pub fn is_declaration(&self, i: usize) -> bool {
        self.declared_at(i).is_some()
    }
}

fn stmt_start(tokens: &[Token], i: usize) -> bool {
    let mut p = i;
    if p > 0 && word(tokens, p - 1) == Some("pub") {
        p -= 1;
    }
    p == 0
        || matches!(
            tokens[p - 1].kind,
            T::Newline | T::Semicolon | T::LBrace | T::RBrace
        )
}

/// The index of the token that closes the bracket opened at `open`.
fn matching(tokens: &[Token], open: usize) -> Option<usize> {
    let (o, c) = match tokens.get(open)?.kind {
        T::LParen => (T::LParen, T::RParen),
        T::LBrace => (T::LBrace, T::RBrace),
        T::Lt => (T::Lt, T::Gt),
        _ => return None,
    };
    let mut depth = 0usize;
    for (k, t) in tokens.iter().enumerate().skip(open) {
        if t.kind == o {
            depth += 1;
        } else if t.kind == c {
            depth -= 1;
            if depth == 0 {
                return Some(k);
            }
        }
    }
    None
}

fn scan(text: &str, tokens: &[Token]) -> Vec<Decl> {
    let mut decls: Vec<Decl> = Vec::new();
    let mut frames: Vec<Frame> = vec![Frame {
        start: 0,
        decls: Vec::new(),
    }];
    let mut pending: Vec<usize> = Vec::new();
    let line_end = |from: usize| {
        tokens[from..]
            .iter()
            .find(|t| matches!(t.kind, T::Newline | T::Eof))
            .map(|t| t.span.start)
            .unwrap_or(text.len())
    };
    let mut i = 0;
    while i < tokens.len() {
        let t = &tokens[i];
        let public = i > 0 && word(tokens, i - 1) == Some("pub");
        match &t.kind {
            T::LBrace => {
                for &d in &pending {
                    decls[d].scope.0 = t.span.start;
                }
                frames.push(Frame {
                    start: t.span.start,
                    decls: std::mem::take(&mut pending),
                });
            }
            T::RBrace => {
                if frames.len() > 1 {
                    for d in frames.pop().map(|f| f.decls).unwrap_or_default() {
                        decls[d].scope.1 = t.span.end;
                    }
                }
            }
            T::Stick | T::Roll => {
                if let Some(n) = word(tokens, i + 1) {
                    let nt = &tokens[i + 1].span;
                    decls.push(Decl {
                        name: n.to_string(),
                        kind: if t.kind == T::Stick {
                            DeclKind::Stick
                        } else {
                            DeclKind::Roll
                        },
                        start: nt.start,
                        end: nt.end,
                        kw: t.span.start,
                        scope: (nt.end, usize::MAX),
                        public,
                        owner: None,
                        index: 0,
                    });
                    frames.last_mut().unwrap().decls.push(decls.len() - 1);
                }
            }
            T::For => {
                if let Some(n) = word(tokens, i + 1) {
                    let nt = &tokens[i + 1].span;
                    decls.push(Decl {
                        name: n.to_string(),
                        kind: DeclKind::ForVar,
                        start: nt.start,
                        end: nt.end,
                        kw: t.span.start,
                        scope: (nt.end, usize::MAX),
                        public: false,
                        owner: None,
                        index: 0,
                    });
                    pending.push(decls.len() - 1);
                }
            }
            T::Ashtray => {
                if let Some(n) = word(tokens, i + 1) {
                    let nt = &tokens[i + 1].span;
                    decls.push(Decl {
                        name: n.to_string(),
                        kind: DeclKind::Catch,
                        start: nt.start,
                        end: nt.end,
                        kw: t.span.start,
                        scope: (nt.end, usize::MAX),
                        public: false,
                        owner: None,
                        index: 0,
                    });
                    pending.push(decls.len() - 1);
                }
            }
            T::Pull | T::Pack => {
                let is_pull = t.kind == T::Pull;
                let mut j = i + 1;
                let mut owner = None;
                if is_pull {
                    if let Some(n) = word(tokens, i + 1) {
                        let nt = &tokens[i + 1].span;
                        decls.push(Decl {
                            name: n.to_string(),
                            kind: DeclKind::Pull,
                            start: nt.start,
                            end: nt.end,
                            kw: t.span.start,
                            scope: (frames.last().map_or(0, |f| f.start), usize::MAX),
                            public,
                            owner: None,
                            index: 0,
                        });
                        frames.last_mut().unwrap().decls.push(decls.len() - 1);
                        owner = Some(decls.len() - 1);
                        j = i + 2;
                    } else {
                        i += 1;
                        continue;
                    }
                    if kind(tokens, j) == Some(&T::Lt) {
                        j = matching(tokens, j).map(|c| c + 1).unwrap_or(j + 1);
                    }
                }
                if kind(tokens, j) == Some(&T::LParen) {
                    let close = matching(tokens, j).unwrap_or(tokens.len() - 1);
                    let mut depth = 0usize;
                    let mut index = 0;
                    let mut params = Vec::new();
                    for k in j..close {
                        match &tokens[k].kind {
                            T::LParen | T::LBracket | T::LBrace | T::Lt => depth += 1,
                            T::RParen | T::RBracket | T::RBrace | T::Gt => {
                                depth = depth.saturating_sub(1)
                            }
                            T::Ident(n)
                                if depth == 1
                                    && matches!(
                                        kind(tokens, k + 1),
                                        Some(T::Colon | T::Comma | T::RParen)
                                    )
                                    && matches!(
                                        kind(tokens, k - 1),
                                        Some(T::LParen | T::Comma | T::Ident(_))
                                    ) =>
                            {
                                let nt = &tokens[k].span;
                                decls.push(Decl {
                                    name: n.clone(),
                                    kind: DeclKind::Param,
                                    start: nt.start,
                                    end: nt.end,
                                    kw: nt.start,
                                    scope: (nt.end, usize::MAX),
                                    public: false,
                                    owner,
                                    index,
                                });
                                params.push(decls.len() - 1);
                                index += 1;
                            }
                            _ => {}
                        }
                    }
                    if is_pull
                        || (kind(tokens, close + 1) == Some(&T::FatArrow)
                            && kind(tokens, close + 2) == Some(&T::LBrace))
                    {
                        pending.extend(params);
                    } else {
                        let end = line_end(close);
                        for p in params {
                            decls[p].scope.1 = end;
                        }
                    }
                    i = close + 1;
                    continue;
                }
            }
            T::Ident(w) if (w == "record" || w == "enum") && stmt_start(tokens, i) => {
                if let Some(n) = word(tokens, i + 1) {
                    let nt = &tokens[i + 1].span;
                    let is_record = w == "record";
                    decls.push(Decl {
                        name: n.to_string(),
                        kind: if is_record {
                            DeclKind::Record
                        } else {
                            DeclKind::Enum
                        },
                        start: nt.start,
                        end: nt.end,
                        kw: t.span.start,
                        scope: (frames.last().map_or(0, |f| f.start), usize::MAX),
                        public,
                        owner: None,
                        index: 0,
                    });
                    let me = decls.len() - 1;
                    frames.last_mut().unwrap().decls.push(me);
                    let mut j = i + 2;
                    if kind(tokens, j) == Some(&T::Lt) {
                        j = matching(tokens, j).map(|c| c + 1).unwrap_or(j + 1);
                    }
                    if kind(tokens, j) == Some(&T::LBrace) {
                        let close = matching(tokens, j).unwrap_or(tokens.len() - 1);
                        let mut parens = 0usize;
                        let mut index = 0;
                        for k in j + 1..close {
                            match &tokens[k].kind {
                                T::LParen | T::LBracket | T::Lt => parens += 1,
                                T::RParen | T::RBracket | T::Gt => {
                                    parens = parens.saturating_sub(1)
                                }
                                T::Ident(f)
                                    if parens == 0
                                        && matches!(
                                            kind(tokens, k - 1),
                                            Some(T::LBrace | T::Comma | T::Newline)
                                        ) =>
                                {
                                    let is_field = kind(tokens, k + 1) == Some(&T::Colon);
                                    if is_field == is_record {
                                        let ft = &tokens[k].span;
                                        decls.push(Decl {
                                            name: f.clone(),
                                            kind: if is_record {
                                                DeclKind::Field
                                            } else {
                                                DeclKind::Variant
                                            },
                                            start: ft.start,
                                            end: ft.end,
                                            kw: ft.start,
                                            scope: (0, 0),
                                            public: false,
                                            owner: Some(me),
                                            index,
                                        });
                                        index += 1;
                                    }
                                }
                                _ => {}
                            }
                        }
                        i = close + 1;
                        continue;
                    }
                }
            }
            T::Ident(w) if w == "use" && stmt_start(tokens, i) => {
                let mut parts: Vec<String> = Vec::new();
                let mut last = None;
                let mut j = i + 1;
                let mut names: Vec<usize> = Vec::new();
                while let Some(n) = word(tokens, j) {
                    parts.push(n.to_string());
                    last = Some(j);
                    j += 1;
                    if kind(tokens, j) == Some(&T::Dot) {
                        if kind(tokens, j + 1) == Some(&T::LBrace) {
                            let close = matching(tokens, j + 1).unwrap_or(tokens.len() - 1);
                            for k in j + 2..close {
                                if word(tokens, k).is_some()
                                    && matches!(
                                        kind(tokens, k - 1),
                                        Some(T::LBrace | T::Comma | T::Newline)
                                    )
                                {
                                    names.push(k);
                                }
                            }
                            j = close + 1;
                            break;
                        }
                        j += 1;
                    } else {
                        break;
                    }
                }
                if let Some(l) = last {
                    let nt = &tokens[l].span;
                    decls.push(Decl {
                        name: parts[parts.len() - 1].clone(),
                        kind: DeclKind::Module(parts.clone()),
                        start: nt.start,
                        end: nt.end,
                        kw: t.span.start,
                        scope: (0, usize::MAX),
                        public: false,
                        owner: None,
                        index: 0,
                    });
                    for k in names {
                        let ft = &tokens[k].span;
                        decls.push(Decl {
                            name: word(tokens, k).unwrap_or("").to_string(),
                            kind: DeclKind::Import(parts.clone()),
                            start: ft.start,
                            end: ft.end,
                            kw: t.span.start,
                            scope: (0, usize::MAX),
                            public: false,
                            owner: None,
                            index: 0,
                        });
                    }
                    i = j;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    decls
}

/// Every name that is a declaration or use of `name` can be found by this.
pub fn words_named<'a>(lex: &'a Lex, name: &'a str) -> impl Iterator<Item = usize> + 'a {
    lex.tokens
        .iter()
        .enumerate()
        .filter_map(move |(i, t)| matches!(&t.kind, T::Ident(n) if n == name).then_some(i))
}

/// The set of keywords a file at a strength can write.
pub fn keywords(strength: crate::features::Strength) -> BTreeSet<&'static str> {
    use crate::features::{Detect, FEATURES};
    let mut out: BTreeSet<&'static str> = [
        "roll", "stick", "pull", "pack", "snuff", "exhale", "cough", "try", "ashtray", "finally",
        "burn", "unlit", "chain", "if", "else", "while", "for", "in", "break", "continue", "and",
        "or", "not", "true", "false", "null",
    ]
    .into_iter()
    .collect();
    for f in FEATURES {
        if let Detect::Word(w) = f.detect {
            if f.strength <= strength && f.built {
                out.insert(w);
            }
        }
    }
    out
}
