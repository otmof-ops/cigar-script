// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! The second rewrite: what the types decided, applied to the lowered text.
//!
//! Lowering cannot know types, so it leaves `measure x` as
//! `_cg_layer_measure(x)` and `m.size()` as written. The checker works out
//! which layer and which `impl` each one means, and where a sized number
//! needs a range check; this applies those choices as edits, the same way
//! lowering applies its own, keeping every line on its line.

use crate::lower::edits::{Applied, Edits};
use crate::typeck::Resolution;

/// Apply `rs` to `text` (the lowered text the checker read).
pub fn apply(text: &str, rs: &[Resolution]) -> Applied {
    let mut e = Edits::default();
    // Statement ends first: at one point, what closes a statement comes
    // after what closes an expression inside it.
    for r in rs {
        if let Resolution::Insert { at, text } = r {
            e.close(*at, text.clone());
        }
    }
    // Wrappers that open at one point open outer first; the edits keep
    // the order they are made in, so make the outer ones first.
    let mut opening: Vec<&Resolution> = rs
        .iter()
        .filter(|r| matches!(r, Resolution::Method { .. } | Resolution::Wrap { .. }))
        .collect();
    // Over the same text, a wrapper goes outside a method call: it wraps the
    // whole call (`exhale s.union(t)` shows the union).
    opening.sort_by_key(|r| match r {
        Resolution::Method {
            recv_start,
            call_end,
            ..
        } => (*recv_start, std::cmp::Reverse(*call_end), 1),
        Resolution::Wrap { start, end, .. } => (*start, std::cmp::Reverse(*end), 0),
        _ => (0, std::cmp::Reverse(0), 0),
    });
    for r in opening {
        match r {
            Resolution::Method {
                recv_start,
                recv_end,
                call_end,
                func,
                has_args,
                safe,
            } => {
                let Some(paren) = open_paren_after(text, *recv_end) else {
                    continue;
                };
                let comma = if *has_args { ", " } else { "" };
                if *safe {
                    // `recv?.name(a)` -> `_cg_safe(recv, pack(__cg_s) => func(__cg_s, a))`:
                    // on null, nothing is called and nothing is evaluated.
                    e.open(*recv_start, "_cg_safe(");
                    e.replace(
                        *recv_end,
                        paren + 1,
                        format!(", pack(__cg_s) => {func}(__cg_s{comma}"),
                    );
                    e.close(*call_end, ")");
                } else {
                    // `recv.name(` -> `func(recv, ` (or `func(recv` with no arguments).
                    e.open(*recv_start, format!("{func}("));
                    e.replace(*recv_end, paren + 1, comma);
                }
            }
            Resolution::Wrap {
                start,
                end,
                pre,
                post,
            } => {
                e.open(*start, pre.clone());
                e.close(*end, post.clone());
            }
            _ => {}
        }
    }
    for r in rs {
        if let Resolution::Rename { start, end, text } = r {
            e.replace(*start, *end, text.clone());
        }
    }
    e.apply(text)
}

/// After a method call's receiver: `.name(` or `?.name(`, with any
/// whitespace between. The offset of the `(`.
fn open_paren_after(text: &str, from: usize) -> Option<usize> {
    let b = text.as_bytes();
    let mut i = from;
    let skip = |i: &mut usize| {
        while *i < b.len() && (b[*i] == b' ' || b[*i] == b'\t' || b[*i] == b'\n' || b[*i] == b'\r')
        {
            *i += 1;
        }
    };
    skip(&mut i);
    if b.get(i) == Some(&b'?') {
        i += 1;
    }
    if b.get(i) != Some(&b'.') {
        return None;
    }
    i += 1;
    skip(&mut i);
    let name_start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    if i == name_start {
        return None;
    }
    skip(&mut i);
    (b.get(i) == Some(&b'(')).then_some(i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_method_call_becomes_a_call_to_its_impl() {
        let src = "exhale m.size(), n.add(1)\n";
        let m = src.find('m').unwrap();
        let n = src.find("n.").unwrap();
        let rs = vec![
            Resolution::Method {
                recv_start: m,
                recv_end: m + 1,
                call_end: src.find(')').unwrap() + 1,
                func: "_cg_impl_Sized_Move_size".into(),
                has_args: false,
                safe: false,
            },
            Resolution::Method {
                recv_start: n,
                recv_end: n + 1,
                call_end: src.rfind(')').unwrap() + 1,
                func: "_cg_impl_Add_Int_add".into(),
                has_args: true,
                safe: false,
            },
        ];
        let a = apply(src, &rs);
        assert_eq!(
            a.text,
            "exhale _cg_impl_Sized_Move_size(m), _cg_impl_Add_Int_add(n, 1)\n"
        );
    }

    #[test]
    fn nested_wraps_open_outer_first() {
        // `m.size() + 1`, sized: the call is inside the range check.
        let src = "stick x = m.size() + 1\n";
        let m = src.find('m').unwrap();
        let close = src.find(')').unwrap() + 1;
        let end = src.find('\n').unwrap();
        let rs = vec![
            Resolution::Method {
                recv_start: m,
                recv_end: m + 1,
                call_end: close,
                func: "f".into(),
                has_args: false,
                safe: false,
            },
            Resolution::Wrap {
                start: m,
                end,
                pre: "_cg_sized(".into(),
                post: ", 0, 255, \"u8\")".into(),
            },
        ];
        let a = apply(src, &rs);
        assert_eq!(a.text, "stick x = _cg_sized(f(m) + 1, 0, 255, \"u8\")\n");
    }
}
