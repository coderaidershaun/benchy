//! Category E: production lines against the reference of the language, runtime dependencies, duplication and unused code.
//! Duplication comes from lizard's clone detector, the one detector that reads all three languages the same way.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use super::analysis::Findings;
use super::correctness::ratio;
use super::process::{self, Log};
use super::sources::{self, SourceFile};
use super::{Context, Metrics};
use benchy::contract::Language;

pub fn measure(
    context: &Context,
    files: &[SourceFile],
    findings: &Findings,
    metrics: &mut Metrics,
    log: &mut Log,
) {
    let rubric = context.rubric;
    let lines = sources::production_line_total(files);
    let par = context.par.production_lines(context.language);
    let size = (par > 0).then(|| ratio(lines, par));
    log.line(&format!(
        "{} production lines: {lines} against par {par}",
        context.language.as_str()
    ));
    metrics.add(rubric, "E.production_lines", size);
    let dependencies = runtime_dependencies(context.root, context.language, files);
    log.line(&format!(
        "{} runtime dependencies: {dependencies:?}",
        context.language.as_str()
    ));
    metrics.add(rubric, "E.runtime_deps", Some(dependencies.len() as f64));
    let duplicated = duplicated_lines(context.root, files, log);
    metrics.add(
        rubric,
        "E.duplication",
        duplicated.map(|count| ratio(count, lines)),
    );
    metrics.add(rubric, "E.unused_code", Some(findings.unused as f64));
}

/// The direct runtime dependencies: crates, Go modules or NuGet packages that production code uses.
pub fn runtime_dependencies(
    root: &Path,
    language: Language,
    files: &[SourceFile],
) -> BTreeSet<String> {
    match language {
        Language::Rust => rust_dependencies(&root.join("rust")),
        Language::Go => go_dependencies(&root.join("go"), files),
        Language::CSharp => csharp_dependencies(&root.join("csharp")),
    }
}

fn manifests(dir: &Path, name: &str, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            if !["target", "bin", "obj", ".git"].contains(&file_name.as_str()) {
                manifests(&path, name, found);
            }
        } else if file_name == name || (name.starts_with('.') && file_name.ends_with(name)) {
            found.push(path);
        }
    }
}

fn rust_dependencies(dir: &Path) -> BTreeSet<String> {
    let mut paths = Vec::new();
    manifests(dir, "Cargo.toml", &mut paths);
    let mut names = BTreeSet::new();
    for path in paths {
        let Ok(manifest) = fs::read_to_string(&path).map(|text| text.parse::<toml::Table>()) else {
            continue;
        };
        let Ok(manifest) = manifest else { continue };
        let mut tables = vec![manifest.get("dependencies").cloned()];
        if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
            tables.extend(
                targets
                    .values()
                    .map(|target| target.get("dependencies").cloned()),
            );
        }
        for table in tables.into_iter().flatten() {
            let Some(table) = table.as_table() else {
                continue;
            };
            for (name, spec) in table {
                // A path dependency is a crate of the same workspace, not an outside dependency.
                if spec.get("path").is_none() {
                    names.insert(name.clone());
                }
            }
        }
    }
    names
}

fn go_dependencies(dir: &Path, files: &[SourceFile]) -> BTreeSet<String> {
    let go_mod = fs::read_to_string(dir.join("go.mod")).unwrap_or_default();
    let own = go_mod
        .lines()
        .find_map(|line| line.trim().strip_prefix("module "))
        .unwrap_or_default()
        .trim()
        .to_string();
    let required: Vec<String> = go_mod
        .lines()
        .map(str::trim)
        .map(|line| line.strip_prefix("require ").unwrap_or(line))
        .filter_map(|line| line.split_whitespace().next())
        .filter(|module| module.contains('.') && module.contains('/'))
        .map(str::to_string)
        .collect();
    let mut modules = BTreeSet::new();
    for file in files {
        for import in go_imports(&file.text) {
            let first = import.split('/').next().unwrap_or_default();
            if !first.contains('.') || import == own || import.starts_with(&format!("{own}/")) {
                continue;
            }
            let module = required
                .iter()
                .filter(|module| import == **module || import.starts_with(&format!("{module}/")))
                .max_by_key(|module| module.len())
                .cloned()
                .unwrap_or(import);
            modules.insert(module);
        }
    }
    modules
}

/// The import paths of a Go file, from single imports and import blocks.
fn go_imports(text: &str) -> Vec<String> {
    let mut imports = Vec::new();
    let mut in_block = false;
    for line in text.lines().map(str::trim) {
        if line.starts_with("import (") {
            in_block = true;
            continue;
        }
        if in_block && line.starts_with(')') {
            in_block = false;
            continue;
        }
        let spec = if in_block {
            Some(line)
        } else {
            line.strip_prefix("import ")
        };
        if let Some(path) = spec.and_then(|spec| spec.split('"').nth(1)) {
            imports.push(path.to_string());
        }
    }
    imports
}

fn csharp_dependencies(dir: &Path) -> BTreeSet<String> {
    let mut paths = Vec::new();
    manifests(dir, ".csproj", &mut paths);
    let mut names = BTreeSet::new();
    for path in paths {
        let text = fs::read_to_string(&path).unwrap_or_default();
        let is_test = ["Microsoft.NET.Test.Sdk", "xunit", "NUnit", "MSTest"]
            .iter()
            .any(|m| text.contains(m));
        if is_test {
            continue;
        }
        for piece in text.split("<PackageReference").skip(1) {
            if let Some(name) = piece
                .split("Include=\"")
                .nth(1)
                .and_then(|rest| rest.split('"').next())
            {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Production lines inside blocks that lizard reports as duplicated; absent when lizard could not run.
fn duplicated_lines(root: &Path, files: &[SourceFile], log: &mut Log) -> Option<usize> {
    if files.is_empty() {
        return Some(0);
    }
    let list: Vec<String> = files
        .iter()
        .map(|f| format!("'{}'", f.path.display()))
        .collect();
    let output = process::shell(root, &format!("lizard -Eduplicate {}", list.join(" ")), log);
    if !output.stdout.contains("Duplicates") {
        return None;
    }
    let mut lines: BTreeMap<String, BTreeSet<usize>> = BTreeMap::new();
    let section = output.stdout.split("Duplicates").nth(1).unwrap_or_default();
    for line in section.lines() {
        let Some((path, range)) = line.trim().rsplit_once(':') else {
            continue;
        };
        let Some((start, end)) = range.split_once('~') else {
            continue;
        };
        let (Ok(start), Ok(end)) = (start.trim().parse::<usize>(), end.trim().parse::<usize>())
        else {
            continue;
        };
        lines
            .entry(path.to_string())
            .or_default()
            .extend(start..=end);
    }
    let mut count = 0;
    for file in files {
        let shown = file.path.display().to_string();
        if let Some(numbers) = lines.get(&shown) {
            count += numbers
                .iter()
                .filter(|&&n| file.is_production_line(n))
                .count();
        }
    }
    log.line(&format!("duplicated production lines: {count}"));
    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_imports_are_read_from_blocks_and_single_lines() {
        let text =
            "package a\n\nimport \"fmt\"\n\nimport (\n\t\"os\"\n\tx \"github.com/a/b/c\"\n)\n";

        assert_eq!(go_imports(text), ["fmt", "os", "github.com/a/b/c"]);
    }

    #[test]
    fn rust_path_dependencies_do_not_count() {
        let dir = tempfile::tempdir().expect("temp dir");
        fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"a\"\n[dependencies]\nserde = \"1\"\ncore = { path = \"core\" }\n[dev-dependencies]\nproptest = \"1\"\n",
        )
        .expect("write");

        assert_eq!(
            rust_dependencies(dir.path()),
            BTreeSet::from(["serde".to_string()])
        );
    }
}
