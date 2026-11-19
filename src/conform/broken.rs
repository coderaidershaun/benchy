//! Tests that the golden suite passes the Rust reference and fails every broken variant of it.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::Deserialize;

use super::{Report, Verdict, discover, run_sessions};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variants {
    variant: Vec<Variant>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Variant {
    name: String,
    group: String,
    change: Vec<Change>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    file: PathBuf,
    find: String,
    replace: String,
}

fn repo() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn reference_source() -> PathBuf {
    repo().join("validation/reference/rust")
}

/// Copies the reference crate's manifest, lockfile and sources into `dir`.
fn copy_reference(dir: &Path) {
    let source = reference_source();
    for file in ["Cargo.toml", "Cargo.lock"] {
        fs::copy(source.join(file), dir.join(file)).unwrap();
    }
    fs::create_dir(dir.join("src")).unwrap();
    for entry in fs::read_dir(source.join("src")).unwrap() {
        let path = entry.unwrap().path();
        fs::copy(&path, dir.join("src").join(path.file_name().unwrap())).unwrap();
    }
}

/// Builds the engine crate in `dir` and returns the path of its program.
fn build(dir: &Path) -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .args(["build", "--quiet", "--offline", "--bin", "engine"])
        .env("CARGO_TARGET_DIR", dir.join("target"))
        .env_remove("RUSTFLAGS")
        .current_dir(dir)
        .status()
        .unwrap();
    assert!(status.success(), "failed to build {}", dir.display());
    dir.join("target/debug/engine")
}

fn golden_report(engine: &Path, dirs: &[PathBuf]) -> Report {
    let sessions = discover(dirs).unwrap();
    Report::new(&run_sessions(engine, &sessions, Duration::from_secs(60)))
}

#[test]
fn the_reference_passes_the_public_examples_and_every_golden_session() {
    let dir = tempfile::tempdir().unwrap();
    copy_reference(dir.path());
    let engine = build(dir.path());
    for sessions in ["task/examples", "validation/scenarios"] {
        let report = golden_report(&engine, &[repo().join(sessions)]);
        assert!(report.all_passed(), "{sessions}:\n{report}");
    }
}

/// The Go and C# references test against their own copies of the public examples. A copy that
/// differs from the task pack would let a reference pass tests the harness never sees.
#[test]
fn the_reference_copies_of_the_public_examples_match_the_task_pack() {
    let examples = repo().join("task/examples");
    let mut names: Vec<_> = std::fs::read_dir(&examples)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    names.sort();
    assert!(!names.is_empty(), "task/examples is empty");
    for copy in [
        "validation/reference/go/cmd/engine/testdata",
        "validation/reference/csharp/Engine.Tests/Examples",
    ] {
        let copy = repo().join(copy);
        let mut copied: Vec<_> = std::fs::read_dir(&copy)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        copied.sort();
        assert_eq!(copied, names, "{} holds other files", copy.display());
        for name in &names {
            assert!(
                std::fs::read(examples.join(name)).unwrap()
                    == std::fs::read(copy.join(name)).unwrap(),
                "{} differs from task/examples",
                copy.join(name).display()
            );
        }
    }
}

#[test]
fn the_golden_suite_has_hand_written_sessions_in_every_group() {
    let sessions = discover(&[repo().join("validation/scenarios")]).unwrap();
    let hand_written = sessions
        .iter()
        .filter(|session| session.name.starts_with("hand-"))
        .count();
    assert!(sessions.len() >= 300, "only {} sessions", sessions.len());
    assert!(
        hand_written >= 40,
        "only {hand_written} hand-written sessions"
    );
    let report = Report::new(
        &sessions
            .into_iter()
            .map(|session| super::Outcome {
                session,
                verdict: Verdict::Pass,
            })
            .collect::<Vec<_>>(),
    );
    assert_eq!(report.groups.len(), 15);
}

#[test]
fn the_golden_suite_fails_every_broken_variant_in_its_own_group() {
    let text = fs::read_to_string(repo().join("src/conform/broken.toml")).unwrap();
    let variants: Variants = toml::from_str(&text).unwrap();
    let scenarios = repo().join("validation/scenarios");
    let groups: Vec<String> = fs::read_dir(&scenarios)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    for group in &groups {
        if scenarios.join(group).is_dir() {
            assert!(
                variants
                    .variant
                    .iter()
                    .any(|variant| &variant.group == group),
                "no broken variant for group {group}"
            );
        }
    }
    for variant in &variants.variant {
        let dir = tempfile::tempdir().unwrap();
        copy_reference(dir.path());
        for change in &variant.change {
            let path = dir.path().join(&change.file);
            let source = fs::read_to_string(&path).unwrap();
            assert_eq!(
                source.matches(&change.find).count(),
                1,
                "variant {}: the text to change must occur once in {}",
                variant.name,
                change.file.display()
            );
            fs::write(&path, source.replacen(&change.find, &change.replace, 1)).unwrap();
        }
        let engine = build(dir.path());
        let report = golden_report(&engine, std::slice::from_ref(&scenarios));
        let own_group = report.groups[&variant.group];
        assert!(
            own_group.passed < own_group.total,
            "variant {} passes every session of group {}",
            variant.name,
            variant.group
        );
    }
}
