// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! `cigar-lsp`: the CigarScript language server. It speaks the Language
//! Server Protocol on stdin and stdout; an editor starts it with no
//! arguments. Nothing but protocol messages is ever written to stdout.

use std::io::{self, BufReader};

fn main() {
    let code = cigarscript::lsp::serve(BufReader::new(io::stdin()), io::stdout().lock());
    std::process::exit(code);
}
