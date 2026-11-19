//! The conformance runner: runs an engine program over golden sessions and compares its output byte for byte.
//! A session is a `NAME.in` file with its expected `NAME.out`; its group is the folder that holds it.

#[cfg(test)]
mod broken;

use std::collections::BTreeMap;
use std::fmt;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

#[derive(Debug, clap::Args)]
pub struct ConformArgs {
    /// The engine program to test.
    #[arg(long)]
    pub engine: PathBuf,
    /// Folders of sessions. Each `.in` file below them needs an `.out` file beside it.
    #[arg(default_value = "validation/scenarios")]
    pub dirs: Vec<PathBuf>,
    /// Longest time one session may run before it counts as failed.
    #[arg(long, default_value_t = 60)]
    pub timeout_seconds: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ConformError {
    #[error("failed to read {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("session {input} has no expected output {expected}")]
    MissingExpected { input: PathBuf, expected: PathBuf },
    #[error("no sessions found under {dirs:?}")]
    NoSessions { dirs: Vec<PathBuf> },
}

/// Runs `benchy conform` and returns whether every session passed.
pub fn run(args: &ConformArgs) -> Result<bool, ConformError> {
    let sessions = discover(&args.dirs)?;
    let outcomes = run_sessions(
        &args.engine,
        &sessions,
        Duration::from_secs(args.timeout_seconds),
    );
    let report = Report::new(&outcomes);
    print!("{report}");
    Ok(report.all_passed())
}

#[derive(Debug, Clone)]
pub struct Session {
    pub group: String,
    pub name: String,
    pub input: PathBuf,
    pub expected: PathBuf,
}

/// Finds every session below `dirs`, sorted by group and name.
pub fn discover(dirs: &[PathBuf]) -> Result<Vec<Session>, ConformError> {
    let mut sessions = Vec::new();
    for dir in dirs {
        let top_group = dir
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        collect(dir, dir, &top_group, &mut sessions)?;
    }
    if sessions.is_empty() {
        return Err(ConformError::NoSessions {
            dirs: dirs.to_vec(),
        });
    }
    sessions.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    Ok(sessions)
}

fn collect(
    root: &Path,
    dir: &Path,
    top_group: &str,
    sessions: &mut Vec<Session>,
) -> Result<(), ConformError> {
    let read_error = |source| ConformError::Read {
        path: dir.to_path_buf(),
        source,
    };
    for entry in std::fs::read_dir(dir).map_err(read_error)? {
        let path = entry.map_err(read_error)?.path();
        if path.is_dir() {
            collect(root, &path, top_group, sessions)?;
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "in") {
            continue;
        }
        let expected = path.with_extension("out");
        if !expected.is_file() {
            return Err(ConformError::MissingExpected {
                input: path,
                expected,
            });
        }
        let relative = dir.strip_prefix(root).unwrap_or(dir);
        let group = if relative.as_os_str().is_empty() {
            top_group.to_owned()
        } else {
            relative.to_string_lossy().into_owned()
        };
        let name = path
            .file_stem()
            .map(|stem| stem.to_string_lossy().into_owned())
            .unwrap_or_default();
        sessions.push(Session {
            group,
            name,
            input: path,
            expected,
        });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    Pass,
    /// The output differs first at this line, counted from 1.
    Differs {
        line: usize,
        expected: Option<String>,
        actual: Option<String>,
    },
    /// The engine could not be run, crashed, exited with an error or ran out of time.
    Broken(String),
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub session: Session,
    pub verdict: Verdict,
}

/// Runs `engine` over every session, several at a time.
pub fn run_sessions(engine: &Path, sessions: &[Session], timeout: Duration) -> Vec<Outcome> {
    let next = AtomicUsize::new(0);
    let outcomes = Mutex::new(Vec::with_capacity(sessions.len()));
    let workers = thread::available_parallelism().map_or(4, usize::from);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                while let Some(session) = sessions.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let verdict = judge(engine, session, timeout);
                    let outcome = Outcome {
                        session: session.clone(),
                        verdict,
                    };
                    outcomes
                        .lock()
                        .expect("a worker never panics while it holds the lock")
                        .push(outcome);
                }
            });
        }
    });
    let mut outcomes = outcomes
        .into_inner()
        .expect("every worker has finished without panicking");
    outcomes.sort_by(|a, b| {
        (&a.session.group, &a.session.name).cmp(&(&b.session.group, &b.session.name))
    });
    outcomes
}

fn judge(engine: &Path, session: &Session, timeout: Duration) -> Verdict {
    let read =
        |path: &Path| std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()));
    let (input, expected) = match (read(&session.input), read(&session.expected)) {
        (Ok(input), Ok(expected)) => (input, expected),
        (Err(error), _) | (_, Err(error)) => return Verdict::Broken(error),
    };
    match run_engine(engine, &input, timeout) {
        Ok(actual) => compare(&expected, &actual),
        Err(error) => Verdict::Broken(error),
    }
}

/// Runs `engine` with `input` on standard input and returns its standard output.
/// A failure to start, an exit code other than 0 or a run longer than `timeout` is an error.
pub fn run_engine(engine: &Path, input: &[u8], timeout: Duration) -> Result<Vec<u8>, String> {
    let mut child = Command::new(engine)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", engine.display()))?;
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let (status, output) = thread::scope(|scope| {
        scope.spawn(move || {
            // An engine may exit before it reads all its input; only its output and exit code count.
            let _ = stdin.write_all(input);
        });
        let reader = scope.spawn(move || {
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).map(|_| output)
        });
        let status = wait_with_timeout(&mut child, timeout);
        (
            status,
            reader.join().expect("the reader thread does not panic"),
        )
    });
    let status = status?;
    if !status.success() {
        return Err(format!("the engine exited with {status}"));
    }
    output.map_err(|error| format!("failed to read the engine's output: {error}"))
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) if started.elapsed() > timeout => {
                // Killing the engine closes its pipes, so the helper threads finish too.
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {} seconds", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(1)),
            Err(error) => return Err(format!("failed to wait for the engine: {error}")),
        }
    }
}

/// Compares outputs line by line and reports the first line that differs.
pub fn compare(expected: &[u8], actual: &[u8]) -> Verdict {
    if expected == actual {
        return Verdict::Pass;
    }
    let mut expected_lines = expected.split_inclusive(|&byte| byte == b'\n');
    let mut actual_lines = actual.split_inclusive(|&byte| byte == b'\n');
    let mut line = 1;
    loop {
        let (want, got) = (expected_lines.next(), actual_lines.next());
        if want != got {
            let show = |text: &[u8]| String::from_utf8_lossy(text).escape_debug().to_string();
            return Verdict::Differs {
                line,
                expected: want.map(show),
                actual: got.map(show),
            };
        }
        line += 1;
    }
}

/// Pass counts for each group, and the failing sessions.
#[derive(Debug)]
pub struct Report {
    pub groups: BTreeMap<String, GroupCount>,
    pub failures: Vec<Outcome>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GroupCount {
    pub passed: usize,
    pub total: usize,
}

impl GroupCount {
    pub fn rate(self) -> f64 {
        self.passed as f64 / self.total as f64
    }
}

impl Report {
    pub fn new(outcomes: &[Outcome]) -> Report {
        let mut groups: BTreeMap<String, GroupCount> = BTreeMap::new();
        let mut failures = Vec::new();
        for outcome in outcomes {
            let count = groups.entry(outcome.session.group.clone()).or_default();
            count.total += 1;
            if outcome.verdict == Verdict::Pass {
                count.passed += 1;
            } else {
                failures.push(outcome.clone());
            }
        }
        Report { groups, failures }
    }

    pub fn all_passed(&self) -> bool {
        self.failures.is_empty()
    }

    /// The golden pass rate: the mean of the group rates, so a large group cannot hide a small one.
    pub fn mean_group_rate(&self) -> f64 {
        let sum: f64 = self.groups.values().map(|count| count.rate()).sum();
        sum / self.groups.len() as f64
    }
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "{:<24} {:>7} {:>7} {:>8}",
            "group", "passed", "total", "rate"
        )?;
        for (group, count) in &self.groups {
            let percent = 100.0 * count.rate();
            writeln!(
                f,
                "{group:<24} {:>7} {:>7} {percent:>7.1}%",
                count.passed, count.total
            )?;
        }
        let passed: usize = self.groups.values().map(|count| count.passed).sum();
        let total: usize = self.groups.values().map(|count| count.total).sum();
        writeln!(
            f,
            "sessions passed: {passed} of {total}; mean of group rates: {:.1}%",
            100.0 * self.mean_group_rate()
        )?;
        for failure in &self.failures {
            let session = &failure.session;
            write!(f, "FAIL {}/{}: ", session.group, session.name)?;
            match &failure.verdict {
                Verdict::Differs {
                    line,
                    expected,
                    actual,
                } => {
                    let shown = |text: &Option<String>| {
                        text.as_deref()
                            .map_or("end of output".to_owned(), |text| format!("\"{text}\""))
                    };
                    writeln!(
                        f,
                        "output line {line}: expected {}, got {}",
                        shown(expected),
                        shown(actual)
                    )?;
                }
                Verdict::Broken(reason) => writeln!(f, "{reason}")?,
                Verdict::Pass => writeln!(f, "passed")?,
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_differing_line_is_reported() {
        assert_eq!(compare(b"A\nB\n", b"A\nB\n"), Verdict::Pass);
        assert_eq!(
            compare(b"A\nB\nC\n", b"A\nX\nC\n"),
            Verdict::Differs {
                line: 2,
                expected: Some("B\\n".to_owned()),
                actual: Some("X\\n".to_owned()),
            }
        );
        assert_eq!(
            compare(b"A\n", b"A\nB\n"),
            Verdict::Differs {
                line: 2,
                expected: None,
                actual: Some("B\\n".to_owned()),
            }
        );
        assert_eq!(
            compare(b"A\n", b"A"),
            Verdict::Differs {
                line: 1,
                expected: Some("A\\n".to_owned()),
                actual: Some("A".to_owned()),
            }
        );
    }

    #[test]
    fn the_pass_rate_is_the_mean_over_groups() {
        let outcome = |group: &str, verdict: Verdict| Outcome {
            session: Session {
                group: group.to_owned(),
                name: "s".to_owned(),
                input: PathBuf::new(),
                expected: PathBuf::new(),
            },
            verdict,
        };
        let mut outcomes = vec![outcome("small", Verdict::Broken("crash".to_owned()))];
        outcomes.extend((0..9).map(|_| outcome("large", Verdict::Pass)));
        let report = Report::new(&outcomes);
        assert!((report.mean_group_rate() - 0.5).abs() < 1e-9);
        assert!(!report.all_passed());
        assert!(report.to_string().contains("FAIL small/s: crash"));
    }
}
