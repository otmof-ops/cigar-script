// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar-lsp`: a Language Server Protocol server over stdio (#16).
//!
//! It reads files with the code `cigar check` does: `check_file_source` for
//! every finding and every type, cigscript's own lexer for names, and
//! `cigar explain --layers`'s reading for layers. There is no second parser.
//!
//! The loop is single-threaded for the work and two-threaded for the pipe: a
//! reader thread frames messages into a bounded queue (a client that writes
//! faster than the server checks waits in the pipe, not in memory), and the
//! main loop applies every queued edit before it checks each changed
//! document once. Opening a document twice, a stale version and a repeated
//! `shutdown` are all harmless.

pub mod analysis;
pub mod lex;
pub mod rpc;

use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc::{self, TryRecvError};

use serde_json::{json, Value};

use self::analysis::{line_starts, range, to_off, Analysis, Loc};
use self::rpc::{Frame, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, NOT_INITIALIZED};
use crate::check::Kind;

/// Messages waiting for the main loop.
const QUEUE: usize = 256;

struct Doc {
    path: Option<PathBuf>,
    kind: Kind,
    version: i64,
    text: String,
}

enum Item {
    Msg(Value),
    Garbled,
    End,
    Broken(io::Error),
}

enum Flow {
    Go,
    Exit(i32),
}

/// Run the server on `input` and `out` until `exit` or the end of the
/// stream. The result is the process's exit code: 0 after `shutdown`, 1
/// otherwise (the protocol's rule).
pub fn serve<R, W>(mut input: R, out: W) -> i32
where
    R: BufRead + Send + 'static,
    W: Write,
{
    let (tx, rx) = mpsc::sync_channel::<Item>(QUEUE);
    std::thread::spawn(move || loop {
        let item = match rpc::read_frame(&mut input) {
            Ok(Frame::Message(v)) => Item::Msg(v),
            Ok(Frame::Garbled) => Item::Garbled,
            Ok(Frame::End) => Item::End,
            Err(e) => Item::Broken(e),
        };
        let last = matches!(item, Item::End | Item::Broken(_));
        if tx.send(item).is_err() || last {
            break;
        }
    });
    let mut s = Server {
        out,
        docs: HashMap::new(),
        cache: HashMap::new(),
        good: HashMap::new(),
        dirty: BTreeSet::new(),
        initialized: false,
        shutdown: false,
    };
    loop {
        let Ok(first) = rx.recv() else {
            return s.code();
        };
        let mut next = Some(first);
        while let Some(item) = next.take() {
            match s.item(item) {
                Ok(Flow::Go) => {}
                Ok(Flow::Exit(c)) => return c,
                // The client is gone: nothing more can be said to it.
                Err(_) => return s.code(),
            }
            match rx.try_recv() {
                Ok(i) => next = Some(i),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => return s.code(),
            }
        }
        if s.flush().is_err() {
            return s.code();
        }
    }
}

struct Server<W: Write> {
    out: W,
    docs: HashMap<String, Doc>,
    /// The analysis of each document at the version it was made for.
    cache: HashMap<String, (i64, Rc<Analysis>)>,
    /// The last analysis the checker took to the end of lowering: types for
    /// completion while a half-typed line does not check.
    good: HashMap<String, Rc<Analysis>>,
    /// Documents changed since their diagnostics were last published.
    dirty: BTreeSet<String>,
    initialized: bool,
    shutdown: bool,
}

impl<W: Write> Server<W> {
    fn code(&self) -> i32 {
        if self.shutdown {
            0
        } else {
            1
        }
    }

    fn send(&mut self, v: Value) -> io::Result<()> {
        rpc::write_message(&mut self.out, &v)
    }

    fn item(&mut self, item: Item) -> io::Result<Flow> {
        match item {
            Item::Msg(v) => self.message(v),
            Item::Garbled => {
                self.send(rpc::error(&Value::Null, rpc::PARSE_ERROR, "not JSON"))?;
                Ok(Flow::Go)
            }
            Item::End => Ok(Flow::Exit(self.code())),
            Item::Broken(e) => {
                eprintln!("cigar-lsp: {e}");
                Ok(Flow::Exit(self.code()))
            }
        }
    }

    fn message(&mut self, msg: Value) -> io::Result<Flow> {
        let Some(method) = msg.get("method").and_then(Value::as_str) else {
            // A response to a request we never make, or noise.
            return Ok(Flow::Go);
        };
        let params = msg.get("params").cloned().unwrap_or(Value::Null);
        match msg.get("id") {
            Some(id) => {
                let id = id.clone();
                self.request(&id, method, &params)?;
                Ok(Flow::Go)
            }
            None => self.notification(method, &params),
        }
    }

    fn request(&mut self, id: &Value, method: &str, p: &Value) -> io::Result<()> {
        if method == "initialize" {
            if self.initialized {
                return self.send(rpc::error(id, INVALID_REQUEST, "already initialized"));
            }
            self.initialized = true;
            return self.send(rpc::response(id, capabilities()));
        }
        if !self.initialized {
            return self.send(rpc::error(id, NOT_INITIALIZED, "send initialize first"));
        }
        if method == "shutdown" {
            self.shutdown = true;
            return self.send(rpc::response(id, Value::Null));
        }
        if self.shutdown {
            return self.send(rpc::error(id, INVALID_REQUEST, "the server is shut down"));
        }
        if method == "textDocument/formatting" {
            let result = match p["textDocument"]["uri"].as_str() {
                Some(uri) => self.formatting(uri),
                None => {
                    return self.send(rpc::error(id, INVALID_PARAMS, "needs textDocument"));
                }
            };
            return self.send(rpc::response(id, result));
        }
        let Some((uri, line, ch)) = position_params(p) else {
            let known = matches!(
                method,
                "textDocument/hover"
                    | "textDocument/definition"
                    | "textDocument/references"
                    | "textDocument/completion"
            );
            return self.send(if known {
                rpc::error(id, INVALID_PARAMS, "needs textDocument and position")
            } else {
                rpc::error(id, METHOD_NOT_FOUND, method)
            });
        };
        let result = match method {
            "textDocument/hover" => self.hover(&uri, line, ch),
            "textDocument/definition" => self.definition(&uri, line, ch),
            "textDocument/references" => {
                let include = p["context"]["includeDeclaration"].as_bool().unwrap_or(true);
                self.references(&uri, line, ch, include)
            }
            "textDocument/completion" => self.completion(&uri, line, ch),
            _ => return self.send(rpc::error(id, METHOD_NOT_FOUND, method)),
        };
        self.send(rpc::response(id, result))
    }

    /// `textDocument/formatting`: the whole document as one edit, from the
    /// same formatter `cigar fmt` runs. No edits when it is already formatted;
    /// `null` when the text does not lex (nothing to lay out) or is not open.
    fn formatting(&self, uri: &str) -> Value {
        let Some(doc) = self.docs.get(uri) else {
            return Value::Null;
        };
        let Ok(out) = crate::fmt::format(&doc.text) else {
            return Value::Null;
        };
        if out == doc.text {
            return json!([]);
        }
        let end_line = doc.text.matches('\n').count();
        let last = doc.text.rsplit('\n').next().unwrap_or("");
        let end_ch: usize = last.chars().map(char::len_utf16).sum();
        json!([{
            "range": {
                "start": {"line": 0, "character": 0},
                "end": {"line": end_line, "character": end_ch},
            },
            "newText": out,
        }])
    }

    fn notification(&mut self, method: &str, p: &Value) -> io::Result<Flow> {
        match method {
            "exit" => return Ok(Flow::Exit(self.code())),
            _ if !self.initialized || self.shutdown => {}
            "textDocument/didOpen" => self.did_open(p),
            "textDocument/didChange" => self.did_change(p),
            "textDocument/didClose" => {
                if let Some(uri) = p["textDocument"]["uri"].as_str() {
                    self.did_close(uri)?;
                }
            }
            _ => {}
        }
        Ok(Flow::Go)
    }

    // ---- documents ----

    fn did_open(&mut self, p: &Value) {
        let td = &p["textDocument"];
        let (Some(uri), Some(text)) = (td["uri"].as_str(), td["text"].as_str()) else {
            return;
        };
        let path = uri_to_path(uri);
        let kind =
            path.as_deref()
                .and_then(Kind::from_path)
                .or_else(|| match td["languageId"].as_str() {
                    Some("cigar") => Some(Kind::Cigar),
                    Some("cig") => Some(Kind::Cig),
                    _ => None,
                });
        // Only a language this server knows is checked.
        let Some(kind) = kind else { return };
        self.docs.insert(
            uri.to_string(),
            Doc {
                path,
                kind,
                version: td["version"].as_i64().unwrap_or(0),
                text: text.to_string(),
            },
        );
        self.cache.remove(uri);
        self.dirty.insert(uri.to_string());
    }

    fn did_change(&mut self, p: &Value) {
        let Some(uri) = p["textDocument"]["uri"].as_str() else {
            return;
        };
        let version = p["textDocument"]["version"].as_i64();
        let Some(doc) = self.docs.get_mut(uri) else {
            return;
        };
        // An edit older than what is held has already been applied.
        if version.is_some_and(|v| v < doc.version) {
            return;
        }
        // Full sync: the last change without a range is the whole text.
        let whole = p["contentChanges"]
            .as_array()
            .and_then(|a| a.iter().rev().find(|c| c.get("range").is_none()))
            .and_then(|c| c["text"].as_str());
        let Some(text) = whole else { return };
        doc.text = text.to_string();
        doc.version = version.unwrap_or(doc.version + 1);
        self.dirty.insert(uri.to_string());
    }

    fn did_close(&mut self, uri: &str) -> io::Result<()> {
        self.docs.remove(uri);
        self.cache.remove(uri);
        self.good.remove(uri);
        self.dirty.remove(uri);
        self.send(rpc::notification(
            "textDocument/publishDiagnostics",
            json!({"uri": uri, "diagnostics": []}),
        ))
    }

    /// Check every changed document once, and publish.
    fn flush(&mut self) -> io::Result<()> {
        for uri in std::mem::take(&mut self.dirty) {
            let Some(a) = self.analysis(&uri) else {
                continue;
            };
            let version = self.docs.get(&uri).map(|d| d.version);
            self.send(rpc::notification(
                "textDocument/publishDiagnostics",
                json!({"uri": uri, "version": version, "diagnostics": a.diagnostics()}),
            ))?;
        }
        Ok(())
    }

    fn analysis(&mut self, uri: &str) -> Option<Rc<Analysis>> {
        let doc = self.docs.get(uri)?;
        if let Some((v, a)) = self.cache.get(uri) {
            if *v == doc.version {
                return Some(a.clone());
            }
        }
        let a = Rc::new(Analysis::build(doc.path.as_deref(), doc.kind, &doc.text));
        if a.checked.map.is_some() {
            self.good.insert(uri.to_string(), a.clone());
        }
        self.cache.insert(uri.to_string(), (doc.version, a.clone()));
        Some(a)
    }

    /// A module's text: the open document's, else the file's.
    fn reader(&self) -> impl Fn(&Path) -> Option<String> + '_ {
        move |p: &Path| {
            self.docs
                .values()
                .find(|d| d.path.as_deref() == Some(p))
                .map(|d| d.text.clone())
                .or_else(|| std::fs::read_to_string(p).ok())
        }
    }

    // ---- requests ----

    fn hover(&mut self, uri: &str, line: u32, ch: u32) -> Value {
        let Some(a) = self.analysis(uri) else {
            return Value::Null;
        };
        let off = to_off(&a.text, &a.starts, line, ch);
        match a.hover(off) {
            Some((md, (s, e))) => json!({
                "contents": {"kind": "markdown", "value": md},
                "range": range(&a.text, &a.starts, s, e),
            }),
            None => Value::Null,
        }
    }

    fn locations(&self, a: &Analysis, locs: Vec<Loc>, uri: &str) -> Value {
        let read = self.reader();
        let mut out = Vec::new();
        for l in locs {
            match l.path {
                None => out.push(json!({
                    "uri": uri,
                    "range": range(&a.text, &a.starts, l.start, l.end),
                })),
                Some(p) => {
                    let Some(text) = read(&p) else { continue };
                    let starts = line_starts(&text);
                    out.push(json!({
                        "uri": path_to_uri(&p),
                        "range": range(&text, &starts, l.start, l.end),
                    }));
                }
            }
        }
        Value::Array(out)
    }

    fn definition(&mut self, uri: &str, line: u32, ch: u32) -> Value {
        let Some(a) = self.analysis(uri) else {
            return Value::Null;
        };
        let off = to_off(&a.text, &a.starts, line, ch);
        let locs = a.definition(off, &self.reader());
        self.locations(&a, locs, uri)
    }

    fn references(&mut self, uri: &str, line: u32, ch: u32, include: bool) -> Value {
        let Some(a) = self.analysis(uri) else {
            return Value::Null;
        };
        let off = to_off(&a.text, &a.starts, line, ch);
        let locs = a.references(off, include, &self.reader());
        self.locations(&a, locs, uri)
    }

    fn completion(&mut self, uri: &str, line: u32, ch: u32) -> Value {
        let Some(a) = self.analysis(uri) else {
            return Value::Null;
        };
        let off = to_off(&a.text, &a.starts, line, ch);
        let items = match a.is_dot_completion(off) {
            Some(dot) => self.members(uri, &a, off, dot),
            None => a.complete(off, &self.reader()),
        };
        json!({"isIncomplete": false, "items": items})
    }

    /// What follows `a.`. The line being typed rarely checks, so the file is
    /// checked again with the dot and the half-typed word blanked (the same
    /// length, so every offset holds); failing that, the last analysis that
    /// reached the end of lowering answers.
    fn members(&mut self, uri: &str, a: &Analysis, off: usize, dot: usize) -> Vec<Value> {
        let chain = a.lex.receiver_chain(dot);
        let Some(first) = a
            .lex
            .tokens
            .get(dot.saturating_sub(2 * chain.len().saturating_sub(1) + 1))
        else {
            return Vec::new();
        };
        let first_off = first.span.start;
        let read = self.reader();
        if a.checked.map.is_some() {
            return a.complete_chain(&chain, first_off, &read);
        }
        let start = a.lex.tokens[dot].span.start;
        let end = a
            .lex
            .token_at(off)
            .map(|i| a.lex.tokens[i].span.end)
            .unwrap_or(a.lex.tokens[dot].span.end)
            .max(a.lex.tokens[dot].span.end);
        let mut patched = a.text.clone();
        // Same length in bytes: one space per byte.
        patched.replace_range(start..end, &" ".repeat(end - start));
        let (path, kind) = match self.docs.get(uri) {
            Some(d) => (d.path.clone(), d.kind),
            None => return Vec::new(),
        };
        let p = Analysis::build(path.as_deref(), kind, &patched);
        if p.checked.map.is_some() {
            return p.complete_chain(&chain, first_off, &read);
        }
        match self.good.get(uri) {
            Some(g) => g.complete_chain(&chain, first_off, &read),
            None => Vec::new(),
        }
    }
}

fn capabilities() -> Value {
    json!({
        "capabilities": {
            "positionEncoding": "utf-16",
            "textDocumentSync": {"openClose": true, "change": 1},
            "hoverProvider": true,
            "definitionProvider": true,
            "referencesProvider": true,
            "documentFormattingProvider": true,
            "completionProvider": {"triggerCharacters": ["."]},
        },
        "serverInfo": {"name": "cigar-lsp", "version": env!("CARGO_PKG_VERSION")},
    })
}

fn position_params(p: &Value) -> Option<(String, u32, u32)> {
    Some((
        p["textDocument"]["uri"].as_str()?.to_string(),
        u32::try_from(p["position"]["line"].as_u64()?).ok()?,
        u32::try_from(p["position"]["character"].as_u64()?).ok()?,
    ))
}

/// `file:///a/b%20c.cigar` as a path; any other scheme is not a file.
pub fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // An authority other than empty or localhost is not this machine's.
    let rest = rest.strip_prefix("localhost").unwrap_or(rest);
    if !rest.starts_with('/') {
        return None;
    }
    let bytes = rest.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    let mut s = String::from_utf8(out).ok()?;
    // `/C:/x` is a Windows drive path.
    let b = s.as_bytes();
    if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
        s.remove(0);
    }
    Some(PathBuf::from(s))
}

/// A path as a `file://` URI.
pub fn path_to_uri(p: &Path) -> String {
    let s = p.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !s.starts_with('/') {
        out.push('/');
    }
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'_' | b'.' | b'~' | b':') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uris_round_trip() {
        let p = Path::new("/tmp/a b/é.cigar");
        let u = path_to_uri(p);
        assert_eq!(u, "file:///tmp/a%20b/%C3%A9.cigar");
        assert_eq!(uri_to_path(&u).as_deref(), Some(p));
        assert_eq!(uri_to_path("http://x/y"), None);
        assert_eq!(
            uri_to_path("file:///C:/x/y.cigar").as_deref(),
            Some(Path::new("C:/x/y.cigar"))
        );
    }
}
