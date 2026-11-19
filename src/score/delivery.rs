//! The measured parts of categories H and I: the build and program contract, and the hygiene of the delivered repository.
//! The commands of `DELIVERY.md` run as written, in the scored tree with the pristine `Makefile`.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::process::{self, Input, Log, Output};
use super::sources::SourceFile;
use super::workspace::{default_branch_head, git};
use super::{Context, Metrics, RUN_TIMEOUT};
use crate::run::workspace::DEFAULT_BRANCH;
use benchy::contract::Language;

/// The shell commands in the fenced code blocks of `DELIVERY.md`, without prompts and comments.
pub fn delivery_commands(text: &str) -> Vec<String> {
    let mut commands = Vec::new();
    let mut in_block = false;
    let mut is_shell = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if let Some(info) = trimmed.strip_prefix("```") {
            if in_block {
                in_block = false;
            } else {
                in_block = true;
                is_shell = ["", "sh", "bash", "shell", "console", "zsh"].contains(&info.trim());
            }
            continue;
        }
        if !in_block || !is_shell || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        commands.push(trimmed.strip_prefix("$ ").unwrap_or(trimmed).to_string());
    }
    commands
}

/// Which language a command is about, or none when it names none of them.
fn command_language(command: &str) -> Option<Language> {
    let words: Vec<String> = command
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '#')
        .map(str::to_lowercase)
        .collect();
    let has = |names: &[&str]| words.iter().any(|w| names.contains(&w.as_str()));
    if has(&["rust", "cargo"]) {
        Some(Language::Rust)
    } else if has(&["go", "golang", "gofmt"]) {
        Some(Language::Go)
    } else if has(&["csharp", "dotnet", "c#"]) {
        Some(Language::CSharp)
    } else {
        None
    }
}

/// Runs every command of `DELIVERY.md` once and records whether it succeeded.
pub fn run_delivery_commands(root: &Path, log: &mut Log) -> Vec<(String, bool)> {
    let text = fs::read_to_string(root.join("DELIVERY.md")).unwrap_or_default();
    let mut results = Vec::new();
    for command in delivery_commands(&text) {
        let output = process::shell(root, &command, log);
        results.push((command, output.success));
    }
    results
}

const RUST_IO: [&str; 13] = [
    "io::stdin(",
    "io::stdout(",
    "io::stderr(",
    "println!",
    "print!",
    "eprintln!",
    "eprint!",
    "dbg!",
    "std::fs",
    "File::open",
    "File::create",
    "fs::read",
    "fs::write",
];
const GO_IO: [&str; 11] = [
    "os.Stdin",
    "os.Stdout",
    "os.Stderr",
    "fmt.Print",
    "os.Open",
    "os.Create",
    "os.ReadFile",
    "os.WriteFile",
    "log.Print",
    "log.Fatal",
    "os.Exit",
];
const CSHARP_IO: [&str; 6] = [
    "Console.",
    "File.",
    "Directory.",
    "Environment.Exit",
    "Process.",
    "FileStream(",
];

/// Whether `code` uses `name` where it starts a word, so `SourceFile.` does not count as `File.`.
fn uses(code: &str, name: &str) -> bool {
    code.match_indices(name).any(|(at, _)| {
        code[..at]
            .chars()
            .next_back()
            .is_none_or(|before| !(before.is_alphanumeric() || before == '_'))
    })
}

/// The core library: production files outside the program, with at least one line of code.
fn core_does_no_io(files: &[SourceFile], language: Language) -> bool {
    let core: Vec<&SourceFile> = files
        .iter()
        .filter(|f| !f.is_program && f.production_line_count() > 0)
        .collect();
    let patterns: &[&str] = match language {
        Language::Rust => &RUST_IO,
        Language::Go => &GO_IO,
        Language::CSharp => &CSHARP_IO,
    };
    !core.is_empty()
        && core
            .iter()
            .all(|file| !patterns.iter().any(|pattern| uses(&file.code, pattern)))
}

fn prints_help(engine: &Path) -> bool {
    process::run(
        engine,
        &["--help"],
        &Input::Bytes(b""),
        &Output::Keep,
        RUN_TIMEOUT,
    )
    .is_ok_and(|run| {
        run.exited_cleanly() && !String::from_utf8_lossy(&run.stdout).trim().is_empty()
    })
}

pub fn measure_contract(
    context: &Context,
    files: &[SourceFile],
    delivery: &[(String, bool)],
    metrics: &mut Metrics,
    log: &mut Log,
) {
    let language = context.language;
    let own: Vec<&(String, bool)> = delivery
        .iter()
        .filter(|(command, _)| command_language(command).is_none_or(|l| l == language))
        .collect();
    let commands_run = !own.is_empty() && own.iter().all(|(_, ok)| *ok);
    let help = prints_help(&context.engine);
    let core = core_does_no_io(files, language);
    log.line(&format!(
        "{} contract: DELIVERY.md commands {commands_run} ({} of them), help {help}, core without input or output {core}",
        language.as_str(),
        own.len()
    ));
    let passed = [commands_run, help, core].iter().filter(|&&ok| ok).count();
    metrics.add(
        context.rubric,
        "H.build_contract",
        Some(passed as f64 / 3.0),
    );
}

const ALLOWED_TOP_LEVEL: [&str; 14] = [
    "Makefile",
    "TASK.md",
    "VERSION",
    "DELIVERY.md",
    "README.md",
    ".gitignore",
    ".gitattributes",
    ".editorconfig",
    "lint",
    "examples",
    "spec",
    "rust",
    "go",
    "csharp",
];
const BUILD_DIRS: [&str; 8] = [
    "target",
    "bin",
    "obj",
    "node_modules",
    "__pycache__",
    ".vs",
    ".idea",
    "TestResults",
];
const STRAY_SUFFIXES: [&str; 14] = [
    ".exe",
    ".dll",
    ".o",
    ".so",
    ".dylib",
    ".pdb",
    ".a",
    ".log",
    ".tmp",
    ".swp",
    ".orig",
    ".rej",
    "~",
    ".DS_Store",
];

/// Committed files that do not belong in a delivery: build output, editor and system files, and files outside the task layout.
fn stray_files(tracked: &[String], language: Language) -> Vec<String> {
    tracked
        .iter()
        .filter(|path| {
            let parts: Vec<&str> = path.split('/').collect();
            let top = parts.first().copied().unwrap_or_default();
            let ours = top == language.as_str() || !["rust", "go", "csharp"].contains(&top);
            let outside_layout = !ALLOWED_TOP_LEVEL.contains(&top);
            let build_output = parts.iter().any(|part| BUILD_DIRS.contains(part))
                || STRAY_SUFFIXES.iter().any(|suffix| path.ends_with(suffix));
            ours && (outside_layout || build_output)
        })
        .cloned()
        .collect()
}

fn lockfiles_present(root: &Path, language: Language) -> bool {
    match language {
        Language::Rust => root.join("rust/Cargo.lock").is_file(),
        Language::Go => {
            let go_mod = fs::read_to_string(root.join("go/go.mod")).unwrap_or_default();
            let requires = go_mod
                .lines()
                .any(|line| line.trim_start().starts_with("require"));
            !requires || root.join("go/go.sum").is_file()
        }
        Language::CSharp => {
            let mut projects = Vec::new();
            find_projects(&root.join("csharp"), &mut projects);
            projects.iter().all(|project| {
                let uses_packages =
                    fs::read_to_string(project).is_ok_and(|t| t.contains("<PackageReference"));
                !uses_packages || project.with_file_name("packages.lock.json").is_file()
            })
        }
    }
}

fn find_projects(dir: &Path, found: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if !["bin", "obj"].contains(&entry.file_name().to_string_lossy().as_ref()) {
                find_projects(&path, found);
            }
        } else if path.extension().is_some_and(|e| e == "csproj") {
            found.push(path);
        }
    }
}

/// The git facts that hygiene needs, read once from the workspace as it was handed in.
pub struct GitState {
    pub on_default_branch: bool,
    pub committed_dirs: BTreeMap<String, bool>,
    pub clean: bool,
    pub tracked: Vec<String>,
}

pub fn git_state(original: &Path) -> GitState {
    let head = default_branch_head(original);
    let branch = git(original, &["symbolic-ref", "--short", "HEAD"]).unwrap_or_default();
    let status = git(
        original,
        &["status", "--porcelain", "--untracked-files=all"],
    );
    let tracked: Vec<String> = head
        .as_ref()
        .and_then(|head| git(original, &["ls-tree", "-r", "--name-only", head]).ok())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let committed_dirs = Language::ALL
        .iter()
        .map(|language| {
            let name = language.as_str().to_string();
            let has_code = tracked
                .iter()
                .any(|p| p.starts_with(&format!("{name}/")) && !p.ends_with(".gitkeep"));
            (name, has_code)
        })
        .collect();
    GitState {
        on_default_branch: head.is_some() && branch.trim() == DEFAULT_BRANCH,
        committed_dirs,
        clean: status.is_ok_and(|status| status.trim().is_empty()),
        tracked,
    }
}

pub fn measure_hygiene(context: &Context, state: &GitState, metrics: &mut Metrics, log: &mut Log) {
    let language = context.language;
    let committed = state.on_default_branch
        && state
            .committed_dirs
            .get(language.as_str())
            .copied()
            .unwrap_or(false);
    let stray = stray_files(&state.tracked, language);
    let no_stray = !state.tracked.is_empty() && stray.is_empty();
    let lockfiles = lockfiles_present(context.root, language);
    log.line(&format!(
        "{} hygiene: committed on {DEFAULT_BRANCH} {committed}, no uncommitted work {}, stray {stray:?}, lockfiles {lockfiles}",
        language.as_str(),
        state.clean
    ));
    let passed = [committed, state.clean, no_stray, lockfiles]
        .iter()
        .filter(|&&ok| ok)
        .count();
    metrics.add(context.rubric, "I.hygiene", Some(passed as f64 / 4.0));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_top_level_file_of_the_task_pack_is_allowed() {
        let task = Path::new(env!("CARGO_MANIFEST_DIR")).join("task");
        let tracked: Vec<String> = fs::read_dir(&task)
            .expect("read task")
            .map(|entry| {
                entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect();

        assert_eq!(stray_files(&tracked, Language::Rust), Vec::<String>::new());
    }

    #[test]
    fn delivery_commands_come_from_shell_blocks_only() {
        let text = "# Delivery\n\n```sh\n$ make build-rust\n# a comment\ncd go && go test ./...\n```\n\n```text\nnot a command\n```\n";

        assert_eq!(
            delivery_commands(text),
            ["make build-rust", "cd go && go test ./..."]
        );
    }

    #[test]
    fn commands_belong_to_the_language_they_name() {
        assert_eq!(
            command_language("make build-go test-go"),
            Some(Language::Go)
        );
        assert_eq!(
            command_language("cd rust && cargo test"),
            Some(Language::Rust)
        );
        assert_eq!(
            command_language("dotnet test csharp"),
            Some(Language::CSharp)
        );
        assert_eq!(command_language("make clean"), None);
    }

    #[test]
    fn build_output_and_files_outside_the_layout_are_stray() {
        let tracked = [
            "rust/src/lib.rs",
            "rust/target/debug/engine",
            "notes.txt",
            "go/main.go",
            "DELIVERY.md",
        ]
        .map(String::from);

        assert_eq!(
            stray_files(&tracked, Language::Rust),
            ["rust/target/debug/engine", "notes.txt"]
        );
        assert_eq!(stray_files(&tracked, Language::Go), ["notes.txt"]);
    }

    #[test]
    fn a_name_inside_a_longer_word_is_not_a_use() {
        assert!(uses("Console.WriteLine(x);", "Console."));
        assert!(!uses("var f = SourceFile.Open();", "File."));
    }
}
