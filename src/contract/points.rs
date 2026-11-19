//! The three languages and the nine scoring categories, which every scoring step names the same way.

use serde::{Deserialize, Serialize};

/// A language that the task asks the harness to build the engine in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    Go,
    #[serde(rename = "csharp")]
    CSharp,
}

impl Language {
    /// Every language, in the order that reports and the TSV row use.
    pub const ALL: [Language; 3] = [Language::Rust, Language::Go, Language::CSharp];

    /// The lowercase name used in paths, `Makefile` targets and TSV columns.
    pub fn as_str(self) -> &'static str {
        match self {
            Language::Rust => "rust",
            Language::Go => "go",
            Language::CSharp => "csharp",
        }
    }
}

/// One of the scoring categories A to I of a language score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CategoryId {
    A,
    B,
    C,
    D,
    E,
    F,
    G,
    H,
    I,
}

impl CategoryId {
    /// Every category, in rubric order.
    pub const ALL: [CategoryId; 9] = [
        CategoryId::A,
        CategoryId::B,
        CategoryId::C,
        CategoryId::D,
        CategoryId::E,
        CategoryId::F,
        CategoryId::G,
        CategoryId::H,
        CategoryId::I,
    ];

    /// The single letter used in the rubric, metric keys and TSV columns.
    pub fn as_str(self) -> &'static str {
        match self {
            CategoryId::A => "A",
            CategoryId::B => "B",
            CategoryId::C => "C",
            CategoryId::D => "D",
            CategoryId::E => "E",
            CategoryId::F => "F",
            CategoryId::G => "G",
            CategoryId::H => "H",
            CategoryId::I => "I",
        }
    }
}

/// The points one language earned in one category, beside the most it could earn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CategoryPoints {
    pub category: CategoryId,
    pub earned: f64,
    pub max: f64,
}
