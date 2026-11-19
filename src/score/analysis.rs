//! Runs the published lint configuration and the formatter of one language once, and counts what they report.
//! Economy reads the unused items from these findings and static maintainability reads the rest.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::process::{self, Log};
use super::sources::SourceFile;
use benchy::contract::Language;

/// What the analysers of one language reported.
#[derive(Debug, Clone, Default)]
pub struct Findings {
    /// Distinct findings of the lint configuration, without the unused items that economy counts.
    pub lint: usize,
    /// Distinct places the compiler or an analyser reports as unused.
    pub unused: usize,
    /// Each of those places, as the file path the tool printed and a line number counted from 1.
    pub unused_at: Vec<(String, usize)>,
    /// Files the formatter would change; absent when it could not run.
    pub unformatted: Option<usize>,
}

const CSHARP_UNUSED: [&str; 10] = [
    "CS0168", "CS0169", "CS0219", "CS0414", "CS0649", "CS8321", "IDE0051", "IDE0052", "IDE0060",
    "CA1823",
];

const CSHARP_UNUSED_CONFIG: &str = "is_global = true
dotnet_diagnostic.IDE0051.severity = warning
dotnet_diagnostic.IDE0052.severity = warning
dotnet_diagnostic.IDE0060.severity = warning
";

pub fn analyse(root: &Path, language: Language, log: &mut Log) -> Findings {
    match language {
        Language::Rust => rust(root, log),
        Language::Go => go(root, log),
        Language::CSharp => csharp(root, log),
    }
}

/// The lines of a lint file, without comments.
fn lint_lines(root: &Path, file: &str) -> Vec<String> {
    fs::read_to_string(root.join("lint").join(file))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect()
}

fn rust(root: &Path, log: &mut Log) -> Findings {
    // -Dwarnings is left out so that a finding in the library does not stop the lints of its tests.
    let lints: Vec<String> = lint_lines(root, "rust/clippy.txt")
        .into_iter()
        .filter(|lint| lint != "-Dwarnings")
        .collect();
    let line = format!(
        "cd rust && CARGO_TARGET_DIR=target RUSTFLAGS= cargo clippy --release --locked --all-targets --message-format=json -- {}",
        lints.join(" ")
    );
    let output = process::shell(root, &line, log);
    let mut lint = BTreeSet::new();
    let mut unused = BTreeSet::new();
    for message in output
        .stdout
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
    {
        if message["reason"] != "compiler-message" {
            continue;
        }
        let message = &message["message"];
        let level = message["level"].as_str().unwrap_or_default();
        let Some(span) = message["spans"].as_array().and_then(|spans| spans.first()) else {
            continue;
        };
        if level != "warning" && level != "error" {
            continue;
        }
        let code = message["code"]["code"]
            .as_str()
            .unwrap_or("compile-error")
            .to_string();
        let file = span["file_name"].as_str().unwrap_or_default().to_string();
        let at = span["line_start"].as_u64().unwrap_or_default();
        if code == "dead_code" || code.starts_with("unused_") {
            unused.insert((file, usize::try_from(at).unwrap_or_default()));
            continue;
        }
        lint.insert((
            file,
            at,
            code,
            message["message"].as_str().unwrap_or_default().to_string(),
        ));
    }
    let format = process::shell(
        root,
        "cd rust && cargo fmt --all --check -- --config-path ../lint/rust/rustfmt.toml",
        log,
    );
    let changed: BTreeSet<String> = format
        .stdout
        .lines()
        .filter_map(|l| l.strip_prefix("Diff in "))
        .map(|rest| rest.rsplitn(3, ':').last().unwrap_or(rest).to_string())
        .collect();
    let unformatted = if format.success {
        0
    } else {
        changed.len().max(1)
    };
    Findings {
        lint: lint.len(),
        unused: unused.len(),
        unused_at: unused.into_iter().collect(),
        unformatted: Some(unformatted),
    }
}

/// Lines in the `path:line:column: message` form that Go tools print.
fn go_findings(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| {
        let mut parts = line.splitn(3, ':');
        let file = parts.next().unwrap_or_default();
        let number = parts.next().unwrap_or_default();
        file.ends_with(".go") && !number.is_empty() && number.bytes().all(|b| b.is_ascii_digit())
    })
}

fn go(root: &Path, log: &mut Log) -> Findings {
    let checks = lint_lines(root, "go/staticcheck.txt").join(",");
    let environment = "CGO_ENABLED=0 GOFLAGS=-mod=readonly GOWORK=off";
    let vet = process::shell(root, &format!("cd go && {environment} go vet ./..."), log);
    let staticcheck = process::shell(
        root,
        &format!("cd go && {environment} staticcheck -checks \"{checks}\" ./..."),
        log,
    );
    let mut lint: BTreeSet<String> = go_findings(&vet.text()).map(str::to_string).collect();
    let mut unused = BTreeSet::new();
    for finding in go_findings(&staticcheck.text()) {
        if finding.ends_with("(U1000)") {
            let mut parts = finding.splitn(3, ':');
            let file = parts.next().unwrap_or_default().to_string();
            let line = parts
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or_default();
            unused.insert((file, line));
        } else {
            lint.insert(finding.to_string());
        }
    }
    let format = process::shell(root, "cd go && gofmt -l .", log);
    let unformatted = format.success.then(|| {
        format
            .stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count()
    });
    Findings {
        lint: lint.len(),
        unused: unused.len(),
        unused_at: unused.into_iter().collect(),
        unformatted,
    }
}

/// One `file(line,column): warning CODE: message` line of an MSBuild log, as (file, line, column, code).
fn msbuild_finding(line: &str) -> Option<(String, u32, u32, String)> {
    let (location, rest) = line.split_once("): ")?;
    let (file, position) = location.rsplit_once('(')?;
    let (row, column) = position.split_once(',')?;
    let rest = rest
        .strip_prefix("warning ")
        .or_else(|| rest.strip_prefix("error "))?;
    let code = rest.split(':').next()?.trim().to_string();
    Some((
        file.trim().to_string(),
        row.parse().ok()?,
        column.parse().ok()?,
        code,
    ))
}

fn csharp(root: &Path, log: &mut Log) -> Findings {
    let config = root.join(".globalconfig");
    drop(fs::write(&config, CSHARP_UNUSED_CONFIG));
    let build = process::shell(
        root,
        "cd csharp && dotnet build -c Release --no-incremental @../lint/csharp/analyzers.rsp \
         -p:TreatWarningsAsErrors=false -p:EnforceCodeStyleInBuild=true",
        log,
    );
    drop(fs::remove_file(&config));
    let mut lint = BTreeSet::new();
    let mut unused = BTreeSet::new();
    for (file, row, column, code) in build.text().lines().filter_map(msbuild_finding) {
        if CSHARP_UNUSED.contains(&code.as_str()) {
            unused.insert((file, usize::try_from(row).unwrap_or_default()));
        } else {
            lint.insert((file, row, column, code));
        }
    }
    let format = process::shell(
        root,
        "cd csharp && dotnet format whitespace --verify-no-changes",
        log,
    );
    let changed: BTreeSet<String> = format
        .text()
        .lines()
        .filter_map(msbuild_finding)
        .map(|(file, ..)| file)
        .collect();
    let unformatted = if format.success {
        0
    } else {
        changed.len().max(1)
    };
    Findings {
        lint: lint.len(),
        unused: unused.len(),
        unused_at: unused.into_iter().collect(),
        unformatted: Some(unformatted),
    }
}

/// Hazards in production code: bare unwraps, stubs, discarded errors, empty catch blocks, lint suppressions and unsafe code.
pub fn hazards(files: &[SourceFile], language: Language) -> usize {
    let mut count = 0;
    for file in files {
        let code_lines: Vec<&str> = file.code.lines().collect();
        let text_lines: Vec<&str> = file.text.lines().collect();
        for (index, code) in code_lines.iter().enumerate() {
            let text = text_lines.get(index).copied().unwrap_or_default();
            if file.is_production_line(index + 1) || is_suppression(text, language) {
                count += line_hazards(code, text, language);
            }
        }
        if language == Language::CSharp {
            count += empty_catches(&file.code);
        }
    }
    count
}

fn is_suppression(text: &str, language: Language) -> bool {
    language == Language::Go && (text.contains("//nolint") || text.contains("//lint:"))
}

fn words(code: &str) -> impl Iterator<Item = &str> {
    code.split(|c: char| !(c.is_alphanumeric() || c == '_'))
}

fn line_hazards(code: &str, text: &str, language: Language) -> usize {
    let occurrences = |needle: &str| code.matches(needle).count();
    let unsafe_words = words(code).filter(|&word| word == "unsafe").count();
    match language {
        Language::Rust => {
            let trimmed = code.trim_start();
            occurrences(".unwrap()")
                + occurrences("todo!(")
                + occurrences("unimplemented!(")
                + unsafe_words
                + occurrences("#[allow(")
                + occurrences("#![allow(")
                + occurrences("#[expect(")
                + usize::from(trimmed.starts_with("let _ =") || trimmed.starts_with("let _: "))
        }
        Language::Go => {
            let trimmed = code.trim_start();
            let stub = [
                "panic(\"TODO",
                "panic(\"todo",
                "panic(\"not implemented",
                "panic(\"unimplemented",
            ]
            .iter()
            .any(|stub| text.contains(stub));
            let discarded =
                trimmed.starts_with("_ = ") || code.contains(", _ :=") || code.contains(", _ = ");
            usize::from(stub)
                + usize::from(discarded)
                + usize::from(text.contains("//nolint") || text.contains("//lint:"))
                + code.matches("unsafe.").count()
        }
        Language::CSharp => {
            occurrences("NotImplementedException")
                + occurrences("#pragma warning disable")
                + occurrences("SuppressMessage(")
                + unsafe_words
        }
    }
}

/// `catch { }` or `catch (Exception e) { }` with nothing inside the braces.
fn empty_catches(code: &str) -> usize {
    let mut count = 0;
    let mut rest = code;
    while let Some(position) = rest.find("catch") {
        let after = &rest[position + "catch".len()..];
        let after = after.trim_start();
        let after = if after.starts_with('(') {
            after
                .find(')')
                .map_or("", |end| &after[end + 1..])
                .trim_start()
        } else {
            after
        };
        let after = after.strip_prefix("when").map_or(after, |when| {
            when.trim_start()
                .find(')')
                .map_or("", |end| &when.trim_start()[end + 1..])
                .trim_start()
        });
        if after
            .strip_prefix('{')
            .is_some_and(|body| body.trim_start().starts_with('}'))
        {
            count += 1;
        }
        rest = &rest[position + "catch".len()..];
    }
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn msbuild_warnings_are_read_with_their_place_and_code() {
        let line = "/w/csharp/Matching/Book.cs(6,17): warning IDE0051: Private member 'Book.Unused' is unused [/w/x.csproj]";

        let finding = msbuild_finding(line);

        assert_eq!(
            finding,
            Some((
                "/w/csharp/Matching/Book.cs".to_string(),
                6,
                17,
                "IDE0051".to_string()
            ))
        );
    }

    #[test]
    fn hazards_are_counted_in_code_and_not_in_comments_or_strings() {
        let rust = "let a = b.unwrap(); // c.unwrap()\nlet s = \"x.unwrap()\";\nlet _ = f();\n";
        let csharp = "try { F(); } catch (IOException) { }\ntry { F(); } catch { Log(); }\n";

        let code = crate::score::sources::strip(rust, Language::Rust);
        let counts: Vec<usize> = code
            .lines()
            .map(|line| line_hazards(line, "", Language::Rust))
            .collect();

        assert_eq!(counts, [1, 0, 1]);
        assert_eq!(empty_catches(csharp), 1);
    }
}
