//! Reads the tests of one language from their code: where each test is, whether it asserts anything and what kind it is.
//! Helpers count: a test asserts when it or a helper it calls asserts, and it reaches what its helpers reach.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::sources::{SourceFile, TestFile};
use benchy::contract::Language;

/// The three kinds of test that category D looks for. A test is counted as the first kind that fits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// Checks properties over random or generated input, or uses a property-testing library.
    Invariant,
    /// Runs the program, as a process or through its own entry code.
    EndToEnd,
    /// Calls the core library directly.
    Unit,
}

/// Where a function's body is, in characters of its file's code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    /// `{ ... }`: the positions of the two braces.
    Block { open: usize, close: usize },
    /// C# `=> expression;`: the positions of the arrow and of the semicolon.
    Expression { arrow: usize, end: usize },
}

impl Body {
    fn range(self) -> std::ops::Range<usize> {
        match self {
            Body::Block { open, close } => open..close + 1,
            Body::Expression { arrow, end } => arrow..end + 1,
        }
    }
}

/// A function or method defined in test code.
#[derive(Debug, Clone)]
pub struct Function {
    /// Index into [`Suite::files`].
    pub file: usize,
    pub name: String,
    pub is_test: bool,
    /// Marked to pass only by panicking, such as Rust's `#[should_panic]`.
    pub expects_panic: bool,
    pub body: Body,
    /// For Go tests: the name of the `*testing.T` parameter.
    pub parameter: Option<String>,
}

/// One test, as category D scores it.
#[derive(Debug, Clone)]
pub struct Test {
    pub name: String,
    pub asserts: bool,
    pub kind: Option<Kind>,
}

/// The test code of one language.
#[derive(Debug, Clone)]
pub struct Suite {
    pub files: Vec<TestFile>,
    pub functions: Vec<Function>,
}

impl Suite {
    pub fn read(workspace: &Path, language: Language) -> Suite {
        let files = super::sources::test_files(workspace, language);
        let mut functions = Vec::new();
        for (index, file) in files.iter().enumerate() {
            functions.extend(match language {
                Language::Rust => rust_functions(index, file),
                Language::Go => go_functions(index, file),
                Language::CSharp => csharp_functions(index, file),
            });
        }
        Suite { files, functions }
    }

    pub fn line_count(&self) -> usize {
        self.files.iter().map(TestFile::test_line_count).sum()
    }

    pub fn tests(&self) -> impl Iterator<Item = &Function> {
        self.functions.iter().filter(|f| f.is_test)
    }

    fn body_code(&self, function: &Function) -> String {
        let code: Vec<char> = self.files[function.file].code.chars().collect();
        code[function.body.range()].iter().collect()
    }

    fn body_text(&self, function: &Function) -> String {
        let text: Vec<char> = self.files[function.file].text.chars().collect();
        text.get(function.body.range())
            .map(|chars| chars.iter().collect())
            .unwrap_or_default()
    }

    /// The functions a test reaches through the helpers it calls, itself included.
    fn reach(&self, start: usize) -> Vec<usize> {
        let mut by_name: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (index, function) in self.functions.iter().enumerate() {
            by_name.entry(&function.name).or_default().push(index);
        }
        let mut seen = BTreeSet::from([start]);
        let mut queue = vec![start];
        while let Some(index) = queue.pop() {
            for name in called_names(&self.body_code(&self.functions[index])) {
                for &next in by_name.get(name.as_str()).into_iter().flatten() {
                    if seen.insert(next) {
                        queue.push(next);
                    }
                }
            }
        }
        seen.into_iter().collect()
    }

    /// Every test with whether it asserts and its kind. `context` names what the tests can reach.
    pub fn classify(&self, language: Language, context: &Reachable) -> Vec<Test> {
        let mut tests = Vec::new();
        for (index, function) in self.functions.iter().enumerate() {
            if !function.is_test {
                continue;
            }
            let reached = self.reach(index);
            let code: String = reached
                .iter()
                .map(|&i| self.body_code(&self.functions[i]))
                .collect::<Vec<_>>()
                .join("\n");
            let text: String = reached
                .iter()
                .map(|&i| self.body_text(&self.functions[i]))
                .collect::<Vec<_>>()
                .join("\n");
            let asserts = function.expects_panic || has_assertion(&code, language);
            let file = &self.files[function.file];
            tests.push(Test {
                name: function.name.clone(),
                asserts,
                kind: kind(language, file, &code, &text, context),
            });
        }
        tests
    }
}

/// What production code the tests can reach, read from the production files.
#[derive(Debug, Clone, Default)]
pub struct Reachable {
    /// Rust: the name the core library crate is used by in tests.
    pub crate_name: Option<String>,
    /// C#: types declared in the core library.
    pub core_types: BTreeSet<String>,
    /// C#: types declared in the program.
    pub program_types: BTreeSet<String>,
}

impl Reachable {
    pub fn read(workspace: &Path, language: Language, production: &[SourceFile]) -> Reachable {
        let mut reachable = Reachable::default();
        match language {
            Language::Rust => reachable.crate_name = rust_crate_name(&workspace.join("rust")),
            Language::CSharp => {
                for file in production {
                    let names = csharp_type_names(&file.code);
                    if file.is_program {
                        reachable.program_types.extend(names);
                    } else {
                        reachable.core_types.extend(names);
                    }
                }
            }
            Language::Go => {}
        }
        reachable
    }
}

fn rust_crate_name(dir: &Path) -> Option<String> {
    let manifest: toml::Table = fs::read_to_string(dir.join("Cargo.toml"))
        .ok()?
        .parse()
        .ok()?;
    let name = manifest
        .get("lib")
        .and_then(|lib| lib.get("name"))
        .or_else(|| manifest.get("package")?.get("name"))?
        .as_str()?;
    Some(name.replace('-', "_"))
}

fn csharp_type_names(code: &str) -> Vec<String> {
    let words: Vec<&str> = words(code).collect();
    words
        .windows(2)
        .filter(|pair| ["class", "record", "struct", "interface", "enum"].contains(&pair[0]))
        .map(|pair| pair[1].to_string())
        .filter(|name| name.chars().next().is_some_and(char::is_uppercase))
        .collect()
}

fn words(code: &str) -> impl Iterator<Item = &str> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
}

fn has_word(code: &str, wanted: &[&str]) -> bool {
    words(code).any(|word| wanted.contains(&word))
}

/// Names followed by `(`: the functions and methods a body calls.
fn called_names(code: &str) -> BTreeSet<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut names = BTreeSet::new();
    for (index, &c) in chars.iter().enumerate() {
        if c != '(' {
            continue;
        }
        let end = index;
        let start = chars[..end]
            .iter()
            .rposition(|c| !(c.is_alphanumeric() || *c == '_'))
            .map_or(0, |p| p + 1);
        if start < end {
            names.insert(chars[start..end].iter().collect());
        }
    }
    names
}

fn has_assertion(code: &str, language: Language) -> bool {
    match language {
        Language::Rust => [
            "assert!",
            "assert_eq!",
            "assert_ne!",
            "assert_matches!",
            "debug_assert",
            "panic!",
            "unreachable!",
            "prop_assert",
        ]
        .iter()
        .any(|marker| code.contains(marker)),
        Language::Go => [
            ".Error(",
            ".Errorf(",
            ".Fatal(",
            ".Fatalf(",
            ".Fail(",
            ".FailNow(",
            "assert.",
            "require.",
            "panic(",
        ]
        .iter()
        .any(|marker| code.contains(marker)),
        Language::CSharp => ["Assert.", ".Should", "throw "]
            .iter()
            .any(|marker| code.contains(marker)),
    }
}

const PROPERTY_WORDS: [&str; 10] = [
    "proptest",
    "prop_assert",
    "quickcheck",
    "rapid",
    "quick",
    "FsCheck",
    "CsCheck",
    "Hedgehog",
    "Arbitrary",
    "arbitrary",
];
const RANDOM_WORDS: [&str; 10] = [
    "Rng", "rng", "rand", "Rand", "Random", "random", "seed", "Seed", "seeds", "Seeds",
];
const LOOP_WORDS: [&str; 4] = ["for", "while", "loop", "foreach"];

fn kind(
    language: Language,
    file: &TestFile,
    code: &str,
    text: &str,
    context: &Reachable,
) -> Option<Kind> {
    let property = has_word(code, &PROPERTY_WORDS);
    let random = has_word(code, &RANDOM_WORDS) && has_word(code, &LOOP_WORDS);
    if property || random {
        return Some(Kind::Invariant);
    }
    let file_code = &file.code;
    let (program, core) = match language {
        Language::Rust => {
            let starts_program = ["CARGO_BIN_EXE", "Command::new", "assert_cmd"]
                .iter()
                .any(|marker| text.contains(marker));
            let uses_crate = context
                .crate_name
                .as_ref()
                .is_some_and(|name| file_code.contains(&format!("{name}::")));
            let in_library = !file.path.starts_with("rust/tests") && !file.is_program;
            (starts_program || file.is_program, in_library || uses_crate)
        }
        Language::Go => {
            let starts_program = code.contains("exec.Command");
            (starts_program || file.is_program, !file.is_program)
        }
        Language::CSharp => {
            let names = |types: &BTreeSet<String>| words(code).any(|word| types.contains(word));
            let starts_program =
                code.contains("Process.Start") || code.contains("ProcessStartInfo");
            (
                starts_program || names(&context.program_types),
                names(&context.core_types),
            )
        }
    };
    if program {
        Some(Kind::EndToEnd)
    } else if core {
        Some(Kind::Unit)
    } else {
        None
    }
}

/// The position of the brace that closes the one at `open`.
pub fn closing_brace(code: &[char], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, &c) in code.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// The first `{` at or after `from` that is outside parentheses, unless a `;` ends the declaration first.
fn body_brace(code: &[char], from: usize) -> Option<usize> {
    let mut parens = 0i32;
    for (index, &c) in code.iter().enumerate().skip(from) {
        match c {
            '(' | '[' => parens += 1,
            ')' | ']' => parens -= 1,
            '{' if parens <= 0 => return Some(index),
            ';' if parens <= 0 => return None,
            _ => {}
        }
    }
    None
}

/// The character offset at which each line starts.
fn line_starts(code: &[char]) -> Vec<usize> {
    let mut starts = vec![0];
    starts.extend(
        code.iter()
            .enumerate()
            .filter(|(_, c)| **c == '\n')
            .map(|(i, _)| i + 1),
    );
    starts
}

fn line_text(code: &[char], starts: &[usize], line: usize) -> String {
    let start = starts[line];
    let end = starts.get(line + 1).map_or(code.len(), |next| next - 1);
    code[start..end].iter().collect()
}

/// The attribute lines right above `line`, joined.
fn attributes_above(code: &[char], starts: &[usize], line: usize, opener: char) -> String {
    let mut found = String::new();
    let mut current = line;
    while current > 0 {
        current -= 1;
        let text = line_text(code, starts, current);
        let trimmed = text.trim();
        if trimmed.starts_with(opener) {
            found.push_str(trimmed);
            found.push('\n');
        } else {
            break;
        }
    }
    found
}

fn identifier_at(code: &[char], start: usize) -> String {
    code[start..]
        .iter()
        .take_while(|c| c.is_alphanumeric() || **c == '_')
        .collect()
}

fn rust_functions(file_index: usize, file: &TestFile) -> Vec<Function> {
    let code: Vec<char> = file.code.chars().collect();
    let starts = line_starts(&code);
    let property_blocks: Vec<(usize, usize)> = find_all(&code, "proptest!")
        .into_iter()
        .filter_map(|at| {
            let open = code[at..].iter().position(|&c| c == '{')? + at;
            Some((open, closing_brace(&code, open)?))
        })
        .collect();
    let mut functions = Vec::new();
    for at in find_all(&code, "fn ") {
        let line = starts.partition_point(|&s| s <= at) - 1;
        let before = at.checked_sub(1).map(|i| code[i]);
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') || !file.is_test_line(line + 1) {
            continue;
        }
        let name = identifier_at(&code, at + 3);
        let Some(open) = body_brace(&code, at + 3 + name.len()) else {
            continue;
        };
        let Some(close) = closing_brace(&code, open) else {
            continue;
        };
        let attributes = attributes_above(&code, &starts, line, '#');
        let in_property_block = property_blocks.iter().any(|&(o, c)| o < at && at < c);
        let is_test = in_property_block
            || attributes
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|word| word == "test" || word == "rstest" || word == "test_case");
        functions.push(Function {
            file: file_index,
            name,
            is_test,
            expects_panic: attributes.contains("should_panic"),
            body: Body::Block { open, close },
            parameter: None,
        });
    }
    functions
}

fn find_all(code: &[char], needle: &str) -> Vec<usize> {
    let needle: Vec<char> = needle.chars().collect();
    (0..code.len().saturating_sub(needle.len() - 1))
        .filter(|&i| code[i..i + needle.len()] == needle[..])
        .collect()
}

fn go_functions(file_index: usize, file: &TestFile) -> Vec<Function> {
    let code: Vec<char> = file.code.chars().collect();
    let starts = line_starts(&code);
    let mut functions = Vec::new();
    for (line, &start) in starts.iter().enumerate() {
        if !code[start..].starts_with(&['f', 'u', 'n', 'c', ' ']) {
            continue;
        }
        let mut at = start + 5;
        if code.get(at) == Some(&'(') {
            let Some(close) = code[at..].iter().position(|&c| c == ')') else {
                continue;
            };
            at += close + 1;
            at += code[at..].iter().take_while(|c| **c == ' ').count();
        }
        let name = identifier_at(&code, at);
        let Some(open) = body_brace(&code, at + name.len()) else {
            continue;
        };
        let Some(close) = closing_brace(&code, open) else {
            continue;
        };
        let signature: String = code[at..open].iter().collect();
        let is_test = file.path.to_string_lossy().ends_with("_test.go")
            && go_test_name(&name)
            && (signature.contains("*testing.T") || signature.contains("*testing.F"));
        let parameter = signature
            .split_once('(')
            .and_then(|(_, rest)| rest.split_whitespace().next())
            .map(str::to_string);
        functions.push(Function {
            file: file_index,
            name,
            is_test: is_test && file.is_test_line(line + 1),
            expects_panic: false,
            body: Body::Block { open, close },
            parameter,
        });
    }
    functions
}

fn go_test_name(name: &str) -> bool {
    ["Test", "Fuzz"].iter().any(|prefix| {
        name.strip_prefix(prefix)
            .is_some_and(|rest| rest.is_empty() || !rest.starts_with(char::is_lowercase))
    })
}

const CSHARP_TEST_ATTRIBUTES: [&str; 7] = [
    "Fact",
    "Theory",
    "Test",
    "TestMethod",
    "TestCase",
    "DataTestMethod",
    "Property",
];
const CSHARP_STATEMENTS: [&str; 16] = [
    "if", "else", "for", "foreach", "while", "switch", "return", "using", "var", "new", "await",
    "catch", "lock", "throw", "case", "yield",
];

fn csharp_functions(file_index: usize, file: &TestFile) -> Vec<Function> {
    let code: Vec<char> = file.code.chars().collect();
    let starts = line_starts(&code);
    let mut functions = Vec::new();
    for line in 0..starts.len() {
        if !file.is_test_line(line + 1) {
            continue;
        }
        let text = line_text(&code, &starts, line);
        let Some((name, after_params)) = csharp_declaration(&text) else {
            continue;
        };
        let after = starts[line] + after_params;
        let rest: String = code[after..].iter().take_while(|c| **c != '\n').collect();
        let body = if let Some(arrow) = rest.find("=>") {
            let arrow = after + rest[..arrow].chars().count();
            let Some(end) = expression_end(&code, arrow) else {
                continue;
            };
            Body::Expression { arrow, end }
        } else {
            let Some(open) = body_brace(&code, after) else {
                continue;
            };
            let between: String = code[after..open].iter().collect();
            if between.lines().count() > 2 || between.contains('=') {
                continue;
            }
            let Some(close) = closing_brace(&code, open) else {
                continue;
            };
            Body::Block { open, close }
        };
        let attributes = attributes_above(&code, &starts, line, '[');
        let is_test = words(&attributes).any(|word| CSHARP_TEST_ATTRIBUTES.contains(&word));
        functions.push(Function {
            file: file_index,
            name,
            is_test,
            expects_panic: false,
            body,
            parameter: None,
        });
    }
    functions
}

/// The name of the method declared on this line, and the position just after its parameter list.
fn csharp_declaration(line: &str) -> Option<(String, usize)> {
    let chars: Vec<char> = line.chars().collect();
    let open = chars.iter().position(|&c| c == '(')?;
    let mut end = open;
    if end > 0 && chars[end - 1] == '>' {
        end = chars[..end].iter().rposition(|&c| c == '<')?;
    }
    let start = chars[..end]
        .iter()
        .rposition(|c| !(c.is_alphanumeric() || *c == '_'))
        .map_or(0, |p| p + 1);
    let name: String = chars[start..end].iter().collect();
    let before: String = chars[..start].iter().collect();
    let first = before.split_whitespace().next()?;
    let is_declaration = !name.is_empty()
        && before.ends_with(' ')
        && !before.contains(['=', '(', '.', ',', '"'])
        && !CSHARP_STATEMENTS.contains(&first)
        && !first.starts_with(['[', '{', '}']);
    if !is_declaration {
        return None;
    }
    let mut depth = 0usize;
    for (index, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some((name, index + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// The `;` that ends the expression body starting at `arrow`.
fn expression_end(code: &[char], arrow: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (index, &c) in code.iter().enumerate().skip(arrow) {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ';' if depth <= 0 => return Some(index),
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn test_file(path: &str, text: &str, language: Language, is_program: bool) -> TestFile {
        let code = super::super::sources::strip(text, language);
        let test_lines = code.lines().map(|l| !l.trim().is_empty()).collect();
        TestFile {
            path: PathBuf::from(path),
            text: text.to_string(),
            code,
            test_lines,
            is_program,
        }
    }

    fn classify(files: Vec<TestFile>, language: Language, context: &Reachable) -> Vec<Test> {
        let mut functions = Vec::new();
        for (index, file) in files.iter().enumerate() {
            functions.extend(match language {
                Language::Rust => rust_functions(index, file),
                Language::Go => go_functions(index, file),
                Language::CSharp => csharp_functions(index, file),
            });
        }
        Suite { files, functions }.classify(language, context)
    }

    #[test]
    fn rust_tests_assert_through_helpers_and_get_their_kind() {
        let cli = "fn run(input: &str) -> String {\n    let out = Command::new(env!(\"CARGO_BIN_EXE_engine\"));\n    assert_eq!(out, 0);\n    out\n}\n\n#[test]\nfn example() {\n    run(\"x\");\n}\n\n#[test]\nfn nothing() {\n    run_quietly();\n}\n";
        let props = "use engine::Engine;\n\n#[test]\n#[should_panic]\nfn random_sessions() {\n    for seed in 0..9 {\n        Engine::new();\n    }\n}\n\n#[test]\nfn direct() {\n    assert!(engine::parse(1).is_ok());\n}\n";
        let context = Reachable {
            crate_name: Some("engine".to_string()),
            ..Reachable::default()
        };

        let tests = classify(
            vec![
                test_file("rust/tests/cli.rs", cli, Language::Rust, false),
                test_file("rust/tests/props.rs", props, Language::Rust, false),
            ],
            Language::Rust,
            &context,
        );

        let summary: Vec<(String, bool, Option<Kind>)> = tests
            .into_iter()
            .map(|t| (t.name, t.asserts, t.kind))
            .collect();
        assert_eq!(
            summary,
            [
                ("example".to_string(), true, Some(Kind::EndToEnd)),
                ("nothing".to_string(), false, None),
                ("random_sessions".to_string(), true, Some(Kind::Invariant)),
                ("direct".to_string(), true, Some(Kind::Unit)),
            ]
        );
    }

    #[test]
    fn go_tests_are_found_by_name_and_signature() {
        let core = "package matching\n\nfunc check(t *testing.T, s string) {\n\tif s != \"\" {\n\t\tt.Fatalf(\"bad\")\n\t}\n}\n\nfunc TestBook(t *testing.T) {\n\tcheck(t, \"x\")\n}\n\nfunc TestEmpty(t *testing.T) {\n}\n\nfunc Testable(t *testing.T) {\n}\n";
        let program = "package main\n\nfunc TestRun(tb *testing.T) {\n\ttb.Error(\"x\")\n}\n";

        let tests = classify(
            vec![
                test_file("go/matching/book_test.go", core, Language::Go, false),
                test_file("go/cmd/engine/main_test.go", program, Language::Go, true),
            ],
            Language::Go,
            &Reachable::default(),
        );

        let summary: Vec<(String, bool, Option<Kind>)> = tests
            .into_iter()
            .map(|t| (t.name, t.asserts, t.kind))
            .collect();
        assert_eq!(
            summary,
            [
                ("TestBook".to_string(), true, Some(Kind::Unit)),
                ("TestEmpty".to_string(), false, Some(Kind::Unit)),
                ("TestRun".to_string(), true, Some(Kind::EndToEnd)),
            ]
        );
    }

    #[test]
    fn csharp_tests_with_block_and_expression_bodies_are_found() {
        let text = "namespace Engine.Tests;\n\npublic class BookTests\n{\n    private static void Check(string s)\n    {\n        Assert.Equal(s, Session.Run(s));\n    }\n\n    [Fact]\n    public void Trades() => Check(\"x\");\n\n    [Theory]\n    [InlineData(1)]\n    public void Direct(int n)\n    {\n        var book = new Book(n);\n        if (n > 0)\n        {\n            book.Add(n);\n        }\n    }\n}\n";
        let context = Reachable {
            core_types: BTreeSet::from(["Book".to_string()]),
            program_types: BTreeSet::from(["Session".to_string()]),
            ..Reachable::default()
        };

        let tests = classify(
            vec![test_file(
                "csharp/Engine.Tests/BookTests.cs",
                text,
                Language::CSharp,
                false,
            )],
            Language::CSharp,
            &context,
        );

        let summary: Vec<(String, bool, Option<Kind>)> = tests
            .into_iter()
            .map(|t| (t.name, t.asserts, t.kind))
            .collect();
        assert_eq!(
            summary,
            [
                ("Trades".to_string(), true, Some(Kind::EndToEnd)),
                ("Direct".to_string(), false, Some(Kind::Unit)),
            ]
        );
    }
}

#[cfg(test)]
mod reference_tests {
    use super::*;

    #[test]
    fn every_reference_test_asserts_and_the_kinds_are_found() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("validation/reference");
        for (language, expected) in [
            (
                Language::Rust,
                vec![Kind::Invariant, Kind::EndToEnd, Kind::Unit],
            ),
            (
                Language::Go,
                vec![Kind::Invariant, Kind::EndToEnd, Kind::Unit],
            ),
            (
                Language::CSharp,
                vec![Kind::Invariant, Kind::EndToEnd, Kind::Unit],
            ),
        ] {
            let production = super::super::sources::production_files(&root, language);
            let suite = Suite::read(&root, language);
            let tests = suite.classify(language, &Reachable::read(&root, language, &production));
            let kinds: BTreeSet<Kind> = tests.iter().filter_map(|t| t.kind).collect();

            assert!(
                tests.len() >= 10,
                "{} has {} tests",
                language.as_str(),
                tests.len()
            );
            assert!(tests.iter().all(|t| t.asserts), "{}", language.as_str());
            assert_eq!(
                kinds.into_iter().collect::<Vec<_>>(),
                expected,
                "{}",
                language.as_str()
            );
        }
    }
}
