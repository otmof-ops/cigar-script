// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! Edits to the original text, applied in one pass, and the maps between
//! original and lowered positions.
//!
//! A replacement takes the place of a range. An insertion sits at a point:
//! an *opening* one (the start of a wrapper, such as `_cg_show(`) goes in
//! the order it was made, so an outer wrapper opens first; a *closing* one
//! goes in the reverse order, so the inner wrapper closes first. At one
//! point, closings come before openings, and both before a replacement that
//! starts there.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Close,
    Open,
    Replace,
}

#[derive(Clone, Debug)]
pub struct Edit {
    pub start: usize,
    pub end: usize,
    pub text: String,
    pub kind: Kind,
    pub seq: usize,
}

/// A stretch of the lowered text: copied from the original (`copied`), or
/// written by an edit (anchored at the original offset `anchor`).
#[derive(Clone, Copy, Debug)]
struct Segment {
    lowered: usize,
    len: usize,
    original: usize,
    copied: bool,
}

#[derive(Default)]
pub struct Edits {
    list: Vec<Edit>,
    seq: usize,
}

/// The result of applying the edits.
pub struct Applied {
    pub text: String,
    segments: Vec<Segment>,
    /// Where each edit's text starts in the lowered text, by `seq`.
    starts: std::collections::HashMap<usize, usize>,
}

impl Edits {
    fn push(&mut self, start: usize, end: usize, text: String, kind: Kind) -> usize {
        self.seq += 1;
        let seq = self.seq;
        self.list.push(Edit {
            start,
            end,
            text,
            kind,
            seq,
        });
        seq
    }

    /// Replace `[start, end)` with `text`.
    pub fn replace(&mut self, start: usize, end: usize, text: impl Into<String>) -> usize {
        self.push(start, end, text.into(), Kind::Replace)
    }

    /// Blank `[start, end)`: spaces, keeping every newline, so nothing after
    /// it moves.
    pub fn blank(&mut self, src: &str, start: usize, end: usize) -> usize {
        // Keep the byte length: a multi-byte character becomes as many spaces.
        let mut out = String::with_capacity(end - start);
        for c in src[start..end].chars() {
            if c == '\n' {
                out.push('\n');
            } else {
                out.extend(std::iter::repeat_n(' ', c.len_utf8()));
            }
        }
        self.push(start, end, out, Kind::Replace)
    }

    /// Open a wrapper at `at`.
    pub fn open(&mut self, at: usize, text: impl Into<String>) -> usize {
        self.push(at, at, text.into(), Kind::Open)
    }

    /// Close a wrapper at `at`.
    pub fn close(&mut self, at: usize, text: impl Into<String>) -> usize {
        self.push(at, at, text.into(), Kind::Close)
    }

    /// Two replacements may not overlap; say which do, if any.
    pub fn overlap(&self) -> Option<(Edit, Edit)> {
        let mut r: Vec<&Edit> = self
            .list
            .iter()
            .filter(|e| e.kind == Kind::Replace && e.end > e.start)
            .collect();
        r.sort_by_key(|e| (e.start, e.end));
        r.windows(2)
            .find(|w| w[1].start < w[0].end)
            .map(|w| (w[0].clone(), w[1].clone()))
    }

    pub fn apply(&self, src: &str) -> Applied {
        let mut list = self.list.clone();
        list.sort_by(|a, b| {
            a.start
                .cmp(&b.start)
                .then(a.kind.cmp(&b.kind))
                .then(match a.kind {
                    Kind::Close => b.seq.cmp(&a.seq),
                    _ => a.seq.cmp(&b.seq),
                })
        });
        let mut text = String::with_capacity(src.len() + 256);
        let mut segments = Vec::new();
        let mut starts = std::collections::HashMap::new();
        let mut cursor = 0usize;
        for e in &list {
            if e.start < cursor {
                // Inside a replacement already applied: an insertion at a
                // point the replacement removed is dropped with it.
                if e.kind != Kind::Replace || e.end <= cursor {
                    continue;
                }
            }
            if e.start > cursor {
                segments.push(Segment {
                    lowered: text.len(),
                    len: e.start - cursor,
                    original: cursor,
                    copied: true,
                });
                text.push_str(&src[cursor..e.start]);
                cursor = e.start;
            }
            starts.insert(e.seq, text.len());
            if !e.text.is_empty() {
                segments.push(Segment {
                    lowered: text.len(),
                    len: e.text.len(),
                    original: e.start,
                    copied: false,
                });
                text.push_str(&e.text);
            }
            cursor = cursor.max(e.end);
        }
        if cursor < src.len() {
            segments.push(Segment {
                lowered: text.len(),
                len: src.len() - cursor,
                original: cursor,
                copied: true,
            });
            text.push_str(&src[cursor..]);
        }
        Applied {
            text,
            segments,
            starts,
        }
    }
}

impl Applied {
    /// The original offset a lowered offset came from: exact in copied
    /// text, the edit's place for text an edit wrote.
    pub fn original(&self, lowered: usize) -> usize {
        let i = self.segments.partition_point(|s| s.lowered <= lowered);
        if i == 0 {
            return lowered;
        }
        let s = self.segments[i - 1];
        if s.copied {
            s.original + (lowered - s.lowered).min(s.len)
        } else {
            s.original
        }
    }

    /// The lowered offset of an original offset that was copied as it is
    /// (a keyword no edit touched).
    pub fn lowered(&self, original: usize) -> Option<usize> {
        self.segments
            .iter()
            .find(|s| s.copied && original >= s.original && original < s.original + s.len)
            .map(|s| s.lowered + (original - s.original))
    }

    /// Was the text at a lowered offset copied from the original (not
    /// written by an edit)?
    pub fn copied(&self, lowered: usize) -> bool {
        let i = self.segments.partition_point(|s| s.lowered <= lowered);
        match i {
            0 => true,
            _ => self.segments[i - 1].copied,
        }
    }

    /// Where the text of edit `seq` begins in the lowered text.
    pub fn edit_start(&self, seq: usize) -> Option<usize> {
        self.starts.get(&seq).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wrappers_nest_and_maps_go_both_ways() {
        let src = "exhale Move {a: 1}\nstick x = 2\n";
        let mut e = Edits::default();
        let p = 7;
        let q = src.find('}').unwrap() + 1;
        e.open(p, "_cg_show(");
        e.close(q, ")");
        e.replace(p, p + 4, "_cg_new_Move(");
        e.close(q, ")");
        let a = e.apply(src);
        assert_eq!(
            a.text,
            "exhale _cg_show(_cg_new_Move( {a: 1}))\nstick x = 2\n"
        );
        let stick = src.find("stick").unwrap();
        let low = a.lowered(stick).unwrap();
        assert_eq!(&a.text[low..low + 5], "stick");
        assert_eq!(a.original(low), stick);
        assert!(e.overlap().is_none());
    }

    #[test]
    fn blank_keeps_lines_and_bytes() {
        let src = "stick x: Int = 1\nrecord A {\n  b: Int\n}\nexhale x\n";
        let mut e = Edits::default();
        let r = src.find("record").unwrap();
        let end = src.find("}\n").unwrap() + 1;
        e.blank(src, r, end);
        let a = e.apply(src);
        assert_eq!(a.text.len(), src.len());
        assert_eq!(a.text.lines().count(), src.lines().count());
        assert!(a.text.ends_with("exhale x\n"));
    }
}
