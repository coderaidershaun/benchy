//! The engine program: reads protocol lines from standard input and writes events to standard output.

use std::io::{self, BufRead, BufWriter, Write};
use std::process::ExitCode;

use engine::Engine;

const USAGE: &str = "usage: engine < commands > events
Reads matching engine commands from standard input, one per line, and writes events to standard output.
";

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => match run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("engine: {error}");
                ExitCode::FAILURE
            }
        },
        [arg] if arg == "--help" => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprint!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn run() -> io::Result<()> {
    let mut input = io::stdin().lock();
    let mut output = BufWriter::with_capacity(1 << 16, io::stdout().lock());
    let mut engine = Engine::new();
    let mut line = Vec::new();
    let mut events = Vec::new();
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let without_lf = line.strip_suffix(b"\n").unwrap_or(&line);
        engine.handle_line(without_lf, &mut events);
        for event in events.drain(..) {
            writeln!(output, "{event}")?;
        }
    }
    output.flush()
}
