//! End-to-end tests of the engine program: bytes in on standard input, bytes out on standard output.

use std::io::Write;
use std::process::{Command, Output, Stdio};

fn run(args: &[&str], input: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_engine"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the engine program starts");
    child
        .stdin
        .take()
        .expect("stdin is piped")
        .write_all(input)
        .expect("the engine reads its input");
    child.wait_with_output().expect("the engine exits")
}

fn session(input: &str) -> String {
    let output = run(&[], input.as_bytes());
    assert_eq!(output.status.code(), Some(0));
    String::from_utf8(output.stdout).expect("output is ASCII")
}

#[test]
fn price_time_priority_session() {
    let input = "S,ACME,1,1
N,1,ACME,S,101,10,GTC,AL
N,2,ACME,S,100,10,GTC,BO
N,3,ACME,S,100,10,GTC,CY
N,4,ACME,B,101,25,GTC,DI
Q,ACME
";
    let expected = "SYMBOL,ACME
ACCEPT,1
ACCEPT,2
ACCEPT,3
ACCEPT,4
TRADE,ACME,100,10,4,2
TRADE,ACME,100,10,4,3
TRADE,ACME,101,5,4,1
TOP,ACME,0,0,101,5
";
    assert_eq!(session(input), expected);
}

#[test]
fn iceberg_refills_behind_the_queue_and_cancels_whole() {
    let input = "S,ACME,1,1
I,1,ACME,S,100,30,10,AL
N,2,ACME,S,100,5,GTC,BO
N,3,ACME,B,100,22,GTC,CY
D,ACME,5
C,1
";
    let expected = "SYMBOL,ACME
ACCEPT,1
ACCEPT,2
ACCEPT,3
TRADE,ACME,100,10,3,1
TRADE,ACME,100,5,3,2
TRADE,ACME,100,7,3,1
DEPTH,ACME,0,1
LEVEL,S,100,3,1
OUT,1,13,CANCELLED
";
    assert_eq!(session(input), expected);
}

#[test]
fn crlf_lines_and_a_last_line_without_lf_are_read() {
    assert_eq!(
        session("S,ACME,1,1\r\nQ,ACME\r"),
        "SYMBOL,ACME\nTOP,ACME,0,0,0,0\n"
    );
}

#[test]
fn only_one_cr_is_removed_and_empty_lines_count() {
    assert_eq!(
        session("\n\nS,ACME,1,1\r\r\nS,ACME,1,1\n"),
        "REJECT,1,UNKNOWN_COMMAND\nREJECT,2,UNKNOWN_COMMAND\nREJECT,3,MALFORMED\nSYMBOL,ACME\n"
    );
}

#[test]
fn bytes_outside_ascii_are_malformed() {
    let output = run(&[], b"Q,AC\xC3\x89\n\xFF\n");
    assert_eq!(
        output.stdout,
        b"REJECT,1,MALFORMED\nREJECT,2,UNKNOWN_COMMAND\n"
    );
}

#[test]
fn empty_input_writes_nothing() {
    assert_eq!(session(""), "");
}

#[test]
fn help_goes_to_stdout_and_other_arguments_exit_2() {
    let help = run(&["--help"], b"");
    assert_eq!(help.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&help.stdout).starts_with("usage: engine"));

    let wrong = run(&["--verbose"], b"S,ACME,1,1\n");
    assert_eq!(wrong.status.code(), Some(2));
    assert!(wrong.stdout.is_empty());
    assert!(String::from_utf8_lossy(&wrong.stderr).starts_with("usage: engine"));
}
