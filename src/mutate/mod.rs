//! The one mutator for all three languages: the same five operators find candidate mutants in Rust, Go and C# alike.
//! A seeded sample of them is what category D runs the tests against, so the same seed and source give the same mutants.

pub mod run;

use std::fmt;
use std::path::{Path, PathBuf};

use benchy::contract::Language;

/// The seed of the mutant sample. It is part of the scoring rules: changing it changes every mutation score.
pub const SEED: u64 = 0x6d75_7461_6e74_7331;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Operator {
    /// `<` becomes `>`, `<=` becomes `>=`, `==` becomes `!=`, and back.
    FlipComparison,
    /// `<` becomes `<=`, `>=` becomes `>`, and back.
    ShiftBoundary,
    /// `+` and `-` swap, and so do `*` and `/`, also in compound assignments.
    SwapArithmetic,
    /// The condition of an `if` is wrapped in a negation.
    NegateCondition,
    /// An integer literal changes by one, or `true` and `false` swap.
    ChangeConstant,
}

impl Operator {
    pub fn as_str(self) -> &'static str {
        match self {
            Operator::FlipComparison => "flip comparison",
            Operator::ShiftBoundary => "shift boundary",
            Operator::SwapArithmetic => "swap arithmetic",
            Operator::NegateCondition => "negate condition",
            Operator::ChangeConstant => "change constant",
        }
    }
}

/// One change to one place of one source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mutant {
    /// Path relative to the workspace.
    pub file: PathBuf,
    /// Line number, counted from 1.
    pub line: usize,
    /// Position in the line, in characters, counted from 0.
    pub column: usize,
    pub operator: Operator,
    pub original: String,
    pub replacement: String,
}

impl fmt::Display for Mutant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}:{} {} `{}` -> `{}`",
            self.file.display(),
            self.line,
            self.operator.as_str(),
            self.original,
            self.replacement
        )
    }
}

impl Mutant {
    /// The source text with this mutant in place, or None when `text` does not hold the original at its place.
    pub fn apply(&self, text: &str) -> Option<String> {
        let mut offset = 0;
        for (index, line) in text.split_inclusive('\n').enumerate() {
            if index + 1 == self.line {
                let start = offset + line.char_indices().nth(self.column)?.0;
                if !text[start..].starts_with(&self.original) {
                    return None;
                }
                let end = start + self.original.len();
                return Some(format!(
                    "{}{}{}",
                    &text[..start],
                    self.replacement,
                    &text[end..]
                ));
            }
            offset += line.len();
        }
        None
    }
}

/// Every mutant of the lines of one file for which `eligible` holds, in line and column order.
/// `code` is the file's text with comments and string contents blanked, line for line and character for character,
/// so no mutant ever lands in a comment or a string.
pub fn candidates(
    path: &Path,
    code: &str,
    language: Language,
    eligible: impl Fn(usize) -> bool,
) -> Vec<Mutant> {
    let mut found = Vec::new();
    for (index, line) in code.lines().enumerate() {
        let number = index + 1;
        if !eligible(number) || line.trim_start().starts_with("#[") {
            continue;
        }
        let mut changes = binary_operators(line);
        changes.extend(negated_condition(line, language));
        changes.extend(constants(line));
        changes.sort_by_key(|change| (change.column, change.operator));
        found.extend(changes.into_iter().map(|change| Mutant {
            file: path.to_path_buf(),
            line: number,
            column: change.column,
            operator: change.operator,
            original: change.original,
            replacement: change.replacement,
        }));
    }
    found
}

/// Draws `count` mutants from `candidates` with a seeded shuffle, and returns them in their original order.
/// The same candidates and seed always give the same sample.
pub fn sample(candidates: &[Mutant], count: usize, seed: u64) -> Vec<Mutant> {
    let mut order: Vec<usize> = (0..candidates.len()).collect();
    let mut random = SplitMix(seed);
    let take = count.min(order.len());
    for position in 0..take {
        let remaining = (order.len() - position) as u64;
        let pick = position + usize::try_from(random.next() % remaining).expect("fits in usize");
        order.swap(position, pick);
    }
    let mut chosen = order[..take].to_vec();
    chosen.sort_unstable();
    chosen.into_iter().map(|i| candidates[i].clone()).collect()
}

/// SplitMix64. The sample keeps its own generator so that a change to the input generators never moves it.
struct SplitMix(u64);

impl SplitMix {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

struct Change {
    column: usize,
    operator: Operator,
    original: String,
    replacement: String,
}

impl Change {
    fn new(column: usize, operator: Operator, original: &str, replacement: &str) -> Change {
        Change {
            column,
            operator,
            original: original.to_string(),
            replacement: replacement.to_string(),
        }
    }
}

/// The binary operators and what each becomes. Longer operators come first so `<=` is never read as `<`.
const BINARY: [(&str, &[(Operator, &str)]); 14] = [
    (
        "<=",
        &[
            (Operator::FlipComparison, ">="),
            (Operator::ShiftBoundary, "<"),
        ],
    ),
    (
        ">=",
        &[
            (Operator::FlipComparison, "<="),
            (Operator::ShiftBoundary, ">"),
        ],
    ),
    ("==", &[(Operator::FlipComparison, "!=")]),
    ("!=", &[(Operator::FlipComparison, "==")]),
    ("+=", &[(Operator::SwapArithmetic, "-=")]),
    ("-=", &[(Operator::SwapArithmetic, "+=")]),
    ("*=", &[(Operator::SwapArithmetic, "/=")]),
    ("/=", &[(Operator::SwapArithmetic, "*=")]),
    (
        "<",
        &[
            (Operator::FlipComparison, ">"),
            (Operator::ShiftBoundary, "<="),
        ],
    ),
    (
        ">",
        &[
            (Operator::FlipComparison, "<"),
            (Operator::ShiftBoundary, ">="),
        ],
    ),
    ("+", &[(Operator::SwapArithmetic, "-")]),
    ("-", &[(Operator::SwapArithmetic, "+")]),
    ("*", &[(Operator::SwapArithmetic, "/")]),
    ("/", &[(Operator::SwapArithmetic, "*")]),
];

/// Binary operators with a space on each side. All three formatters write binary operators that way, and
/// generics, arrows, pointers and unary signs never have it, so the spaces tell operators from the rest.
fn binary_operators(line: &str) -> Vec<Change> {
    let chars: Vec<char> = line.chars().collect();
    let mut changes = Vec::new();
    for start in 1..chars.len() {
        if chars[start - 1] != ' ' {
            continue;
        }
        for (operator, replacements) in BINARY {
            let length = operator.chars().count();
            let matches = chars[start..]
                .iter()
                .take(length)
                .copied()
                .eq(operator.chars());
            if matches && chars.get(start + length) == Some(&' ') {
                for (kind, replacement) in replacements {
                    changes.push(Change::new(start, *kind, operator, replacement));
                }
                break;
            }
        }
    }
    changes
}

/// The condition of an `if` that fits on one line, wrapped in a negation.
fn negated_condition(line: &str, language: Language) -> Option<Change> {
    let chars: Vec<char> = line.chars().collect();
    let keyword = find_word(&chars, "if")?;
    let after = keyword + 2;
    let (start, end) = if language == Language::CSharp {
        let open = after + count_spaces(&chars[after..]);
        if chars.get(open) != Some(&'(') {
            return None;
        }
        let close = matching_paren(&chars, open)?;
        (open + 1, close)
    } else {
        if chars.get(after) != Some(&' ') || !line.trim_end().ends_with(" {") {
            return None;
        }
        let brace = line.trim_end().chars().count() - 1;
        let mut start = after + 1;
        let condition: String = chars[start..brace].iter().collect();
        // Go allows a statement before the condition: `if v, ok := m[k]; ok {`.
        if let Some(semicolon) = condition.rfind("; ") {
            start += condition[..semicolon].chars().count() + 2;
        }
        (start, brace - 1)
    };
    let condition: String = chars.get(start..end)?.iter().collect();
    let condition = condition.trim_end();
    let unsuitable = condition.is_empty()
        || condition.starts_with("let ")
        || condition.contains("=>")
        || condition.contains(" = ")
        || condition.contains(" := ");
    if unsuitable {
        return None;
    }
    Some(Change::new(
        start,
        Operator::NegateCondition,
        condition,
        &format!("!({condition})"),
    ))
}

/// The position of `word` where it stands alone, not as part of a longer name.
fn find_word(chars: &[char], word: &str) -> Option<usize> {
    let length = word.chars().count();
    (0..chars.len()).find(|&start| {
        let before = start.checked_sub(1).map(|i| chars[i]);
        let after = chars.get(start + length);
        chars[start..].iter().take(length).copied().eq(word.chars())
            && !before.is_some_and(is_name_char)
            && !after.is_some_and(|c| is_name_char(*c))
    })
}

fn count_spaces(chars: &[char]) -> usize {
    chars.iter().take_while(|c| **c == ' ').count()
}

fn matching_paren(chars: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, c) in chars.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn is_name_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Decimal integer literals, which change by one (and 1 becomes 0), and the words `true` and `false`, which swap.
fn constants(line: &str) -> Vec<Change> {
    let chars: Vec<char> = line.chars().collect();
    let mut changes = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let before = index.checked_sub(1).map(|i| chars[i]);
        let starts_token = !before.is_some_and(|c| is_name_char(c) || c == '.');
        if !starts_token {
            index += 1;
            continue;
        }
        if chars[index].is_ascii_digit() {
            let digits = chars[index..]
                .iter()
                .take_while(|c| c.is_ascii_digit() || **c == '_')
                .count();
            let end = index + digits;
            let is_float = chars.get(end) == Some(&'.')
                && chars.get(end + 1).is_some_and(char::is_ascii_digit);
            let is_prefixed = chars[index] == '0'
                && chars
                    .get(index + 1)
                    .is_some_and(|c| matches!(c, 'x' | 'b' | 'o' | 'X' | 'B' | 'O'));
            let original: String = chars[index..end].iter().collect();
            let value = original.replace('_', "").parse::<u64>().ok();
            if let (false, false, Some(value)) = (is_float, is_prefixed, value) {
                let changed = if value == 1 { 0 } else { value + 1 };
                changes.push(Change::new(
                    index,
                    Operator::ChangeConstant,
                    &original,
                    &changed.to_string(),
                ));
            }
            index = end;
            continue;
        }
        for (word, swapped) in [("true", "false"), ("false", "true")] {
            let length = word.len();
            let matches = chars[index..].iter().take(length).copied().eq(word.chars());
            let after = chars.get(index + length);
            if matches && !after.is_some_and(|c| is_name_char(*c)) {
                changes.push(Change::new(index, Operator::ChangeConstant, word, swapped));
            }
        }
        index += 1;
    }
    changes
}

#[cfg(test)]
mod tests;
