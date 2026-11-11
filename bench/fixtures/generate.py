#!/usr/bin/env python3
# SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
# SPDX-License-Identifier: FSL-1.1-ALv2
"""Generate the checker's benchmark programs (cigar-script#17).

    generate.py OUTDIR            write the files into OUTDIR
    generate.py --check OUTDIR    write them, then compare with SHA256SUMS

The output depends on nothing but this file: no clock, no random numbers, no
locale, no platform. Every file is UTF-8 with LF line ends, so the sha256 in
SHA256SUMS is the same on every machine (the test
`generated_fixtures_are_reproducible` holds it to that).

Files:

  size-<N>-<strength>.cigar       the same program at mild, medium and full, with
                                  exactly N lines (100, 1000, 10000, 100000)
  generics-<N>-<strength>.cigar   generics-heavy (inference at every call), N of
                                  1000 and 10000, medium and full
  match-<N>-<strength>.cigar      pattern matching over enums with five variants,
                                  every match exhaustive, N of 1000 and 10000,
                                  medium and full

A file's first line is its `strength` declaration, which counts in N.
"""
import hashlib
import os
import sys

SIZES = (100, 1000, 10000, 100000)
HEAVY_SIZES = (1000, 10000)


def size_unit(i):
    """Five lines of plain CigScript, valid and warning-free at every strength."""
    return [
        "stick a%d = %d" % (i, i),
        "stick b%d = a%d + %d" % (i, i, i),
        "stick c%d = b%d * 2" % (i, i),
        "stick d%d = c%d - a%d" % (i, i, i),
        "exhale d%d" % i,
    ]


def generics_unit(i):
    """Six lines: two generic pulls, and a call whose type arguments are inferred."""
    return [
        "pull id%d<T>(x: T) -> T { snuff x }" % i,
        "pull pick%d<T>(_xs: List<T>, d: T) -> T {" % i,
        "  snuff d",
        "}",
        "stick n%d: Int = pick%d([1, 2, 3], id%d(%d))" % (i, i, i, i),
        "exhale n%d" % i,
    ]


def match_unit(i):
    """Six lines: an enum of five variants and a match that covers all five."""
    return [
        "enum S%d { ok, failed(reason: Text), skipped, waiting(n: Int), done }" % i,
        "pull code%d(s: S%d) -> Int {" % (i, i),
        "  snuff match s { ok => 0, failed(r) => 1, skipped => 2, waiting(n) => 3, done => 4 }",
        "}",
        "stick c%d: Int = code%d(S%d.skipped)" % (i, i, i),
        "exhale c%d" % i,
    ]


def program(unit, strength, lines):
    """`lines` lines in all: the declaration, whole units, then `exhale` lines as padding."""
    out = ["strength %s" % strength]
    i = 1
    while True:
        u = unit(i)
        if len(out) + len(u) > lines:
            break
        out.extend(u)
        i += 1
    k = 1
    while len(out) < lines:
        out.append("exhale %d" % k)
        k += 1
    return "\n".join(out) + "\n"


def files():
    for n in SIZES:
        for s in ("mild", "medium", "full"):
            yield "size-%d-%s.cigar" % (n, s), program(size_unit, s, n)
    for n in HEAVY_SIZES:
        for s in ("medium", "full"):
            yield "generics-%d-%s.cigar" % (n, s), program(generics_unit, s, n)
            yield "match-%d-%s.cigar" % (n, s), program(match_unit, s, n)


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def main(argv):
    check = "--check" in argv
    args = [a for a in argv if a != "--check"]
    if len(args) != 1:
        sys.stderr.write(__doc__)
        return 3
    out = args[0]
    os.makedirs(out, exist_ok=True)
    sums = []
    for name, text in files():
        data = text.encode("utf-8")
        path = os.path.join(out, name)
        # A case's setup runs this for every case: leave a file that is right.
        try:
            with open(path, "rb") as f:
                same = f.read() == data
        except OSError:
            same = False
        if not same:
            with open(path, "wb") as f:
                f.write(data)
        sums.append("%s  %s\n" % (sha256(data), name))
    listing = "".join(sums)
    sys.stdout.write(listing)
    if not check:
        return 0
    here = os.path.dirname(os.path.abspath(__file__))
    with open(os.path.join(here, "SHA256SUMS"), encoding="utf-8") as f:
        want = f.read()
    if want != listing:
        sys.stderr.write("generated files differ from SHA256SUMS\n")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
