// The checker's feature table is the docs table. Parse the rows between
// <!-- features:begin --> and <!-- features:end --> in docs/LANGUAGE.md
// into a generated Rust table, written to $OUT_DIR/features.rs.
//
// A malformed row fails the build with a clear message.

use std::env;
use std::fs;
use std::path::PathBuf;

const BEGIN: &str = "<!-- features:begin -->";
const END: &str = "<!-- features:end -->";

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let language_md = PathBuf::from(&manifest_dir).join("docs/LANGUAGE.md");
    println!("cargo:rerun-if-changed=docs/LANGUAGE.md");
    println!("cargo:rerun-if-changed=build.rs");

    let md = fs::read_to_string(&language_md)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", language_md.display()));

    let table = extract_table(&md).unwrap_or_else(|e| panic!("{e}"));
    let rows = parse_rows(&table).unwrap_or_else(|e| panic!("{e}"));

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR");
    let out = PathBuf::from(&out_dir).join("features.rs");
    fs::write(&out, render(&rows))
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out.display()));
}

fn extract_table(md: &str) -> Result<String, String> {
    let begin = md
        .find(BEGIN)
        .ok_or_else(|| format!("LANGUAGE.md: missing `{BEGIN}`"))?;
    let after = &md[begin + BEGIN.len()..];
    let end_rel = after
        .find(END)
        .ok_or_else(|| format!("LANGUAGE.md: missing `{END}`"))?;
    Ok(after[..end_rel].to_string())
}

#[derive(Debug)]
struct Row {
    name: String,
    strength: String,
    detect: String,
    issue: String,
    built: String,
}

fn parse_rows(table: &str) -> Result<Vec<Row>, String> {
    let mut rows = Vec::new();
    let mut seen_header = false;
    for (n, line) in table.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if !line.starts_with('|') {
            continue;
        }
        // The header separator `|---|---|...`: skip it.
        if line.chars().all(|c| matches!(c, '|' | '-' | ':' | ' ')) {
            continue;
        }
        // The first non-separator row is the header.
        if !seen_header {
            seen_header = true;
            // Validate the header, so a reordered table fails at the build.
            let cells = split_cells(line);
            let want = ["feature", "strength", "detect", "issue", "built"];
            if cells.len() != want.len()
                || cells
                    .iter()
                    .zip(want)
                    .any(|(got, w)| got.to_lowercase() != w)
            {
                return Err(format!(
                    "LANGUAGE.md feature table: header is `{}`, want `{}`",
                    cells.join(" | "),
                    want.join(" | ")
                ));
            }
            continue;
        }
        let cells = split_cells(line);
        if cells.len() != 5 {
            return Err(format!(
                "LANGUAGE.md feature table, row {}: `{}` has {} cells, want 5 (feature | strength | detect | issue | built)",
                n + 1,
                line,
                cells.len()
            ));
        }
        let name = cells[0].trim().to_string();
        let strength = cells[1].trim().to_string();
        let detect = cells[2].trim().to_string();
        let issue = cells[3].trim().to_string();
        let built = cells[4].trim().to_string();

        if name.is_empty() {
            return Err(format!(
                "LANGUAGE.md feature table, row {}: empty feature name",
                n + 1
            ));
        }
        if !matches!(strength.as_str(), "medium" | "full") {
            return Err(format!(
                "LANGUAGE.md feature table, row {}: strength is `{strength}`, want `medium` or `full`",
                n + 1
            ));
        }
        validate_detect(&detect)
            .map_err(|e| format!("LANGUAGE.md feature table, row {}: {e}", n + 1))?;
        validate_issue(&issue)
            .map_err(|e| format!("LANGUAGE.md feature table, row {}: {e}", n + 1))?;
        if !matches!(built.as_str(), "yes" | "no") {
            return Err(format!(
                "LANGUAGE.md feature table, row {}: built is `{built}`, want `yes` or `no`",
                n + 1
            ));
        }
        rows.push(Row {
            name,
            strength,
            detect,
            issue,
            built,
        });
    }
    if !seen_header {
        return Err("LANGUAGE.md feature table: no rows".to_string());
    }
    if rows.is_empty() {
        return Err("LANGUAGE.md feature table: no feature rows".to_string());
    }
    Ok(rows)
}

fn split_cells(line: &str) -> Vec<String> {
    let s = line.trim_matches('|');
    s.split('|').map(|c| c.trim().to_string()).collect()
}

fn validate_detect(d: &str) -> Result<(), String> {
    if d == "annotation" {
        return Ok(());
    }
    if let Some(w) = d.strip_prefix("word:") {
        if w.is_empty() {
            return Err("detect `word:` is missing the word".to_string());
        }
        if !w.chars().all(is_ident_char) {
            return Err(format!("detect `word:{w}` is not an identifier"));
        }
        return Ok(());
    }
    Err(format!("detect is `{d}`, want `annotation` or `word:NAME`"))
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

fn validate_issue(i: &str) -> Result<(), String> {
    let digits = i.strip_prefix('#').unwrap_or(i);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return Err(format!("issue is `{i}`, want `#<number>`"));
    }
    Ok(())
}

fn render(rows: &[Row]) -> String {
    let mut s = String::new();
    s.push_str("// Generated by build.rs from docs/LANGUAGE.md. Do not edit by hand.\n\n");
    s.push_str("pub const FEATURES: &[Feature] = &[\n");
    for r in rows {
        let strength = match r.strength.as_str() {
            "medium" => "Strength::Medium",
            "full" => "Strength::Full",
            _ => unreachable!(),
        };
        let detect = if r.detect == "annotation" {
            "Detect::Annotation".to_string()
        } else {
            let w = r.detect.strip_prefix("word:").unwrap();
            format!("Detect::Word(\"{w}\")")
        };
        let built = matches!(r.built.as_str(), "yes");
        let name = escape(&r.name);
        let issue = escape(&r.issue);
        s.push_str(&format!(
            "    Feature {{ name: \"{name}\", strength: {strength}, detect: {detect}, issue: \"{issue}\", built: {built} }},\n"
        ));
    }
    s.push_str("];\n");
    s
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
