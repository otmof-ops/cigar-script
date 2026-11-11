// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! CigarScript: CigScript grown deeper, in three strengths.
//!
//! This first slice implements issue #7: strength declarations and the
//! checker's feature gate. A `.cig` file goes straight to cigscript's own
//! parse and check. A `.cigar` file has its `strength` declaration read,
//! the declaration blanked (so cigscript sees exact spans and lines), and
//! the blanked source scanned for features above the file's strength
//! before it, too, is handed to cigscript.

pub mod check;
pub mod cli;
pub mod doctor;
pub mod experience;
pub mod explain;
pub mod features;
pub mod fmt;
pub mod gate;
pub mod lower;
pub mod lsp;
pub mod own;
pub mod phases;
pub mod prepare;
pub mod resolve;
pub mod run;
pub mod strength;
pub mod typeck;
pub mod types;
