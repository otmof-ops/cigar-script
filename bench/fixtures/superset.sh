#!/usr/bin/env sh
# SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
# SPDX-License-Identifier: FSL-1.1-ALv2
# The superset group's inputs (cigar-script#17, #1): cig's shared compute set,
# copied beside each other as it is, once as `.cig` and once as mild `.cigar`
# (the same bytes, no `strength` line), with its `.expect` file. Usage:
#
#     superset.sh OUTDIR
#
# The set is cig's `bench/compute`: from $CIG_BENCH_SET, else
# $CIGSCRIPT_SRC/bench/compute, else the installer's layout beside `cig`
# (<prefix>/share/cig/bench/compute). The manifest's env sets CIGSCRIPT_SRC to
# `../../CigScript`, the sibling checkout where the family's CI puts cig.
set -eu
out="${1:?usage: superset.sh OUTDIR}"
set_dir=""
for d in "${CIG_BENCH_SET:-}" "${CIGSCRIPT_SRC:-}/bench/compute"; do
    if [ -n "$d" ] && [ -d "$d" ]; then
        set_dir="$d"
        break
    fi
done
if [ -z "$set_dir" ]; then
    bin="$(command -v cig 2>/dev/null || true)"
    if [ -n "$bin" ]; then
        d="$(cd "$(dirname "$bin")/.." && pwd)/share/cig/bench/compute"
        [ -d "$d" ] && set_dir="$d"
    fi
fi
if [ -z "$set_dir" ]; then
    echo "superset.sh: cig's bench/compute not found (set CIG_BENCH_SET or CIGSCRIPT_SRC)" >&2
    exit 3
fi
mkdir -p "$out"
for name in closures csv deltablue dispatch fannkuch fib json-deep json lists loop \
    mandel maps nbody regex-ish richards sieve spectral strbuild strings trees words; do
    cp "$set_dir/$name.cig" "$out/$name.cig"
    cp "$set_dir/$name.cig" "$out/$name.cigar"
    cp "$set_dir/$name.expect" "$out/$name.expect"
done
