//! Runs the kill command against each mutant: the program build first, then `make test-<language>`, in parallel copies of the tree.
//! Every command runs in its own process group, so a mutant that hangs is killed with all the tools it started.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use super::Mutant;
use benchy::contract::Language;

/// What happened to one mutant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The tests failed.
    Killed,
    /// The tests did not finish in time, which a suite run with a time limit also reports as a failure.
    TimedOut,
    /// The tests passed.
    Survived,
    /// The program does not build with the mutant in place, so it says nothing about the tests.
    Unviable,
}

impl Outcome {
    pub fn is_killed(self) -> bool {
        matches!(self, Outcome::Killed | Outcome::TimedOut)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Outcome::Killed => "killed",
            Outcome::TimedOut => "killed (timed out)",
            Outcome::Survived => "survived",
            Outcome::Unviable => "does not build",
        }
    }
}

/// The result of one mutant, with the time its build and tests took.
#[derive(Debug, Clone)]
pub struct Tried {
    pub outcome: Outcome,
    pub elapsed: Duration,
}

/// The longest a mutant's program build may take.
const BUILD_TIMEOUT: Duration = Duration::from_secs(600);

/// How a command ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    Passed,
    Failed,
    TimedOut,
}

/// Runs `line` with `sh -c` in `dir`, its output in `output`, and kills its whole process group at `timeout`.
/// The group is killed after a normal exit too, so no tool it started outlives it.
pub fn run_command(dir: &Path, line: &str, output: &Path, timeout: Duration) -> (Ended, Duration) {
    use std::os::unix::process::CommandExt;
    let started = Instant::now();
    let Ok(log) = fs::File::create(output) else {
        return (Ended::Failed, started.elapsed());
    };
    let Ok(log_err) = log.try_clone() else {
        return (Ended::Failed, started.elapsed());
    };
    let child = Command::new("sh")
        .arg("-c")
        .arg(line)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(log)
        .stderr(log_err)
        .process_group(0)
        .spawn();
    let Ok(mut child) = child else {
        return (Ended::Failed, started.elapsed());
    };
    let group = i32::try_from(child.id()).expect("process ids fit in i32");
    let ended = loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break Ended::Passed,
            Ok(Some(_)) | Err(_) => break Ended::Failed,
            Ok(None) if started.elapsed() > timeout => break Ended::TimedOut,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
        }
    };
    // SAFETY: sending a signal has no memory effects; a group that is already gone only makes kill return an error.
    unsafe { libc::kill(-group, libc::SIGKILL) };
    drop(child.wait());
    (ended, started.elapsed())
}

/// Copies the tree that is scored without its build output, as `make clean` would leave it.
fn copy_without_build_output(from: &Path, to: &Path, relative: &Path) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        let path = relative.join(&name);
        let is_build_output = path == Path::new("bin")
            || path == Path::new("rust/target")
            || (path.starts_with("csharp") && (name == "bin" || name == "obj"));
        let kind = entry.file_type()?;
        if kind.is_dir() && !is_build_output {
            copy_without_build_output(&entry.path(), &to.join(&name), &path)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), to.join(&name))?;
        }
    }
    Ok(())
}

/// Tries every mutant against the tree in `root`, on `workers` copies of it below `scratch`.
/// Results come back in the order of `mutants`; `progress` hears of each one as it finishes.
///
/// # Errors
///
/// Fails when the copies cannot be made.
pub fn try_all(
    root: &Path,
    scratch: &Path,
    language: Language,
    mutants: &[Mutant],
    test_timeout: Duration,
    workers: usize,
    progress: impl Fn(usize, &Mutant, &Tried) + Sync,
) -> std::io::Result<Vec<Tried>> {
    let workers = workers.clamp(1, mutants.len().max(1));
    let mut dirs = Vec::new();
    for worker in 0..workers {
        let dir = scratch.join(format!("worker-{worker}"));
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        copy_without_build_output(root, &dir, Path::new(""))?;
        dirs.push(dir);
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(mutants.len()));
    thread::scope(|scope| {
        for dir in &dirs {
            let (next, results, progress) = (&next, &results, &progress);
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(mutant) = mutants.get(index) else {
                        break;
                    };
                    let tried = try_one(dir, language, mutant, test_timeout);
                    progress(index, mutant, &tried);
                    results
                        .lock()
                        .expect("no worker panics while it holds the lock")
                        .push((index, tried));
                }
            });
        }
    });
    for dir in &dirs {
        drop(fs::remove_dir_all(dir));
    }
    let mut results = results.into_inner().expect("every worker finished");
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, tried)| tried).collect())
}

fn try_one(dir: &Path, language: Language, mutant: &Mutant, test_timeout: Duration) -> Tried {
    let started = Instant::now();
    let path = dir.join(&mutant.file);
    let Ok(original) = fs::read_to_string(&path) else {
        return Tried {
            outcome: Outcome::Unviable,
            elapsed: started.elapsed(),
        };
    };
    let Some(mutated) = mutant.apply(&original) else {
        return Tried {
            outcome: Outcome::Unviable,
            elapsed: started.elapsed(),
        };
    };
    let outcome = if fs::write(&path, mutated).is_ok() {
        build_and_test(dir, language, test_timeout)
    } else {
        Outcome::Unviable
    };
    // A failed restore would poison every later mutant of this worker, so it is a bug, not a result.
    fs::write(&path, original).expect("the worker copy can be written, as it just was");
    Tried {
        outcome,
        elapsed: started.elapsed(),
    }
}

fn build_and_test(dir: &Path, language: Language, test_timeout: Duration) -> Outcome {
    let name = language.as_str();
    let output = PathBuf::from(format!("{}.log", dir.display()));
    let (built, _) = run_command(dir, &format!("make build-{name}"), &output, BUILD_TIMEOUT);
    if built != Ended::Passed {
        return Outcome::Unviable;
    }
    match run_command(dir, &format!("make test-{name}"), &output, test_timeout).0 {
        Ended::Passed => Outcome::Survived,
        Ended::Failed => Outcome::Killed,
        Ended::TimedOut => Outcome::TimedOut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_past_its_timeout_is_killed_with_its_children() {
        let dir = tempfile::tempdir().expect("temp dir");
        let output = dir.path().join("out.log");

        let (ended, elapsed) = run_command(
            dir.path(),
            "sleep 30 & sleep 30; echo late",
            &output,
            Duration::from_millis(300),
        );

        assert_eq!(ended, Ended::TimedOut);
        assert!(elapsed < Duration::from_secs(10));
        assert!(!fs::read_to_string(&output).expect("log").contains("late"));
    }

    #[test]
    fn a_command_reports_pass_and_failure_and_keeps_its_output() {
        let dir = tempfile::tempdir().expect("temp dir");
        let output = dir.path().join("out.log");

        let passed = run_command(dir.path(), "echo hello", &output, Duration::from_secs(10)).0;
        let text = fs::read_to_string(&output).expect("log");
        let failed = run_command(dir.path(), "exit 3", &output, Duration::from_secs(10)).0;

        assert_eq!(passed, Ended::Passed);
        assert_eq!(text, "hello\n");
        assert_eq!(failed, Ended::Failed);
    }

    #[test]
    fn copies_leave_out_build_output_but_keep_sources_named_like_it() {
        let from = tempfile::tempdir().expect("temp dir");
        for dir in [
            "bin/rust",
            "rust/target/release",
            "rust/src/bin",
            "csharp/Engine/obj",
            "go/cmd",
        ] {
            fs::create_dir_all(from.path().join(dir)).expect("create");
        }
        fs::write(from.path().join("rust/src/bin/tool.rs"), "fn main() {}").expect("write");
        fs::write(from.path().join("rust/target/release/engine"), "x").expect("write");
        let to = tempfile::tempdir().expect("temp dir");

        copy_without_build_output(from.path(), to.path(), Path::new("")).expect("copy");

        assert!(to.path().join("rust/src/bin/tool.rs").is_file());
        assert!(to.path().join("go/cmd").is_dir());
        assert!(!to.path().join("bin").exists());
        assert!(!to.path().join("rust/target").exists());
        assert!(!to.path().join("csharp/Engine/obj").exists());
    }
}
