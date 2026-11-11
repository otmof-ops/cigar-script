#!/usr/bin/env sh
# SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
# SPDX-License-Identifier: FSL-1.1-ALv2
# Generate the 1,000-line mild fixture, plus one with a `strength
# medium` declaration and one with `strength full`. The content is
# plain CigScript (mild CigarScript): a thousand sticks numbered in
# order, so the file is parseable at every strength and nothing in it
# trips the feature gate.
set -eu
here="$(cd "$(dirname "$0")" && pwd)"
mild="$here/big-mild.cigar"
medium="$here/big-medium.cigar"
full="$here/big-full.cigar"

make_mild() {
    i=1
    while [ "$i" -le 1000 ]; do
        printf 'stick n%d = %d\n' "$i" "$i"
        i=$((i + 1))
    done
}

if [ ! -s "$mild" ]; then
    make_mild > "$mild"
fi
if [ ! -s "$medium" ]; then
    { echo "strength medium"; cat "$mild"; } > "$medium"
fi
if [ ! -s "$full" ]; then
    { echo "strength full"; cat "$mild"; } > "$full"
fi
