// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar-lsp` (#16): the binary is driven over stdio with framed messages.
//!
//! Every server runs in a scratch folder with its own HOME, CIGSCRIPT_HOME and
//! XDG folders. Every wait for a message has a hard limit, and the server is
//! killed when its test ends however it ends.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use serde_json::{json, Value};

const WAIT: Duration = Duration::from_secs(30);

struct Lsp {
    child: Child,
    stdin: ChildStdin,
    rx: Receiver<Value>,
    notes: Vec<Value>,
    next: i64,
}

impl Drop for Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Lsp {
    fn start(dir: &Path) -> Lsp {
        let state = dir.join(".state");
        let mut child = Command::new(env!("CARGO_BIN_EXE_cigar-lsp"))
            .current_dir(dir)
            .env("HOME", &state)
            .env("CIGSCRIPT_HOME", state.join("cig"))
            .env("XDG_CACHE_HOME", state.join("cache"))
            .env("XDG_CONFIG_HOME", state.join("config"))
            .env("XDG_DATA_HOME", state.join("data"))
            .env("XDG_STATE_HOME", state.join("state"))
            .env("NO_COLOR", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("cigar-lsp starts");
        let stdin = child.stdin.take().unwrap();
        let mut out = BufReader::new(child.stdout.take().unwrap());
        let (tx, rx) = channel();
        std::thread::spawn(move || loop {
            let mut len = None;
            loop {
                let mut line = String::new();
                if out.read_line(&mut line).unwrap_or(0) == 0 {
                    return;
                }
                let line = line.trim_end();
                if line.is_empty() {
                    break;
                }
                if let Some(v) = line.strip_prefix("Content-Length: ") {
                    len = v.parse::<usize>().ok();
                }
            }
            let mut body = vec![0u8; len.expect("every message has a length")];
            if out.read_exact(&mut body).is_err() {
                return;
            }
            let v: Value = serde_json::from_slice(&body).expect("the server sends JSON");
            if tx.send(v).is_err() {
                return;
            }
        });
        Lsp {
            child,
            stdin,
            rx,
            notes: Vec::new(),
            next: 1,
        }
    }

    fn send(&mut self, v: Value) {
        let body = serde_json::to_vec(&v).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n", body.len()).unwrap();
        self.stdin.write_all(&body).unwrap();
        self.stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}));
    }

    fn recv(&mut self) -> Value {
        self.rx
            .recv_timeout(WAIT)
            .expect("the server answered within the limit")
    }

    /// Send a request and wait for its response (the whole message).
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next;
        self.next += 1;
        self.send(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}));
        loop {
            let m = self.recv();
            if m.get("id") == Some(&json!(id)) {
                return m;
            }
            self.notes.push(m);
        }
    }

    fn initialize(&mut self, root: &Path) -> Value {
        let r = self.request(
            "initialize",
            json!({"processId": null, "rootUri": uri(root), "capabilities": {}}),
        );
        self.notify("initialized", json!({}));
        r
    }

    fn open(&mut self, path: &Path, version: i64, text: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({"textDocument": {
                "uri": uri(path), "languageId": "cigar", "version": version, "text": text}}),
        );
    }

    fn change(&mut self, path: &Path, version: i64, text: &str) {
        self.notify(
            "textDocument/didChange",
            json!({"textDocument": {"uri": uri(path), "version": version},
                   "contentChanges": [{"text": text}]}),
        );
    }

    /// The diagnostics published for a file at a version.
    fn diagnostics(&mut self, path: &Path, version: i64) -> Vec<Value> {
        let u = uri(path);
        let hit = |m: &Value| {
            m["method"] == "textDocument/publishDiagnostics"
                && m["params"]["uri"] == u.as_str()
                && m["params"]["version"] == version
        };
        if let Some(i) = self.notes.iter().position(hit) {
            return self.notes.remove(i)["params"]["diagnostics"]
                .as_array()
                .unwrap()
                .clone();
        }
        loop {
            let m = self.recv();
            if hit(&m) {
                return m["params"]["diagnostics"].as_array().unwrap().clone();
            }
            self.notes.push(m);
        }
    }

    fn at(&mut self, method: &str, path: &Path, line: u32, ch: u32) -> Value {
        let r = self.request(
            method,
            json!({"textDocument": {"uri": uri(path)},
                   "position": {"line": line, "character": ch},
                   "context": {"includeDeclaration": true}}),
        );
        assert!(r.get("error").is_none(), "{method}: {r}");
        r["result"].clone()
    }

    fn shutdown(&mut self) -> Option<i32> {
        let r = self.request("shutdown", Value::Null);
        assert!(r["result"].is_null() && r.get("error").is_none());
        self.notify("exit", Value::Null);
        self.child.wait_timeout_or_kill(WAIT)
    }
}

trait WaitOrKill {
    fn wait_timeout_or_kill(&mut self, limit: Duration) -> Option<i32>;
}

impl WaitOrKill for Child {
    fn wait_timeout_or_kill(&mut self, limit: Duration) -> Option<i32> {
        use wait_timeout::ChildExt;
        match self.wait_timeout(limit).expect("waiting on cigar-lsp") {
            Some(s) => s.code(),
            None => {
                let _ = self.kill();
                let _ = self.wait();
                panic!("cigar-lsp did not exit after `exit`");
            }
        }
    }
}

fn uri(p: &Path) -> String {
    format!("file://{}", p.display())
}

/// The line and column (in characters) of the `n`th (from 0) `needle`.
fn find(text: &str, needle: &str, n: usize) -> (u32, u32) {
    let at = text
        .match_indices(needle)
        .nth(n)
        .expect("needle is there")
        .0;
    let before = &text[..at];
    let line = before.matches('\n').count();
    let col = before.rsplit('\n').next().unwrap().chars().count();
    (line as u32, col as u32)
}

const MAIN: &str = r#"strength medium

use tools.plan

record Person { name: Text, age: Int }

pull greet(p: Person) -> Text {
  snuff "hello " + p.name
}

stick who: Person = Person { name: "Ann", age: 3 }
stick msg = greet(who)
exhale msg
exhale plan.double(4)
"#;

const PLAN: &str = "strength medium\n\npub pull double(n: Int) -> Int {\n  snuff n * 2\n}\n";

fn project() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    std::fs::create_dir_all(root.join("tools")).unwrap();
    let main = root.join("main.cigar");
    let plan = root.join("tools/plan.cigar");
    std::fs::write(&main, MAIN).unwrap();
    std::fs::write(&plan, PLAN).unwrap();
    (dir, root, main)
}

fn hover_text(v: &Value) -> String {
    v["contents"]["value"].as_str().unwrap_or("").to_string()
}

#[test]
fn initialize_advertises_what_it_does_and_shutdown_exits_zero() {
    let (_d, root, _) = project();
    let mut s = Lsp::start(&root);
    let r = s.initialize(&root);
    let caps = &r["result"]["capabilities"];
    assert_eq!(caps["textDocumentSync"]["change"], 1);
    assert_eq!(caps["hoverProvider"], true);
    assert_eq!(caps["definitionProvider"], true);
    assert_eq!(caps["referencesProvider"], true);
    assert_eq!(caps["documentFormattingProvider"], true);
    assert_eq!(caps["completionProvider"]["triggerCharacters"][0], ".");
    assert_eq!(r["result"]["serverInfo"]["name"], "cigar-lsp");
    // Asking again is refused, and a request nobody knows is a clear no.
    let again = s.request("initialize", json!({}));
    assert_eq!(again["error"]["code"], -32600);
    let odd = s.request("workspace/nothing", json!({}));
    assert_eq!(odd["error"]["code"], -32601);
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn exit_without_shutdown_is_one_and_requests_before_initialize_are_refused() {
    let (_d, root, main) = project();
    let mut s = Lsp::start(&root);
    let early = s.request(
        "textDocument/hover",
        json!({"textDocument": {"uri": uri(&main)}, "position": {"line": 0, "character": 0}}),
    );
    assert_eq!(early["error"]["code"], -32002);
    s.notify("exit", Value::Null);
    assert_eq!(s.child.wait_timeout_or_kill(WAIT), Some(1));
}

#[test]
fn a_broken_file_is_diagnosed_and_a_fix_clears_it() {
    let (_d, root, _) = project();
    let broken = root.join("broken.cigar");
    let bad = "strength medium\n\nstick n: Int = \"text\"\nexhale n\n";
    let good = "strength medium\n\nstick n: Int = 4\nexhale n\n";
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.open(&broken, 1, bad);
    let d = s.diagnostics(&broken, 1);
    assert_eq!(d.len(), 1, "{d:?}");
    assert_eq!(d[0]["code"], "E1201");
    assert_eq!(d[0]["severity"], 1);
    assert_eq!(d[0]["source"], "cigar");
    assert_eq!(d[0]["range"]["start"]["line"], 2);
    assert_eq!(d[0]["range"]["start"]["character"], 15);
    assert_eq!(d[0]["range"]["end"]["character"], 21);
    // Hover on the error says what kind of no it is, in the registry's words.
    let h = s.at("textDocument/hover", &broken, 2, 17);
    let md = hover_text(&h);
    assert!(md.contains("E1201") && md.contains("check"), "{md}");
    // A code with a page in cig's registry gives its kind of no, as `cig explain` does.
    let outside = root.join("outside.cigar");
    let src = "strength medium\n\nstick rows = [\"a\"]\nrows into \"report.csv\"\n";
    s.open(&outside, 1, src);
    let d = s.diagnostics(&outside, 1);
    assert_eq!(d[0]["code"], "E303", "{d:?}");
    let (l, c) = find(src, "rows into", 0);
    let md = hover_text(&s.at("textDocument/hover", &outside, l, c + 2));
    assert!(md.contains("E303") && md.contains("kind of no:"), "{md}");
    assert!(md.contains("what to type next:"), "{md}");
    // The same fix, sent twice (an edit that arrives again), is one answer.
    s.change(&broken, 2, good);
    s.change(&broken, 2, good);
    assert!(s.diagnostics(&broken, 2).is_empty());
    // An old version arriving late changes nothing.
    s.change(&broken, 1, bad);
    s.change(&broken, 3, good);
    assert!(s.diagnostics(&broken, 3).is_empty());
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn a_burst_of_edits_is_checked_once_for_the_last_one() {
    let (_d, root, _) = project();
    let f = root.join("burst.cigar");
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.open(&f, 1, "strength medium\n");
    s.diagnostics(&f, 1);
    for v in 2..=40 {
        s.change(&f, v, &format!("strength medium\nstick n: Int = {v}\n"));
    }
    s.change(&f, 41, "strength medium\nstick n: Int = \"x\"\n");
    // Whatever was coalesced, the last version is published, and it is the bad one.
    let d = s.diagnostics(&f, 41);
    assert_eq!(d[0]["code"], "E1201");
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn hover_gives_a_names_type_a_signature_and_the_layers() {
    let (_d, root, main) = project();
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.open(&main, 1, MAIN);
    assert!(s.diagnostics(&main, 1).is_empty());
    let (l, c) = find(MAIN, "who)", 0);
    let h = hover_text(&s.at("textDocument/hover", &main, l, c));
    assert!(h.contains("stick who: Person"), "{h}");
    // The checker records the type of a `stick` that is annotated; an
    // inferred one is not in what it hands over, so there is nothing to say.
    let (l, c) = find(MAIN, "msg =", 0);
    assert!(s.at("textDocument/hover", &main, l, c + 1).is_null());
    let (l, c) = find(MAIN, "greet(who)", 0);
    let h = hover_text(&s.at("textDocument/hover", &main, l, c));
    assert!(h.contains("pull greet(p: Person) -> Text"), "{h}");
    let (l, c) = find(MAIN, "p.name", 0);
    let h = hover_text(&s.at("textDocument/hover", &main, l, c + 2));
    assert!(h.contains("(field) name: Text"), "{h}");
    let (l, c) = find(MAIN, "double", 0);
    let h = hover_text(&s.at("textDocument/hover", &main, l, c));
    assert!(h.contains("pull double(n: Int) -> Int"), "{h}");
    // Nothing to say about a string.
    let (l, c) = find(MAIN, "\"hello", 0);
    assert!(s.at("textDocument/hover", &main, l, c + 2).is_null());

    // The layers, from the same reading as `cigar explain --layers`.
    let lay = root.join("layers.cigar");
    let src = "strength medium\n\nstick rows = [\"a\", \"b\"]\nroll seen = []\nrows into seen\n";
    s.open(&lay, 1, src);
    assert!(s.diagnostics(&lay, 1).is_empty());
    let (l, c) = find(src, "rows into", 0);
    let h = hover_text(&s.at("textDocument/hover", &lay, l, c + 6));
    assert!(h.contains("`into` means *append to a list*"), "{h}");
    assert!(h.contains("full"), "{h}");
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn definition_finds_a_pull_a_stick_a_record_and_a_modules_function() {
    let (_d, root, main) = project();
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.open(&main, 1, MAIN);
    s.diagnostics(&main, 1);
    // `greet(who)` -> `pull greet`.
    let (l, c) = find(MAIN, "greet(who)", 0);
    let r = s.at("textDocument/definition", &main, l, c + 1);
    let (dl, dc) = find(MAIN, "greet(p", 0);
    assert_eq!(r[0]["uri"], uri(&main).as_str());
    assert_eq!(r[0]["range"]["start"]["line"], dl);
    assert_eq!(r[0]["range"]["start"]["character"], dc);
    // `who)` -> `stick who`.
    let (l, c) = find(MAIN, "who)", 0);
    let r = s.at("textDocument/definition", &main, l, c);
    let (dl, _) = find(MAIN, "who: Person", 0);
    assert_eq!(r[0]["range"]["start"]["line"], dl);
    // A parameter inside its function.
    let (l, c) = find(MAIN, "p.name", 0);
    let r = s.at("textDocument/definition", &main, l, c);
    let (dl, dc) = find(MAIN, "p: Person", 0);
    assert_eq!(r[0]["range"]["start"]["line"], dl);
    assert_eq!(r[0]["range"]["start"]["character"], dc);
    // A field read through a typed parameter -> the record's field.
    let (l, c) = find(MAIN, "p.name", 0);
    let r = s.at("textDocument/definition", &main, l, c + 2);
    let (dl, dc) = find(MAIN, "name: Text", 0);
    assert_eq!(r[0]["range"]["start"]["line"], dl);
    assert_eq!(r[0]["range"]["start"]["character"], dc);
    // The record `Person` in a type position.
    let (l, c) = find(MAIN, "Person)", 0);
    let r = s.at("textDocument/definition", &main, l, c);
    let (dl, _) = find(MAIN, "Person {", 0);
    assert_eq!(r[0]["range"]["start"]["line"], dl);
    // `plan.double` -> the module file.
    let (l, c) = find(MAIN, "double(4)", 0);
    let r = s.at("textDocument/definition", &main, l, c);
    let plan = root.join("tools/plan.cigar");
    assert_eq!(r[0]["uri"], uri(&plan).as_str());
    assert_eq!(r[0]["range"]["start"]["line"], 2);
    assert_eq!(r[0]["range"]["start"]["character"], 9);
    // `plan` itself -> the module's file.
    let (l, c) = find(MAIN, "plan.double", 0);
    let r = s.at("textDocument/definition", &main, l, c);
    assert_eq!(r[0]["uri"], uri(&plan).as_str());
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn references_follow_a_name_and_not_another_of_the_same_spelling() {
    let (_d, root, main) = project();
    let src =
        "strength medium\n\nstick n = 1\npull f(n: Int) -> Int {\n  snuff n + 1\n}\nexhale f(n)\n";
    let f = root.join("refs.cigar");
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.open(&f, 1, src);
    s.open(&main, 1, MAIN);
    // The outer `n`: its declaration and the use in `f(n)`, not the parameter.
    let (l, c) = find(src, "n = 1", 0);
    let r = s.at("textDocument/references", &f, l, c);
    let lines: Vec<u64> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![2, 6]);
    // The parameter: its declaration and the use inside the body.
    let (l, c) = find(src, "n: Int", 0);
    let r = s.at("textDocument/references", &f, l, c);
    let lines: Vec<u64> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["range"]["start"]["line"].as_u64().unwrap())
        .collect();
    assert_eq!(lines, vec![3, 4]);
    // A module's function: its use here and its declaration there.
    let (l, c) = find(MAIN, "double(4)", 0);
    let r = s.at("textDocument/references", &main, l, c);
    let uris: Vec<&str> = r
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["uri"].as_str().unwrap())
        .collect();
    assert!(
        uris.contains(&uri(&root.join("tools/plan.cigar")).as_str()),
        "{r}"
    );
    assert!(uris.contains(&uri(&main).as_str()), "{r}");
    assert_eq!(s.shutdown(), Some(0));
}

fn labels(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["label"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn completion_gives_fields_after_a_dot_and_keywords_for_the_strength() {
    let (_d, root, main) = project();
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    // The line being typed does not check, and the fields still come.
    let typing = format!("{MAIN}stick x = who.\n");
    let f = root.join("typing.cigar");
    s.open(&f, 1, &typing);
    let (l, c) = find(&typing, "who.\n", 0);
    let r = s.at("textDocument/completion", &f, l, c + 4);
    let got = labels(&r);
    assert!(
        got.contains(&"name".to_string()) && got.contains(&"age".to_string()),
        "{got:?}"
    );
    assert!(!got.contains(&"greet".to_string()), "{got:?}");
    // A half-typed field name too.
    let typing2 = format!("{MAIN}stick x = who.na\n");
    s.change(&f, 2, &typing2);
    let (l, c) = find(&typing2, "who.na", 0);
    let got = labels(&s.at("textDocument/completion", &f, l, c + 6));
    assert!(got.contains(&"name".to_string()), "{got:?}");
    // A module's public names after `plan.`.
    let typing3 = format!("{MAIN}stick y = plan.\n");
    s.change(&f, 3, &typing3);
    let (l, c) = find(&typing3, "plan.\n", 0);
    let got = labels(&s.at("textDocument/completion", &f, l, c + 5));
    assert_eq!(got, vec!["double".to_string()]);
    // Names in scope and keywords, at the file's strength.
    s.open(&main, 1, MAIN);
    let (l, _) = find(MAIN, "exhale msg", 0);
    let got = labels(&s.at("textDocument/completion", &main, l, 0));
    for w in ["greet", "who", "msg", "Person", "plan", "stick", "record"] {
        assert!(got.contains(&w.to_string()), "{w} in {got:?}");
    }
    assert!(
        !got.contains(&"p".to_string()),
        "a parameter is not seen outside its function"
    );
    // A mild file does not offer what a mild file cannot write.
    let mild = root.join("mild.cigar");
    s.open(&mild, 1, "strength mild\n\nstick a = 1\n");
    let got = labels(&s.at("textDocument/completion", &mild, 3, 0));
    assert!(got.contains(&"stick".to_string()) && got.contains(&"a".to_string()));
    assert!(!got.contains(&"record".to_string()), "{got:?}");
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn a_file_that_is_not_cigarscript_is_left_alone_and_closing_clears() {
    let (_d, root, _) = project();
    let notes = root.join("notes.txt");
    let f = root.join("c.cigar");
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.notify(
        "textDocument/didOpen",
        json!({"textDocument": {"uri": uri(&notes), "languageId": "plaintext",
                                "version": 1, "text": "stick = = ="}}),
    );
    s.open(&f, 1, "strength medium\nstick n: Int = \"x\"\nexhale n\n");
    assert_eq!(s.diagnostics(&f, 1).len(), 1);
    s.notify(
        "textDocument/didClose",
        json!({"textDocument": {"uri": uri(&f)}}),
    );
    // Closing publishes an empty list (the version is absent).
    loop {
        let m = s.recv();
        if m["method"] == "textDocument/publishDiagnostics"
            && m["params"]["uri"] == uri(&f).as_str()
        {
            assert!(m["params"]["diagnostics"].as_array().unwrap().is_empty());
            break;
        }
        assert!(
            m["params"]["uri"] != uri(&notes).as_str(),
            "a text file got diagnostics"
        );
    }
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn a_garbled_message_gets_a_parse_error_and_the_server_carries_on() {
    let (_d, root, _) = project();
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    s.stdin
        .write_all(b"Content-Length: 5\r\n\r\n{{{{{")
        .unwrap();
    s.stdin.flush().unwrap();
    let m = s.recv();
    assert_eq!(m["error"]["code"], -32700);
    // And it still answers.
    assert_eq!(s.shutdown(), Some(0));
}

#[test]
fn formatting_returns_one_edit_from_the_same_formatter_as_cigar_fmt() {
    let (_d, root, _) = project();
    let f = root.join("fmt.cigar");
    let mut s = Lsp::start(&root);
    s.initialize(&root);
    // A multi-byte character on the last line: the end of the range is in
    // UTF-16 units, with no final newline.
    let ugly =
        "strength medium\npull  f( a:Int )->Int{\nsnuff a+1 # caf\u{e9}\n}\nexhale \"\u{1F600}\"  ";
    s.open(&f, 1, ugly);
    let r = s.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri(&f)},
        "options": {"tabSize": 4, "insertSpaces": true}}),
    );
    let edits = r["result"].as_array().expect("edits");
    assert_eq!(edits.len(), 1, "{r}");
    let want = cigarscript::fmt::format(ugly).unwrap();
    assert_eq!(edits[0]["newText"], want.as_str());
    assert_eq!(
        edits[0]["range"]["start"],
        json!({"line": 0, "character": 0})
    );
    assert_eq!(
        edits[0]["range"]["end"],
        json!({"line": 4, "character": 13})
    );
    assert!(
        want.contains("pull f(a: Int) -> Int {\n  snuff a + 1 # caf\u{e9}\n}\n"),
        "{want}"
    );

    // Already formatted: no edits. Changed in the editor: the new text.
    s.change(&f, 2, &want);
    let r = s.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri(&f)}}),
    );
    assert_eq!(r["result"], json!([]), "{r}");
    // Does not lex: null, and the server carries on.
    s.change(&f, 3, "stick x = \"open\n");
    let r = s.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri(&f)}}),
    );
    assert!(r["result"].is_null(), "{r}");
    // Not open: null. No document at all: invalid params.
    let r = s.request(
        "textDocument/formatting",
        json!({"textDocument": {"uri": uri(&root.join("no.cigar"))}}),
    );
    assert!(r["result"].is_null(), "{r}");
    let r = s.request("textDocument/formatting", json!({}));
    assert_eq!(r["error"]["code"], -32602);
    assert_eq!(s.shutdown(), Some(0));
}
