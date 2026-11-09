//! The feature table, generated at build time from docs/LANGUAGE.md.

use std::fmt;

/// The three strengths. Mild is included only as the default; mild never
/// appears as a feature's strength in the table (everything mild is what
/// CigScript already has).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Strength {
    Mild,
    Medium,
    Full,
}

impl Strength {
    pub fn word(self) -> &'static str {
        match self {
            Strength::Mild => "mild",
            Strength::Medium => "medium",
            Strength::Full => "full",
        }
    }

    pub fn parse(word: &str) -> Option<Strength> {
        Some(match word {
            "mild" => Strength::Mild,
            "medium" => Strength::Medium,
            "full" => Strength::Full,
            _ => return None,
        })
    }
}

impl fmt::Display for Strength {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.word())
    }
}

/// How the checker spots a use of a feature.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detect {
    /// A particular word in the file (as an identifier token).
    Word(&'static str),
    /// A type annotation on `stick`, `roll`, a `pull` parameter or result.
    Annotation,
}

/// One row of the feature table.
#[derive(Clone, Copy, Debug)]
pub struct Feature {
    /// What it is called, as written in the docs.
    pub name: &'static str,
    /// The strength that allows it.
    pub strength: Strength,
    /// How the checker finds a use of it.
    pub detect: Detect,
    /// The issue the feature waits on, written as `#<number>`.
    pub issue: &'static str,
    /// Whether this version has built it yet. `false` means a file at
    /// the right strength gets E1102 instead of a result.
    pub built: bool,
}

include!(concat!(env!("OUT_DIR"), "/features.rs"));
