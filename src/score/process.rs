//! Runs the programs that scoring measures: an engine on one input, with its exit, output, time and peak memory.
//! Peak memory comes from the kernel's account of the finished child, so the measurement adds no sampling noise.

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Where an engine's input comes from.
pub enum Input<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

/// What happens to an engine's standard output.
pub enum Output<'a> {
    /// Keep all of it.
    Keep,
    /// Compare it byte for byte with this file and keep nothing.
    CompareWith(&'a Path),
    /// Write it to this file.
    WriteTo(&'a Path),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    Code(i32),
    Signal(i32),
    TimedOut,
}

#[derive(Debug, Clone)]
pub struct Finished {
    pub exit: Exit,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// Set when the output was compared with a file.
    pub matches: Option<bool>,
    pub elapsed: Duration,
    pub peak_rss_bytes: u64,
}

impl Finished {
    pub fn exited_cleanly(&self) -> bool {
        self.exit == Exit::Code(0)
    }

    /// A crash is a signal, a time-out, or a panic or unhandled exception the runtime reported.
    pub fn crashed(&self) -> bool {
        let stderr = String::from_utf8_lossy(&self.stderr);
        let panicked = [
            "panicked at",
            "panic:",
            "Unhandled exception",
            "fatal error:",
        ]
        .iter()
        .any(|marker| stderr.contains(marker));
        matches!(self.exit, Exit::Signal(_) | Exit::TimedOut) || panicked
    }
}

/// Runs `program` with `arguments` and the given input, waiting at most `timeout`.
///
/// # Errors
///
/// Fails when the program cannot be started or an input or output file cannot be opened.
pub fn run(
    program: &Path,
    arguments: &[&str],
    input: &Input,
    output: &Output,
    timeout: Duration,
) -> Result<Finished, String> {
    let stdin = match input {
        Input::Bytes(_) => Stdio::piped(),
        Input::File(path) => Stdio::from(
            File::open(path)
                .map_err(|error| format!("failed to open {}: {error}", path.display()))?,
        ),
    };
    let started = Instant::now();
    let mut child = Command::new(program)
        .args(arguments)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", program.display()))?;
    let pid = child.id();
    let feeder = match input {
        Input::Bytes(bytes) => child.stdin.take().map(|mut stdin| {
            let bytes = bytes.to_vec();
            // An engine may exit before it reads all its input; only its output and exit code count.
            thread::spawn(move || drop(stdin.write_all(&bytes)))
        }),
        Input::File(_) => None,
    };
    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut stderr = child.stderr.take().expect("stderr was piped");
    let sink = OutputSink::open(output)?;
    let reader = thread::spawn(move || sink.drain(&mut stdout));
    let error_reader = thread::spawn(move || {
        let mut text = Vec::new();
        drop(stderr.read_to_end(&mut text));
        text
    });
    let (status, peak_rss_bytes, elapsed) = wait(pid, started, timeout);
    // The child is reaped by `wait`; dropping the handle neither waits nor kills.
    drop(child);
    if let Some(feeder) = feeder {
        drop(feeder.join());
    }
    let (stdout, matches) = reader.join().expect("the output reader does not panic");
    let stderr = error_reader
        .join()
        .expect("the error reader does not panic");
    Ok(Finished {
        exit: status,
        stdout,
        stderr,
        matches,
        elapsed,
        peak_rss_bytes,
    })
}

enum OutputSink {
    Keep,
    Compare(File),
    Write(File),
}

impl OutputSink {
    fn open(output: &Output) -> Result<OutputSink, String> {
        let open_error = |path: &Path, error| format!("failed to open {}: {error}", path.display());
        Ok(match output {
            Output::Keep => OutputSink::Keep,
            Output::CompareWith(path) => {
                OutputSink::Compare(File::open(path).map_err(|e| open_error(path, e))?)
            }
            Output::WriteTo(path) => {
                OutputSink::Write(File::create(path).map_err(|e| open_error(path, e))?)
            }
        })
    }

    fn drain(self, from: &mut impl Read) -> (Vec<u8>, Option<bool>) {
        let mut buffer = vec![0u8; 1 << 16];
        let mut expected = vec![0u8; 1 << 16];
        let mut kept = Vec::new();
        let mut same = true;
        let mut writer = None;
        let mut reference = None;
        match self {
            OutputSink::Keep => {}
            OutputSink::Compare(file) => reference = Some(file),
            OutputSink::Write(file) => writer = Some(std::io::BufWriter::new(file)),
        }
        loop {
            let count = match from.read(&mut buffer) {
                Ok(0) | Err(_) => break,
                Ok(count) => count,
            };
            let chunk = &buffer[..count];
            if let Some(file) = reference.as_mut() {
                let wanted = &mut expected[..count];
                same = same && file.read_exact(wanted).is_ok() && wanted == chunk;
            } else if let Some(file) = writer.as_mut() {
                same = same && file.write_all(chunk).is_ok();
            } else {
                kept.extend_from_slice(chunk);
            }
        }
        match (reference, writer) {
            (Some(mut file), _) => {
                let at_end = file.read(&mut expected[..1]).is_ok_and(|count| count == 0);
                (kept, Some(same && at_end))
            }
            (None, Some(mut file)) => (kept, Some(same && file.flush().is_ok())),
            (None, None) => (kept, None),
        }
    }
}

/// Waits for the child with `wait4`, which also reports its peak resident memory, and kills it at the timeout.
fn wait(pid: u32, started: Instant, timeout: Duration) -> (Exit, u64, Duration) {
    let pid = i32::try_from(pid).expect("process ids fit in i32");
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let mut status = 0;
        // SAFETY: `usage` is a plain C struct that wait4 fills in; zeroed is a valid value for it.
        let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
        // SAFETY: `pid` is our own child, and both pointers are valid for the duration of the call.
        let result = unsafe { libc::wait4(pid, &raw mut status, 0, &raw mut usage) };
        sender
            .send((result, status, usage.ru_maxrss, started.elapsed()))
            .ok();
    });
    let mut timed_out = false;
    let (result, status, max_rss, elapsed) = match receiver.recv_timeout(timeout) {
        Ok(done) => done,
        Err(_) => {
            timed_out = true;
            // SAFETY: sending a signal has no memory effects; the child is not reaped yet, so the pid is still ours.
            unsafe { libc::kill(pid, libc::SIGKILL) };
            receiver.recv().expect("the waiting thread always reports")
        }
    };
    let exit = if timed_out || result < 0 {
        Exit::TimedOut
    } else if libc::WIFSIGNALED(status) {
        Exit::Signal(libc::WTERMSIG(status))
    } else {
        Exit::Code(libc::WEXITSTATUS(status))
    };
    (exit, rss_bytes(max_rss), elapsed)
}

#[cfg(target_os = "macos")]
fn rss_bytes(max_rss: libc::c_long) -> u64 {
    u64::try_from(max_rss).unwrap_or(0)
}

#[cfg(not(target_os = "macos"))]
fn rss_bytes(max_rss: libc::c_long) -> u64 {
    // Linux reports kilobytes.
    u64::try_from(max_rss).unwrap_or(0) * 1024
}

/// The outcome of a build or analysis command.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

impl CommandOutput {
    pub fn text(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr)
    }
}

/// Runs a shell command line in `dir` and keeps its output. A program that cannot start counts as a failure.
pub fn shell(dir: &Path, line: &str, log: &mut Log) -> CommandOutput {
    let started = Instant::now();
    let output = Command::new("sh")
        .arg("-c")
        .arg(line)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output();
    let result = match output {
        Ok(output) => CommandOutput {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        },
        Err(error) => CommandOutput {
            success: false,
            stdout: String::new(),
            stderr: format!("failed to start sh: {error}"),
        },
    };
    log.command(dir, line, &result, started.elapsed());
    result
}

/// The score log: every command scoring ran, with its outcome and the end of its output.
#[derive(Debug, Default)]
pub struct Log {
    pub text: String,
}

impl Log {
    pub fn line(&mut self, text: &str) {
        self.text.push_str(text);
        self.text.push('\n');
        eprintln!("{text}");
    }

    fn command(&mut self, dir: &Path, line: &str, output: &CommandOutput, elapsed: Duration) {
        self.line(&format!(
            "$ (cd {} && {line})  -> {} in {:.1}s",
            dir.display(),
            if output.success { "ok" } else { "FAILED" },
            elapsed.as_secs_f64()
        ));
        let text = output.text();
        let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
        for shown in &lines[lines.len().saturating_sub(15)..] {
            self.text.push_str("    ");
            self.text.push_str(shown);
            self.text.push('\n');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_reports_output_exit_code_and_memory() {
        let finished = run(
            Path::new("/bin/sh"),
            &["-c", "cat; exit 3"],
            &Input::Bytes(b"hello"),
            &Output::Keep,
            Duration::from_secs(10),
        )
        .expect("sh starts");

        assert_eq!(finished.stdout, b"hello");
        assert_eq!(finished.exit, Exit::Code(3));
        assert!(finished.peak_rss_bytes > 0);
        assert!(!finished.crashed());
    }

    #[test]
    fn a_run_past_its_timeout_is_killed() {
        let finished = run(
            Path::new("/bin/sh"),
            &["-c", "sleep 30"],
            &Input::Bytes(b""),
            &Output::Keep,
            Duration::from_millis(200),
        )
        .expect("sh starts");

        assert_eq!(finished.exit, Exit::TimedOut);
        assert!(finished.crashed());
    }

    #[test]
    fn output_is_compared_with_a_file_byte_for_byte() {
        let dir = tempfile::tempdir().expect("temp dir");
        let expected = dir.path().join("expected");
        std::fs::write(&expected, b"abc\n").expect("write");
        let compare = |input: &[u8]| {
            run(
                Path::new("/bin/cat"),
                &[],
                &Input::Bytes(input),
                &Output::CompareWith(&expected),
                Duration::from_secs(10),
            )
            .expect("cat starts")
            .matches
        };

        assert_eq!(compare(b"abc\n"), Some(true));
        assert_eq!(compare(b"abc"), Some(false));
        assert_eq!(compare(b"abc\nd"), Some(false));
    }
}
