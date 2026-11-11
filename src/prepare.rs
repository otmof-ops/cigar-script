// SPDX-FileCopyrightText: 2026 Jay Taylor (https://github.com/otmof-ops/cigar-script)
// SPDX-License-Identifier: FSL-1.1-ALv2
//! A `.cigar` file, ready for a compiler (#7, otmof-ops/cig-compile#7).
//!
//! `cigcompile build x.cigar` checks the file as `cigar check` does and
//! compiles the CigScript it lowers to. This is the one call it makes: the
//! file in, the CigScript out, every error and warning rendered here the
//! way `cig check` renders them. It takes and returns plain types (strings,
//! booleans, [`Strength`]) so the compiler's own cigscript version does not
//! have to be the one cigar was built on.

use std::path::Path;

use cigscript::cli::Ctx;

use crate::check::{check_file_source, Kind};
use crate::features::Strength;

/// How diagnostics are shown: the flags `cigcompile` was given.
#[derive(Clone, Copy, Debug, Default)]
pub struct Render {
    pub json: bool,
    pub plain: bool,
    pub color: bool,
}

/// A `.cigar` file that checked.
#[derive(Debug)]
pub struct Prepared {
    /// The CigScript to compile: what `cigar run` runs for this file.
    pub source: String,
    /// The strength it was checked at: the declaration, or `--strength`.
    pub strength: Strength,
    /// Each warning, rendered, for the caller to say (or keep to say again).
    pub warnings: Vec<String>,
}

/// Check the `.cigar` file at `path` (its text is `text`; `shown` is the
/// name to print) at `strength` (the declaration's, when none). On a check
/// that fails, every error is reported here and `Err(())` comes back.
#[allow(clippy::result_unit_err)]
pub fn prepare(
    path: &Path,
    shown: &str,
    text: &str,
    strength: Option<Strength>,
    render: Render,
) -> Result<Prepared, ()> {
    let checked = check_file_source(Kind::Cigar, text, strength, Some(path));
    if !checked.ok() {
        let ctx = Ctx {
            json: render.json,
            color: render.color,
            doctor: false,
            plain: render.plain || cigscript::cli::plain_by_env(),
        };
        if ctx.json {
            for d in &checked.errors {
                cigscript::cli::report(&ctx, d, Some(shown), Some(&checked.source_as_written));
            }
        } else {
            cigscript::cli::report_all(&ctx, &checked.errors, shown, &checked.source_as_written);
        }
        return Err(());
    }
    let warnings = checked
        .warnings
        .iter()
        .map(|w| {
            w.render(Some(shown), Some(&checked.source_as_written))
                .replacen("error[", "warning[", 1)
        })
        .collect();
    Ok(Prepared {
        source: checked.source_as_cigscript_saw_it,
        strength: checked.strength,
        warnings,
    })
}
