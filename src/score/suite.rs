//! Category D: whether the tests the harness wrote pass every time, how many sampled mutants they catch, and what they cost.
//! Mutants come only from live lines of the core library: no test can catch a change to code that never runs,
//! so dead code costs points in category E and nowhere else.

use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use super::analysis::Findings;
use super::correctness::ratio;
use super::process::Log;
use super::sources::{self, SourceFile};
use super::test_code::{Kind, Reachable, Suite};
use super::{Context, Metrics};
use crate::mutate::run::{Ended, run_command, try_all};
use crate::mutate::{self, Mutant};
use benchy::contract::Language;

const KEYS: [&str; 7] = [
    "D.pass_stable",
    "D.mutation",
    "D.mix",
    "D.test_ratio",
    "D.kill_density",
    "D.suite_time",
    "D.assertion_free",
];

/// The longest one run of the unmutated suite may take.
const SUITE_TIMEOUT: Duration = Duration::from_secs(900);
/// The least time a mutant's tests get before they count as hung.
const MIN_MUTANT_TIMEOUT: Duration = Duration::from_secs(180);

pub fn measure(
    context: &Context,
    files: &[SourceFile],
    findings: &Findings,
    metrics: &mut Metrics,
    log: &mut Log,
) {
    let rubric = context.rubric;
    let language = context.language;
    let name = language.as_str();
    let suite = Suite::read(context.root, language);
    let reachable = Reachable::read(context.root, language, files);
    let tests = suite.classify(language, &reachable);
    if tests.is_empty() {
        metrics.note(format!(
            "no {name} tests were found, so category D scores 0"
        ));
        for key in KEYS {
            metrics.add(rubric, key, None);
        }
        return;
    }
    let test_lines = suite.line_count();
    let production_lines = sources::production_line_total(files);
    let kinds: BTreeSet<Kind> = tests.iter().filter_map(|t| t.kind).collect();
    let without_assertion = tests.iter().filter(|t| !t.asserts).count();
    log.line(&format!(
        "{name} tests: {}, test lines {test_lines}, kinds {kinds:?}, without an assertion {without_assertion}",
        tests.len()
    ));
    for test in tests.iter().filter(|t| !t.asserts || t.kind.is_none()) {
        log.line(&format!(
            "  test {}: asserts {}, kind {:?}",
            test.name, test.asserts, test.kind
        ));
    }
    let runs = stable_runs(context, log);
    metrics.add(
        rubric,
        "D.pass_stable",
        Some(f64::from(u8::from(runs.all_passed))),
    );
    let mutation = if runs.all_passed {
        mutants(context, files, findings, runs.first, log)
    } else {
        metrics.note(format!(
            "the {name} suite does not pass every run, so no mutant can count as caught"
        ));
        None
    };
    metrics.add(
        rubric,
        "D.mutation",
        mutation.and_then(|m| (m.viable > 0).then(|| ratio(m.killed, m.viable))),
    );
    metrics.add(rubric, "D.mix", Some(kinds.len() as f64 / 3.0));
    metrics.add(
        rubric,
        "D.test_ratio",
        (production_lines > 0).then(|| ratio(test_lines, production_lines)),
    );
    metrics.add(
        rubric,
        "D.kill_density",
        mutation
            .and_then(|m| (test_lines > 0).then(|| m.killed as f64 * 100.0 / test_lines as f64)),
    );
    metrics.add(rubric, "D.suite_time", runs.suite_seconds);
    metrics.add(
        rubric,
        "D.assertion_free",
        Some(ratio(without_assertion, tests.len())),
    );
}

struct Runs {
    all_passed: bool,
    /// How long the first run took, test build included.
    first: Duration,
    /// The median of the later runs, whose builds have nothing left to do.
    suite_seconds: Option<f64>,
}

/// Runs the suite `test_runs` times with the pristine `Makefile`; Go once more, shuffled and with the race detector.
fn stable_runs(context: &Context, log: &mut Log) -> Runs {
    let name = context.language.as_str();
    let output = context.scratch.join(format!("tests-{name}.log"));
    let mut durations = Vec::new();
    let mut all_passed = true;
    for run in 1..=context.rubric.measurement.test_runs {
        let (ended, elapsed) = run_command(
            context.root,
            &format!("make test-{name}"),
            &output,
            SUITE_TIMEOUT,
        );
        log.line(&format!(
            "make test-{name} run {run}: {ended:?} in {:.1}s",
            elapsed.as_secs_f64()
        ));
        if ended != Ended::Passed {
            log_tail(&output, log);
        }
        all_passed &= ended == Ended::Passed;
        durations.push(elapsed);
    }
    if context.language == Language::Go {
        let (ended, elapsed) = run_command(
            context.root,
            "cd go && CGO_ENABLED=1 GOFLAGS=-mod=readonly GOWORK=off go test -count=1 -shuffle=on -race ./...",
            &output,
            SUITE_TIMEOUT,
        );
        log.line(&format!(
            "go test shuffled with the race detector: {ended:?} in {:.1}s",
            elapsed.as_secs_f64()
        ));
        if ended != Ended::Passed {
            log_tail(&output, log);
        }
        all_passed &= ended == Ended::Passed;
    }
    let mut later: Vec<f64> = durations
        .iter()
        .skip(1)
        .map(Duration::as_secs_f64)
        .collect();
    later.sort_by(f64::total_cmp);
    Runs {
        all_passed,
        first: durations.first().copied().unwrap_or_default(),
        suite_seconds: later.get(later.len() / 2).copied(),
    }
}

fn log_tail(output: &Path, log: &mut Log) {
    let text = std::fs::read_to_string(output).unwrap_or_default();
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    for line in &lines[lines.len().saturating_sub(15)..] {
        log.line(&format!("    {line}"));
    }
}

#[derive(Debug, Clone, Copy)]
struct Mutation {
    killed: usize,
    viable: usize,
}

/// Lines of `file` that the analysers report as unused, with the whole body of each unused function or type.
pub fn unused_lines(file: &SourceFile, findings: &Findings) -> BTreeSet<usize> {
    let code: Vec<&str> = file.code.lines().collect();
    let mut lines = BTreeSet::new();
    for (path, line) in &findings.unused_at {
        if !same_file(path, &file.path) || *line == 0 {
            continue;
        }
        lines.insert(*line);
        let declaration = code.get(line - 1).copied().unwrap_or_default();
        let next = code.get(*line).copied().unwrap_or_default();
        let opens_here = declaration.contains('{');
        let opens_next = !declaration.contains(';') && next.trim() == "{";
        if !(opens_here || opens_next) {
            continue;
        }
        let mut depth = 0usize;
        for (index, text) in code.iter().enumerate().skip(line - 1) {
            for c in text.chars() {
                match c {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    _ => {}
                }
            }
            lines.insert(index + 1);
            if depth == 0 && index + 1 > *line - usize::from(!opens_next) {
                break;
            }
        }
    }
    lines
}

/// Whether a path an analyser printed, relative or absolute, names the workspace file `relative`.
fn same_file(reported: &str, relative: &Path) -> bool {
    let reported = Path::new(reported);
    let parts: Vec<_> = relative.components().collect();
    (1..=parts.len()).any(|skip| {
        let tail: std::path::PathBuf = parts[parts.len() - skip..].iter().collect();
        reported.ends_with(&tail) && (skip >= 2 || reported == tail.as_path())
    }) || reported.ends_with(relative)
}

fn mutants(
    context: &Context,
    files: &[SourceFile],
    findings: &Findings,
    first_run: Duration,
    log: &mut Log,
) -> Option<Mutation> {
    let language = context.language;
    let name = language.as_str();
    let mut candidates: Vec<Mutant> = Vec::new();
    let mut dead = 0;
    for file in files.iter().filter(|f| !f.is_program) {
        let unused = unused_lines(file, findings);
        dead += unused.len();
        candidates.extend(mutate::candidates(
            &file.path,
            &file.code,
            language,
            |line| file.is_production_line(line) && !unused.contains(&line),
        ));
    }
    let count =
        usize::try_from(context.rubric.measurement.mutants_per_language).unwrap_or(usize::MAX);
    let sample = mutate::sample(&candidates, count, mutate::SEED);
    log.line(&format!(
        "{name} mutants: {} sampled from {} candidates; {dead} unused lines left out",
        sample.len(),
        candidates.len()
    ));
    if sample.is_empty() {
        return None;
    }
    let timeout = MIN_MUTANT_TIMEOUT.max(first_run * 5);
    let workers = std::thread::available_parallelism().map_or(2, usize::from);
    let progress = |index: usize, mutant: &Mutant, tried: &mutate::run::Tried| {
        eprintln!(
            "mutant {}/{} {mutant}: {} in {:.1}s",
            index + 1,
            sample.len(),
            tried.outcome.as_str(),
            tried.elapsed.as_secs_f64()
        );
    };
    let scratch = context.scratch.join(format!("mutants-{name}"));
    let tried = match try_all(
        context.root,
        &scratch,
        language,
        &sample,
        timeout,
        workers,
        progress,
    ) {
        Ok(tried) => tried,
        Err(error) => {
            log.line(&format!("could not copy the tree for mutants: {error}"));
            return None;
        }
    };
    for (mutant, result) in sample.iter().zip(&tried) {
        log.text.push_str(&format!(
            "    {mutant}: {} in {:.1}s\n",
            result.outcome.as_str(),
            result.elapsed.as_secs_f64()
        ));
    }
    let killed = tried.iter().filter(|t| t.outcome.is_killed()).count();
    let viable = tried
        .iter()
        .filter(|t| t.outcome != mutate::run::Outcome::Unviable)
        .count();
    log.line(&format!(
        "{name} mutants: {killed} caught of {viable} that build ({} do not build)",
        tried.len() - viable
    ));
    Some(Mutation { killed, viable })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn source(path: &str, text: &str, language: Language) -> SourceFile {
        let code = sources::strip(text, language);
        let production_lines = code.lines().map(|l| !l.trim().is_empty()).collect();
        SourceFile {
            path: PathBuf::from(path),
            text: text.to_string(),
            code,
            production_lines,
            is_program: false,
            comment_lines: 0,
        }
    }

    #[test]
    fn an_unused_function_is_left_out_with_its_whole_body() {
        let rust = "fn used() -> u8 {\n    1\n}\n\nfn unused(x: u8) -> u8 {\n    if x > 1 {\n        x\n    } else {\n        2\n    }\n}\n\nfn after() {}\n";
        let csharp = "class A\n{\n    private static int Unused(int x)\n    {\n        return x + 1;\n    }\n\n    public int Used() => 1;\n}\n";
        let findings = Findings {
            unused_at: vec![
                ("src/book.rs".to_string(), 5),
                ("/work/scored/csharp/Matching/A.cs".to_string(), 3),
            ],
            ..Findings::default()
        };

        let rust_lines = unused_lines(&source("rust/src/book.rs", rust, Language::Rust), &findings);
        let csharp_lines = unused_lines(
            &source("csharp/Matching/A.cs", csharp, Language::CSharp),
            &findings,
        );

        assert_eq!(rust_lines, (5..=11).collect());
        assert_eq!(csharp_lines, (3..=6).collect());
    }

    #[test]
    fn reported_paths_match_by_their_last_components() {
        assert!(same_file("src/book.rs", Path::new("rust/src/book.rs")));
        assert!(same_file(
            "matching/book.go",
            Path::new("go/matching/book.go")
        ));
        assert!(same_file(
            "/w/csharp/Matching/A.cs",
            Path::new("csharp/Matching/A.cs")
        ));
        assert!(!same_file("src/lib.rs", Path::new("rust/src/book.rs")));
        assert!(!same_file("other/book.rs", Path::new("rust/src/book.rs")));
    }
}
