//! The changes that turn the reference workspace into the synthetic workspaces of the self-test.
//! Each one reads the reference as it is now, so the workspaces stay true when a reference changes.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::score::sources::{self, SourceFile};
use crate::score::test_code::{Body, Suite};
use benchy::contract::Language;

fn io_error(text: String) -> io::Error {
    io::Error::other(text)
}

/// Removes every test: Rust test files and test modules, Go test files, and C# test projects with their solution entries.
pub fn delete_tests(root: &Path) -> io::Result<()> {
    for file in sources::test_files(root, Language::Rust) {
        let path = root.join(&file.path);
        let whole = file
            .code
            .lines()
            .enumerate()
            .all(|(index, line)| line.trim().is_empty() || file.is_test_line(index + 1));
        if whole {
            fs::remove_file(&path)?;
            continue;
        }
        let ranges = sources::rust_test_ranges(&file.code);
        let kept: Vec<&str> = file
            .text
            .lines()
            .enumerate()
            .filter(|(index, _)| !ranges.iter().any(|range| range.contains(index)))
            .map(|(_, line)| line)
            .collect();
        fs::write(&path, kept.join("\n").trim_end().to_string() + "\n")?;
    }
    for file in sources::test_files(root, Language::Go) {
        fs::remove_file(root.join(&file.path))?;
    }
    let mut projects = Vec::new();
    for file in sources::test_files(root, Language::CSharp) {
        let path = root.join(&file.path);
        let project = path
            .ancestors()
            .skip(1)
            .find(|dir| !files_with_extensions(dir, &["csproj"]).is_empty());
        if let Some(dir) = project.filter(|dir| !projects.contains(&dir.to_path_buf())) {
            projects.push(dir.to_path_buf());
        }
    }
    for project in projects {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        fs::remove_dir_all(&project)?;
        for solution in files_with_extensions(&root.join("csharp"), &["sln", "slnx"]) {
            let text = fs::read_to_string(&solution)?;
            let kept: Vec<&str> = text
                .lines()
                .filter(|line| {
                    !line.contains(&format!("{name}/")) && !line.contains(&format!("{name}\\"))
                })
                .collect();
            fs::write(&solution, kept.join("\n") + "\n")?;
        }
    }
    Ok(())
}

fn files_with_extensions(dir: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|e| extensions.contains(&e.to_string_lossy().as_ref()))
        })
        .collect()
}

/// Adds unused functions to the core library of each language until its production lines are twice what they were.
pub fn pad_with_dead_code(root: &Path) -> io::Result<()> {
    for language in Language::ALL {
        let files = sources::production_files(root, language);
        let target = sources::production_line_total(&files);
        let core = files
            .iter()
            .find(|file| !file.is_program)
            .ok_or_else(|| io_error(format!("{} has no core library file", language.as_str())))?;
        let dir = root
            .join(&core.path)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();
        match language {
            Language::Rust => {
                write_padding(&dir.join("padding.rs"), RUST_PADDING, rust_function, target)?;
                add_rust_module(&dir.join("lib.rs"), "padding")?;
            }
            Language::Go => {
                let package = first_word_after(&core.code, "package").unwrap_or("main");
                let header = GO_PADDING.replace("{package}", package);
                write_padding(&dir.join("padding.go"), &header, go_function, target)?;
            }
            Language::CSharp => {
                let namespace = first_word_after(&core.code, "namespace").unwrap_or("Matching");
                let header = CSHARP_PADDING.replace("{namespace}", namespace);
                write_padding(&dir.join("Padding.cs"), &header, csharp_function, target)?;
                let path = dir.join("Padding.cs");
                let text = fs::read_to_string(&path)?;
                fs::write(&path, text + "}\n")?;
            }
        }
    }
    Ok(())
}

const RUST_PADDING: &str =
    "//! Unused functions that the self-test adds to measure what dead code costs.\n";
const GO_PADDING: &str = "// Unused functions that the self-test adds to measure what dead code costs.\n\npackage {package}\n";
const CSHARP_PADDING: &str = "// Unused methods that the self-test adds to measure what dead code costs.\nnamespace {namespace};\n\ninternal static class Padding\n{\n";

fn rust_function(index: usize) -> String {
    format!(
        "\nfn padding_{index}(value: i64) -> i64 {{\n    let doubled = value * 2;\n    if doubled > {index} {{\n        doubled - {index}\n    }} else {{\n        doubled + {index}\n    }}\n}}\n"
    )
}

fn go_function(index: usize) -> String {
    format!(
        "\nfunc padding{index}(value int64) int64 {{\n\tdoubled := value * 2\n\tif doubled > {index} {{\n\t\treturn doubled - {index}\n\t}}\n\treturn doubled + {index}\n}}\n"
    )
}

fn csharp_function(index: usize) -> String {
    let separator = if index == 1 { "" } else { "\n" };
    format!(
        "{separator}    private static long Padding{index}(long value)\n    {{\n        long doubled = value * 2;\n        if (doubled > {index})\n        {{\n            return doubled - {index};\n        }}\n\n        return doubled + {index};\n    }}\n"
    )
}

/// Writes `header` and as many functions as it takes to add `target` lines that are not blank.
fn write_padding(
    path: &Path,
    header: &str,
    function: fn(usize) -> String,
    target: usize,
) -> io::Result<()> {
    let mut text = header.to_string();
    let mut added = 0;
    let mut index = 0;
    while added < target {
        index += 1;
        let next = function(index);
        added += next.lines().filter(|line| !line.trim().is_empty()).count();
        text.push_str(&next);
    }
    fs::write(path, text)
}

fn first_word_after<'a>(code: &'a str, keyword: &str) -> Option<&'a str> {
    code.lines().find_map(|line| {
        line.trim()
            .strip_prefix(keyword)
            .filter(|rest| rest.starts_with(' '))
            .map(|rest| rest.trim().trim_end_matches(';'))
    })
}

/// Declares `module` in `lib`, in sorted order among the first run of `mod` lines, as rustfmt keeps them.
fn add_rust_module(lib: &Path, module: &str) -> io::Result<()> {
    let text = fs::read_to_string(lib)?;
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let declaration = format!("mod {module};");
    let is_mod = |line: &str| line.starts_with("mod ") && line.ends_with(';');
    let first = lines.iter().position(|line| is_mod(line));
    let position = match first {
        Some(first) => {
            let mut position = first;
            while position < lines.len()
                && is_mod(&lines[position])
                && lines[position] < declaration
            {
                position += 1;
            }
            position
        }
        None => {
            let after_docs = lines
                .iter()
                .position(|line| !line.starts_with("//!"))
                .unwrap_or(0);
            lines.insert(after_docs, String::new());
            after_docs + 1
        }
    };
    lines.insert(position, declaration);
    fs::write(lib, lines.join("\n") + "\n")
}

/// Puts an early return behind an assertion that always holds at the top of every test, so each test passes and checks nothing.
pub fn make_tests_tautological(root: &Path) -> io::Result<()> {
    for language in Language::ALL {
        let suite = Suite::read(root, language);
        for (index, file) in suite.files.iter().enumerate() {
            let mut chars: Vec<char> = file.text.chars().collect();
            let mut tests: Vec<_> = suite.tests().filter(|test| test.file == index).collect();
            tests.sort_by_key(|test| std::cmp::Reverse(body_start(test.body)));
            for test in tests {
                let start = body_start(test.body);
                let line_start = chars[..start]
                    .iter()
                    .rposition(|c| *c == '\n')
                    .map_or(0, |p| p + 1);
                let signature: String = chars[line_start..start].iter().collect();
                let returns_value = signature.contains("->");
                match (language, test.body) {
                    (Language::CSharp, Body::Expression { arrow, end }) => {
                        let expression: String = chars[arrow + 2..end].iter().collect();
                        let replacement =
                            format!("{{ {CSHARP_TAUTOLOGY} {}; }}", expression.trim());
                        chars.splice(arrow..=end, replacement.chars());
                    }
                    (_, Body::Block { open, .. }) => {
                        let inserted = match language {
                            Language::Rust if returns_value => {
                                " assert_eq!(2 + 2, 4); if std::hint::black_box(true) { return Ok(()); }".to_string()
                            }
                            Language::Rust => {
                                " assert_eq!(2 + 2, 4); if std::hint::black_box(true) { return; }".to_string()
                            }
                            Language::Go => {
                                let t = test.parameter.as_deref().unwrap_or("t");
                                format!(" if 2+2 != 4 {{ {t}.Fatal(\"arithmetic is broken\") }}; if 2+2 == 4 {{ return }};")
                            }
                            Language::CSharp => format!(" {CSHARP_TAUTOLOGY}"),
                        };
                        chars.splice(open + 1..open + 1, inserted.chars());
                    }
                    (_, Body::Expression { .. }) => {}
                }
            }
            fs::write(root.join(&file.path), chars.into_iter().collect::<String>())?;
        }
    }
    Ok(())
}

const CSHARP_TAUTOLOGY: &str =
    "Assert.Equal(4, 2 + 2); if (Environment.ProcessorCount > 0) { return; }";

fn body_start(body: Body) -> usize {
    match body {
        Body::Block { open, .. } => open,
        Body::Expression { arrow, .. } => arrow,
    }
}

/// One stored change of a reference file: text that occurs exactly once, and what replaces it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub language: Language,
    /// Path inside the language folder.
    pub file: PathBuf,
    pub find: String,
    pub replace: String,
}

/// The broken feature: what it is and the change to each reference that breaks it.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Breakage {
    pub feature: String,
    #[serde(rename = "change")]
    pub changes: Vec<Change>,
}

/// Applies every change of `breakage`.
pub fn break_feature(root: &Path, breakage: &Breakage) -> io::Result<()> {
    for change in &breakage.changes {
        let path = root.join(change.language.as_str()).join(&change.file);
        let text = fs::read_to_string(&path)?;
        if text.matches(&change.find).count() != 1 {
            return Err(io_error(format!(
                "{} must hold {:?} exactly once; correct validation/selftest/broken.toml",
                path.display(),
                change.find
            )));
        }
        fs::write(&path, text.replacen(&change.find, &change.replace, 1))?;
    }
    Ok(())
}

/// Puts a doc comment that only restates the name above every public item of the production code.
pub fn add_filler_docs(root: &Path) -> io::Result<()> {
    for language in Language::ALL {
        for file in sources::production_files(root, language) {
            let text = filler_docs(&file, language);
            fs::write(root.join(&file.path), text)?;
        }
    }
    Ok(())
}

fn filler_docs(file: &SourceFile, language: Language) -> String {
    let code: Vec<&str> = file.code.lines().collect();
    let text: Vec<&str> = file.text.lines().collect();
    let mut inserts: Vec<(usize, String)> = Vec::new();
    for (index, line) in code.iter().enumerate() {
        if !file.is_production_line(index + 1) {
            continue;
        }
        let Some(name) = public_name(line, language) else {
            continue;
        };
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        let doc = match language {
            Language::Rust => format!("{indent}/// The {name}."),
            Language::Go => format!("{indent}// {name} is the {name}."),
            Language::CSharp => format!("{indent}/// <summary>The {name}.</summary>"),
        };
        let mut above = index;
        while above > 0 {
            let previous = text[above - 1].trim();
            let is_attribute = previous.starts_with("#[")
                || (language == Language::CSharp && previous.starts_with('['));
            let is_doc = match language {
                Language::Rust | Language::CSharp => previous.starts_with("///"),
                Language::Go => previous.starts_with("//"),
            };
            if is_attribute || is_doc {
                above -= 1;
            } else {
                break;
            }
        }
        // gofmt wants a blank line before a doc comment and no column alignment across it.
        let needs_blank = language == Language::Go
            && above > 0
            && !text[above - 1].trim().is_empty()
            && !text[above - 1].trim_end().ends_with(['{', '(']);
        inserts.push((above, if needs_blank { format!("\n{doc}") } else { doc }));
    }
    let mut lines: Vec<String> = text.iter().map(|line| (*line).to_string()).collect();
    if language == Language::Go {
        for line in &mut lines {
            if line.starts_with("func ") && line.trim_end().ends_with('}') {
                *line = without_alignment(line);
            }
        }
    }
    for (at, doc) in inserts.into_iter().rev() {
        lines.insert(at, doc);
    }
    lines.join("\n") + "\n"
}

/// A one-line Go function with the spaces that aligned its body to its neighbours cut to one.
fn without_alignment(line: &str) -> String {
    match line.find("  {") {
        Some(at) => format!("{} {}", line[..at].trim_end(), line[at..].trim_start()),
        None => line.to_string(),
    }
}

/// The name of the public item declared on `line`, if it declares one.
fn public_name(line: &str, language: Language) -> Option<String> {
    let trimmed = line.trim();
    let word_after = |text: &str, keywords: &[&str]| {
        let words: Vec<&str> = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|w| !w.is_empty())
            .collect();
        words
            .windows(2)
            .find(|pair| keywords.contains(&pair[0]))
            .map(|pair| pair[1].to_string())
    };
    match language {
        Language::Rust => {
            let rest = trimmed.strip_prefix("pub ")?;
            if rest.starts_with("use ") {
                return None;
            }
            word_after(
                trimmed,
                &[
                    "fn", "struct", "enum", "const", "static", "trait", "type", "mod",
                ],
            )
            .or_else(|| rest.split(':').next().map(|field| field.trim().to_string()))
            .filter(|name| {
                !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
            })
        }
        Language::Go => {
            if line.starts_with(char::is_whitespace) {
                return None;
            }
            let rest = trimmed.strip_prefix("func ").map(|rest| {
                if rest.starts_with('(') {
                    rest.split_once(") ").map_or("", |(_, after)| after)
                } else {
                    rest
                }
            });
            let name = match rest {
                Some(rest) => rest
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .next()?
                    .to_string(),
                None => word_after(trimmed, &["type", "var", "const"])?,
            };
            name.starts_with(char::is_uppercase).then_some(name)
        }
        Language::CSharp => {
            if !trimmed.starts_with("public ") {
                return None;
            }
            let declared = word_after(trimmed, &["class", "record", "struct", "interface", "enum"]);
            let before_paren = trimmed.split('(').next().filter(|_| trimmed.contains('('));
            declared
                .or_else(|| {
                    before_paren
                        .and_then(|text| text.split_whitespace().last())
                        .map(str::to_string)
                })
                .or_else(|| {
                    trimmed
                        .split(['{', '=', ';'])
                        .next()
                        .and_then(|text| text.split_whitespace().last())
                        .map(str::to_string)
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_names_are_read_in_each_language() {
        assert_eq!(
            public_name("pub fn new() -> Engine {", Language::Rust).as_deref(),
            Some("new")
        );
        assert_eq!(
            public_name("    pub price: Price,", Language::Rust).as_deref(),
            Some("price")
        );
        assert_eq!(public_name("pub use event::Event;", Language::Rust), None);
        assert_eq!(
            public_name("func (e *Engine) Apply(line []byte) {", Language::Go).as_deref(),
            Some("Apply")
        );
        assert_eq!(public_name("func helper() {", Language::Go), None);
        assert_eq!(
            public_name("type Reason string", Language::Go).as_deref(),
            Some("Reason")
        );
        assert_eq!(
            public_name("    public sealed record NewOrder(", Language::CSharp).as_deref(),
            Some("NewOrder")
        );
        assert_eq!(
            public_name(
                "    public void Execute(ReadOnlySpan<byte> line, long n)",
                Language::CSharp
            )
            .as_deref(),
            Some("Execute")
        );
        assert_eq!(
            public_name(
                "    public long Open { get; private set; }",
                Language::CSharp
            )
            .as_deref(),
            Some("Open")
        );
    }

    #[test]
    fn a_module_is_declared_in_sorted_order() {
        let dir = tempfile::tempdir().expect("temp dir");
        let lib = dir.path().join("lib.rs");
        fs::write(&lib, "//! Doc.\n\nmod book;\nmod parse;\n\nuse x;\n").expect("write");

        add_rust_module(&lib, "padding").expect("add");

        assert_eq!(
            fs::read_to_string(&lib).expect("read"),
            "//! Doc.\n\nmod book;\nmod padding;\nmod parse;\n\nuse x;\n"
        );
    }
}
