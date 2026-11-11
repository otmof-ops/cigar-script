#!/usr/bin/env sh
# SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
# SPDX-License-Identifier: FSL-1.1-ALv2
# Run one superset program and hold its output to cig's own .expect file
# (cigar-script#17, #1): exit 0 only when the program exits 0 and prints
# exactly what the file holds. Usage:
#
#     same.sh cig|cigar FILE
#
# The harness reads an `expect.stdout` file before a case's `setup` has made
# it, so the check is made here, the same way for both sides; the cost of the
# shell and the compare is in both columns.
set -eu
tool="${1:?usage: same.sh cig|cigar FILE}"
file="${2:?usage: same.sh cig|cigar FILE}"
expect="${file%.*}.expect"
out="$(mktemp)"
trap 'rm -f "$out"' EXIT
"$tool" run "$file" > "$out"
cmp -s "$out" "$expect"
