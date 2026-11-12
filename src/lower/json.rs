// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `derive(Json)` (#38, section 2).
//!
//! - `x.to_json()` gives the value as JSON text: a record as an object of its
//!   fields, an enum externally tagged (`"dot"`, or `{"circle": {"r": 1.0}}`),
//!   lists and maps as they are.
//! - `Config.from_json(text)` gives a `Result<Config, Text>`: `Ok` of the value,
//!   or `Err` naming the path of the first mismatch, as a checked boundary's
//!   `E1210` does (`value.retries should be Int and is Text "x"`). A key the
//!   record does not have is refused (`E1206`) unless the record says
//!   `derive(Json(lenient))`; a missing key takes the field's default, and
//!   without one is `E1205`. Nested records and enums are read by their own
//!   `derive(Json)`.
//!
//! What a type reads is a table, `_cg_json_specs()`, written with the
//! program (so a module's types are in it), beside `_cg_records()`.

use super::ty;
use crate::types::Program;

/// The run-time functions of `derive(Json)`.
pub const JSON: &str = r#"pull _cg_json_out(v) {
  stick t = type_of(v)
  if t == "list" { roll out = []; for x in v { out.push(_cg_json_out(x)) }; snuff out }
  if t != "map" { snuff v }
  if _cg_is_enum(v, null) {
    roll f = {}
    for k in v { if k != "__enum" and k != "__variant" { f[k] = _cg_json_out(v[k]) } }
    if f.len() == 0 { snuff v.__variant }
    roll o = {}
    o[v.__variant] = f
    snuff o
  }
  roll out = {}
  for k in v { if k != "__record" { out[k] = _cg_json_out(v[k]) } }
  snuff out
}
pull _cg_json_bad(v, d, path) { cough _cg_why(v, d, path) }
pull _cg_json_fields(v, spec, name, path) {
  if type_of(v) != "map" { _cg_json_bad(v, {k: "record", name: name}, path) }
  for key in v {
    if not spec.descs.has(key) and not spec.lenient {
      cough {code: "E1206", message: "E1206 " + path + "." + key + ": `" + name + "` has no field `" + key + "`"}
    }
  }
  roll f = {}
  for n in spec.fields {
    if v.has(n) { f[n] = _cg_json_in(v[n], spec.descs[n], path + "." + n) }
    else if spec.defaults.has(n) { f[n] = spec.defaults[n] }
    else { cough {code: "E1205", message: "E1205 " + path + "." + n + ": `" + name + "` is missing the field `" + n + "`"} }
  }
  snuff f
}
pull _cg_json_in(v, d, path) {
  stick k = d.k
  if k == "any" { snuff v }
  if k == "opt" { if v == null { snuff null }; snuff _cg_json_in(v, d.of, path) }
  if k == "list" {
    if type_of(v) != "list" { _cg_json_bad(v, d, path) }
    roll out = []
    roll i = 0
    for x in v { out.push(_cg_json_in(x, d.of, path + "[" + str(i) + "]")); i = i + 1 }
    snuff out
  }
  if k == "map" {
    if type_of(v) != "map" { _cg_json_bad(v, d, path) }
    roll out = {}
    for key in v { out[key] = _cg_json_in(v[key], d.of, path + "." + key) }
    snuff out
  }
  if k == "record" {
    stick spec = _cg_json_specs().get(d.name)
    if spec == null { cough {code: "E1210", message: "E1210 " + path + ": `" + d.name + "` has no `derive(Json)`"} }
    stick make = _cg_records()[d.name]
    snuff make(_cg_json_fields(v, spec, d.name, path))
  }
  if k == "enum" {
    stick spec = _cg_json_specs().get(d.name)
    if spec == null { cough {code: "E1210", message: "E1210 " + path + ": `" + d.name + "` has no `derive(Json)`"} }
    roll variant = null
    roll body = {}
    if type_of(v) == "string" { variant = v }
    else if type_of(v) == "map" and v.len() == 1 { for key in v { variant = key; body = v[key] } }
    else { _cg_json_bad(v, d, path) }
    stick vs = spec.variants.get(variant)
    if vs == null { cough {code: "E1207", message: "E1207 " + path + ": `" + d.name + "` has no variant `" + str(variant) + "`"} }
    stick f = _cg_json_fields(body, vs, d.name + "." + variant, path + "." + variant)
    roll out = {__enum: d.name, __variant: variant}
    for n in vs.fields { out[n] = f[n] }
    snuff out
  }
  if _cg_fits(v, d) { snuff v }
  _cg_json_bad(v, d, path)
}
"#;

/// `_cg_json_specs()`: what each type with `derive(Json)` reads.
pub fn specs(prog: &Program) -> String {
    let says = |d: &[(String, crate::types::At)], w: &str| d.iter().any(|(i, _)| i == w);
    let mut out = Vec::new();
    for r in prog.records.values().filter(|r| says(&r.derives, "Json")) {
        let fields: Vec<String> = r.fields.iter().map(|(f, _)| format!("\"{f}\"")).collect();
        let descs: Vec<String> = r
            .fields
            .iter()
            .map(|(f, t)| format!("{f}: {}", ty::descriptor(t)))
            .collect();
        let defaults: Vec<String> = r
            .defaults
            .iter()
            .map(|(f, v, _)| format!("{f}: {v}"))
            .collect();
        out.push(format!(
            "{}: {{fields: [{}], descs: {{{}}}, defaults: {{{}}}, lenient: {}}}",
            r.name,
            fields.join(", "),
            descs.join(", "),
            defaults.join(", "),
            says(&r.derives, "Json(lenient)")
        ));
    }
    for e in prog.enums.values().filter(|e| says(&e.derives, "Json")) {
        let variants: Vec<String> = e
            .variants
            .iter()
            .map(|v| {
                let fields: Vec<String> =
                    v.fields.iter().map(|(f, _)| format!("\"{f}\"")).collect();
                let descs: Vec<String> = v
                    .fields
                    .iter()
                    .map(|(f, t)| format!("{f}: {}", ty::descriptor(t)))
                    .collect();
                format!(
                    "{}: {{fields: [{}], descs: {{{}}}, defaults: {{}}, lenient: {}}}",
                    v.name,
                    fields.join(", "),
                    descs.join(", "),
                    says(&e.derives, "Json(lenient)")
                )
            })
            .collect();
        out.push(format!(
            "{}: {{variants: {{{}}}}}",
            e.name,
            variants.join(", ")
        ));
    }
    format!("pull _cg_json_specs() {{ snuff {{{}}} }}\n", out.join(", "))
}

/// Does any type of the program carry `derive(Json)`?
pub fn any(prog: &Program) -> bool {
    let says = |d: &[(String, crate::types::At)]| d.iter().any(|(i, _)| i == "Json");
    prog.records.values().any(|r| says(&r.derives)) || prog.enums.values().any(|e| says(&e.derives))
}
