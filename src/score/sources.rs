//! Finds the production source files of one language in a workspace and tells code lines from comments and tests.
//! Every count of lines, hazards and functions reads files through here, so all of them agree on what production code is.

use std::fs;
use std::path::{Path, PathBuf};

use benchy::contract::Language;

/// One source file, with comments and the insides of string literals blanked out.
#[derive(Debug, Clone)]
pub struct SourceFile {
    /// Path relative to the workspace.
    pub path: PathBuf,
    pub text: String,
    /// The text with comments and string contents replaced by spaces; line breaks are kept.
    pub code: String,
    /// For each line, whether it holds production code: not blank, not only comment, not test code.
    pub production_lines: Vec<bool>,
    /// The file belongs to the program rather than the core library.
    pub is_program: bool,
    /// Lines outside test code that hold only a comment.
    pub comment_lines: usize,
}

impl SourceFile {
    pub fn production_line_count(&self) -> usize {
        self.production_lines.iter().filter(|&&line| line).count()
    }

    /// Whether line `number`, counted from 1, is production code.
    pub fn is_production_line(&self, number: usize) -> bool {
        number
            .checked_sub(1)
            .and_then(|index| self.production_lines.get(index))
            .copied()
            .unwrap_or(false)
    }
}

const SKIPPED_DIRS: [&str; 7] = [
    "target",
    "bin",
    "obj",
    "vendor",
    "node_modules",
    "testdata",
    ".git",
];

/// The production files of `language` below `workspace`, sorted by path. Test files are left out
/// and test modules inside production files are not production lines.
pub fn production_files(workspace: &Path, language: Language) -> Vec<SourceFile> {
    let root = workspace.join(language.as_str());
    let extension = match language {
        Language::Rust => "rs",
        Language::Go => "go",
        Language::CSharp => "cs",
    };
    let mut paths = Vec::new();
    walk(&root, extension, &mut paths);
    paths.sort();
    let test_modules = match language {
        Language::Rust => rust_test_module_files(&paths),
        _ => Vec::new(),
    };
    let mut files = Vec::new();
    for path in paths {
        let relative = path.strip_prefix(workspace).unwrap_or(&path).to_path_buf();
        if is_test_file(&root, &path, language) || test_modules.contains(&path) {
            continue;
        }
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let code = strip(&text, language);
        let mut production_lines: Vec<bool> =
            code.lines().map(|line| !line.trim().is_empty()).collect();
        let mut test_lines = vec![false; production_lines.len()];
        if language == Language::Rust {
            for range in rust_test_ranges(&code) {
                for line in range {
                    if let Some(flag) = production_lines.get_mut(line) {
                        *flag = false;
                        test_lines[line] = true;
                    }
                }
            }
        }
        let comment_lines = text
            .lines()
            .zip(code.lines())
            .zip(&test_lines)
            .filter(|((written, stripped), in_test)| {
                !**in_test && !written.trim().is_empty() && stripped.trim().is_empty()
            })
            .count();
        let is_program = is_program_file(&root, &path, &code, language);
        files.push(SourceFile {
            path: relative,
            text,
            code,
            production_lines,
            is_program,
            comment_lines,
        });
    }
    files
}

/// A file that holds test code: a whole test file, or a production file with test modules inside it.
#[derive(Debug, Clone)]
pub struct TestFile {
    /// Path relative to the workspace.
    pub path: PathBuf,
    pub text: String,
    /// The text with comments and string contents replaced by spaces; line breaks are kept.
    pub code: String,
    /// For each line, whether it is test code that is not blank and not only comment.
    pub test_lines: Vec<bool>,
    /// The file belongs to the program rather than the core library.
    pub is_program: bool,
}

impl TestFile {
    pub fn test_line_count(&self) -> usize {
        self.test_lines.iter().filter(|&&line| line).count()
    }

    /// Whether line `number`, counted from 1, is test code.
    pub fn is_test_line(&self, number: usize) -> bool {
        number
            .checked_sub(1)
            .and_then(|index| self.test_lines.get(index))
            .copied()
            .unwrap_or(false)
    }
}

/// The files of `language` below `workspace` that hold test code, sorted by path: the files that
/// [`production_files`] leaves out as tests, and production files with test modules inside.
pub fn test_files(workspace: &Path, language: Language) -> Vec<TestFile> {
    let root = workspace.join(language.as_str());
    let extension = match language {
        Language::Rust => "rs",
        Language::Go => "go",
        Language::CSharp => "cs",
    };
    let mut paths = Vec::new();
    walk(&root, extension, &mut paths);
    paths.sort();
    let test_modules = match language {
        Language::Rust => rust_test_module_files(&paths),
        _ => Vec::new(),
    };
    let mut files = Vec::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let code = strip(&text, language);
        let non_blank: Vec<bool> = code.lines().map(|line| !line.trim().is_empty()).collect();
        let whole_file = is_test_file(&root, &path, language) || test_modules.contains(&path);
        let test_lines = if whole_file {
            non_blank
        } else if language == Language::Rust {
            let mut lines = vec![false; non_blank.len()];
            for range in rust_test_ranges(&code) {
                for line in range {
                    if let Some(flag) = lines.get_mut(line) {
                        *flag = non_blank[line];
                    }
                }
            }
            lines
        } else {
            continue;
        };
        if !test_lines.contains(&true) {
            continue;
        }
        let is_program = is_program_file(&root, &path, &code, language);
        files.push(TestFile {
            path: path.strip_prefix(workspace).unwrap_or(&path).to_path_buf(),
            text,
            code,
            test_lines,
            is_program,
        });
    }
    files
}

pub fn production_line_total(files: &[SourceFile]) -> usize {
    files.iter().map(SourceFile::production_line_count).sum()
}

/// Comment-only lines as a share of the comment-only and code lines of production files. None without any such line.
pub fn comment_density(files: &[SourceFile]) -> Option<f64> {
    let comments: usize = files.iter().map(|file| file.comment_lines).sum();
    let lines = comments + production_line_total(files);
    (lines > 0).then(|| comments as f64 / lines as f64)
}

fn walk(dir: &Path, extension: &str, found: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            if !name.starts_with('.') && !SKIPPED_DIRS.contains(&name.as_str()) {
                walk(&path, extension, found);
            }
        } else if kind.is_file() && path.extension().is_some_and(|e| e == extension) {
            found.push(path);
        }
    }
}

pub(crate) fn is_test_file(root: &Path, path: &Path, language: Language) -> bool {
    let relative = path.strip_prefix(root).unwrap_or(path);
    match language {
        Language::Rust => relative.components().any(|part| {
            ["tests", "benches", "examples"].contains(&part.as_os_str().to_string_lossy().as_ref())
        }),
        Language::Go => path.to_string_lossy().ends_with("_test.go"),
        Language::CSharp => project_file(root, path).is_some_and(|project| {
            fs::read_to_string(project).is_ok_and(|text| {
                ["Microsoft.NET.Test.Sdk", "xunit", "NUnit", "MSTest"]
                    .iter()
                    .any(|marker| text.contains(marker))
            })
        }),
    }
}

fn is_program_file(root: &Path, path: &Path, code: &str, language: Language) -> bool {
    match language {
        Language::Rust => {
            let relative = path.strip_prefix(root).unwrap_or(path);
            relative == Path::new("src/main.rs") || relative.starts_with("src/bin")
        }
        Language::Go => code.lines().any(|line| line.trim() == "package main"),
        Language::CSharp => project_file(root, path).is_some_and(|project| {
            fs::read_to_string(project)
                .is_ok_and(|text| text.contains("<OutputType>Exe</OutputType>"))
        }),
    }
}

/// The nearest `.csproj` in the folder of `path` or above it, up to `root`.
fn project_file(root: &Path, path: &Path) -> Option<PathBuf> {
    let mut dir = path.parent();
    while let Some(current) = dir {
        if let Ok(entries) = fs::read_dir(current) {
            let mut projects: Vec<PathBuf> = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|p| p.extension().is_some_and(|e| e == "csproj"))
                .collect();
            projects.sort();
            if let Some(project) = projects.into_iter().next() {
                return Some(project);
            }
        }
        if current == root {
            break;
        }
        dir = current.parent();
    }
    None
}

/// Files that a `#[cfg(test)] mod name;` line declares, which hold only test code.
fn rust_test_module_files(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for path in paths {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let code = strip(&text, Language::Rust);
        let lines: Vec<&str> = code.lines().map(str::trim).collect();
        for (index, line) in lines.iter().enumerate() {
            if *line != "#[cfg(test)]" {
                continue;
            }
            let Some(next) = lines[index + 1..].iter().find(|l| !l.is_empty()) else {
                continue;
            };
            let declaration = next
                .trim_start_matches("pub(crate) ")
                .trim_start_matches("pub ");
            let Some(name) = declaration
                .strip_prefix("mod ")
                .and_then(|rest| rest.strip_suffix(';'))
            else {
                continue;
            };
            let dir = module_dir(path);
            found.push(dir.join(format!("{name}.rs")));
            found.push(dir.join(name).join("mod.rs"));
        }
    }
    found
}

/// The folder that holds the child modules of the module in `path`.
fn module_dir(path: &Path) -> PathBuf {
    let parent = path.parent().unwrap_or(Path::new("")).to_path_buf();
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned());
    match stem.as_deref() {
        Some("lib" | "main" | "mod") | None => parent,
        Some(stem) => parent.join(stem),
    }
}

/// Zero-based line ranges of items marked `#[cfg(test)]` that have a body in braces.
pub(crate) fn rust_test_ranges(code: &str) -> Vec<std::ops::Range<usize>> {
    let lines: Vec<&str> = code.lines().collect();
    let mut ranges = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if lines[index].trim() != "#[cfg(test)]" {
            index += 1;
            continue;
        }
        let start = index;
        let mut depth = 0usize;
        let mut opened = false;
        let mut end = index;
        'scan: for (offset, line) in lines[index..].iter().enumerate() {
            for byte in line.bytes() {
                match byte {
                    b'{' => {
                        depth += 1;
                        opened = true;
                    }
                    b'}' => depth = depth.saturating_sub(1),
                    b';' if !opened => {
                        end = index + offset;
                        break 'scan;
                    }
                    _ => {}
                }
                if opened && depth == 0 {
                    end = index + offset;
                    break 'scan;
                }
            }
            end = index + offset;
        }
        ranges.push(start..end + 1);
        index = end + 1;
    }
    ranges
}

/// Replaces comments and the contents of string and character literals with spaces, keeping line breaks.
pub fn strip(text: &str, language: Language) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    let blank = |c: char| if c == '\n' { '\n' } else { ' ' };
    while index < chars.len() {
        let c = chars[index];
        let next = chars.get(index + 1).copied();
        if c == '/' && next == Some('/') {
            while index < chars.len() && chars[index] != '\n' {
                out.push(' ');
                index += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0;
            while index < chars.len() {
                let here = chars[index];
                let after = chars.get(index + 1).copied();
                if here == '/' && after == Some('*') && (depth == 0 || language == Language::Rust) {
                    depth += 1;
                    out.push_str("  ");
                    index += 2;
                } else if here == '*' && after == Some('/') {
                    depth -= 1;
                    out.push_str("  ");
                    index += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(here));
                    index += 1;
                }
            }
        } else if let Some(length) = string_start(&chars, index, language) {
            index = copy_string(&chars, index, length, language, &mut out);
        } else if c == '\'' {
            index = copy_char_literal(&chars, index, &mut out);
        } else {
            out.push(c);
            index += 1;
        }
    }
    out
}

/// The length of the opening of a string literal at `index`, if one starts there.
fn string_start(chars: &[char], index: usize, language: Language) -> Option<usize> {
    let at = |offset: usize| chars.get(index + offset).copied();
    let previous_is_word =
        index > 0 && (chars[index - 1].is_alphanumeric() || chars[index - 1] == '_');
    match (language, at(0)) {
        (_, Some('"')) => Some(1),
        (Language::Go, Some('`')) => Some(1),
        (Language::Rust, Some('r' | 'b')) if !previous_is_word => {
            let mut offset = 1;
            if at(0) == Some('b') && at(1) == Some('r') {
                offset = 2;
            } else if at(0) == Some('b') {
                return (at(1) == Some('"')).then_some(2);
            }
            while at(offset) == Some('#') {
                offset += 1;
            }
            (at(offset) == Some('"')).then_some(offset + 1)
        }
        (Language::CSharp, Some('@' | '$')) => {
            let mut offset = 1;
            while matches!(at(offset), Some('@' | '$')) {
                offset += 1;
            }
            (at(offset) == Some('"')).then_some(offset + 1)
        }
        _ => None,
    }
}

/// Copies the quotes of a string literal and blanks its contents. Returns the index after its end.
fn copy_string(
    chars: &[char],
    start: usize,
    opening: usize,
    language: Language,
    out: &mut String,
) -> usize {
    let opener: String = chars[start..start + opening].iter().collect();
    out.push_str(&opener);
    let mut index = start + opening;
    let hashes = opener.chars().filter(|&c| c == '#').count();
    let is_raw_rust = language == Language::Rust && opener.contains('r');
    let is_verbatim = language == Language::CSharp && opener.contains('@');
    let is_go_raw = language == Language::Go && opener == "`";
    let triple = language == Language::CSharp
        && chars.get(index) == Some(&'"')
        && chars.get(index + 1) == Some(&'"');
    if triple {
        out.push_str("\"\"");
        index += 2;
        while index < chars.len() {
            if chars[index..].starts_with(&['"', '"', '"']) {
                out.push_str("\"\"\"");
                return index + 3;
            }
            out.push(if chars[index] == '\n' { '\n' } else { ' ' });
            index += 1;
        }
        return index;
    }
    let closer = if is_go_raw { '`' } else { '"' };
    while index < chars.len() {
        let c = chars[index];
        if c == '\\' && !is_raw_rust && !is_verbatim && !is_go_raw {
            out.push_str("  ");
            index += 2;
            continue;
        }
        if c == closer {
            if is_verbatim && chars.get(index + 1) == Some(&'"') {
                out.push_str("  ");
                index += 2;
                continue;
            }
            let closing_hashes = chars[index + 1..].iter().take_while(|&&h| h == '#').count();
            if !is_raw_rust || closing_hashes >= hashes {
                out.push(c);
                for _ in 0..hashes {
                    out.push('#');
                }
                return index + 1 + if is_raw_rust { hashes } else { 0 };
            }
        }
        out.push(if c == '\n' { '\n' } else { ' ' });
        index += 1;
    }
    index
}

/// Blanks a character literal such as `'a'` or `'\n'`, and leaves a Rust lifetime such as `'a` alone.
fn copy_char_literal(chars: &[char], start: usize, out: &mut String) -> usize {
    let at = |offset: usize| chars.get(start + offset).copied();
    let end = if at(1) == Some('\\') {
        chars
            .get(start + 3..)
            .and_then(|rest| rest.iter().position(|&c| c == '\'' || c == '\n'))
            .map(|position| start + 3 + position)
    } else if at(2) == Some('\'') {
        Some(start + 2)
    } else {
        None
    };
    match end {
        Some(end) if chars[end] == '\'' => {
            out.push('\'');
            for _ in start + 1..end {
                out.push(' ');
            }
            out.push('\'');
            end + 1
        }
        _ => {
            out.push('\'');
            start + 1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_and_strings_are_blanked_but_lines_are_kept() {
        let rust = "let a = \"// not a comment\"; // a comment\n/* one\n two */ let b = 'x';\n";

        let code = strip(rust, Language::Rust);

        assert_eq!(code.lines().count(), 3);
        assert!(code.starts_with("let a = \""));
        assert!(!code.contains("comment"));
        assert!(code.lines().nth(1).is_some_and(|l| l.trim().is_empty()));
        assert!(code.contains("let b = ' ';"));
    }

    #[test]
    fn rust_lifetimes_survive_and_raw_strings_are_blanked() {
        let code = strip(
            "fn f<'a>(x: &'a str) -> &'a str { r#\"un\"wrap()\"# }",
            Language::Rust,
        );

        assert!(code.contains("<'a>"));
        assert!(!code.contains("wrap"));
    }

    #[test]
    fn test_modules_are_not_production_lines() {
        let dir = tempfile::tempdir().expect("temp dir");
        let src = dir.path().join("rust/src");
        fs::create_dir_all(&src).expect("create src");
        fs::write(
            src.join("lib.rs"),
            "//! Doc.\npub fn one() -> u32 {\n    1\n}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn t() {}\n}\n",
        )
        .expect("write lib");
        fs::create_dir_all(dir.path().join("rust/tests")).expect("create tests");
        fs::write(dir.path().join("rust/tests/cli.rs"), "fn main() {}\n").expect("write test");

        let files = production_files(dir.path(), Language::Rust);

        assert_eq!(files.len(), 1);
        assert_eq!(production_line_total(&files), 3);
    }

    #[test]
    fn go_test_files_and_csharp_test_projects_are_left_out() {
        let dir = tempfile::tempdir().expect("temp dir");
        let go = dir.path().join("go");
        fs::create_dir_all(&go).expect("create go");
        fs::write(go.join("a.go"), "package a\n\n// F is f.\nfunc F() {}\n").expect("write");
        fs::write(go.join("a_test.go"), "package a\n").expect("write");
        let tests = dir.path().join("csharp/Engine.Tests");
        fs::create_dir_all(&tests).expect("create tests");
        fs::write(
            tests.join("T.csproj"),
            "<PackageReference Include=\"xunit\" />",
        )
        .expect("write");
        fs::write(tests.join("T.cs"), "class T {}\n").expect("write");

        assert_eq!(
            production_line_total(&production_files(dir.path(), Language::Go)),
            2
        );
        assert!(production_files(dir.path(), Language::CSharp).is_empty());
    }
}
