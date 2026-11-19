//! Category B: a corpus of malformed and hostile input, generated at score time, with the Rust reference as the oracle.
//! The line-ending cases live here because session files cannot hold them: the commit hooks rewrite CR and trailing spaces.

use std::collections::BTreeSet;

use super::correctness::ratio;
use super::process::{self, Input, Log, Output};
use super::{Context, Metrics, RUN_TIMEOUT};
use crate::r#gen::{Feature, stream};

/// One hostile input of the corpus.
pub struct Hostile {
    pub name: String,
    pub bytes: Vec<u8>,
}

fn hostile(name: &str, bytes: impl Into<Vec<u8>>) -> Hostile {
    Hostile {
        name: name.to_string(),
        bytes: bytes.into(),
    }
}

const BOOK: &str = "S,ACME,5,10\nN,1,ACME,B,100,50,GTC,AL\nN,2,ACME,S,105,30,GTC,BO\n";

/// The corpus, the same on every run.
pub fn corpus() -> Vec<Hostile> {
    let session = stream(
        &[
            Feature::Market,
            Feature::Cancel,
            Feature::Queries,
            Feature::Amend,
        ],
        99,
        200,
    );
    let mut inputs = vec![
        hostile("crlf", session.replace('\n', "\r\n")),
        hostile("no-final-newline", session.trim_end_matches('\n')),
        hostile(
            "trailing-cr-last-line",
            format!("{}\r", session.trim_end_matches('\n')),
        ),
        hostile("trailing-spaces", session.replace('\n', " \n")),
        hostile("cr-cr-lf", format!("{BOOK}Q,ACME\r\r\nQ,ACME\r\n")),
        hostile(
            "bare-cr-inside",
            format!("{BOOK}Q,AC\rME\nN,3,ACME,B,1\r00,10,GTC,AL\n"),
        ),
        hostile("empty", ""),
        hostile("only-newlines", "\n".repeat(10_000)),
        hostile("only-crlf", "\r\n".repeat(10_000)),
        hostile(
            "line-of-257-bytes",
            format!("{BOOK}Q,{}\n", "A".repeat(255)),
        ),
        hostile(
            "line-of-256-bytes",
            format!("{BOOK}Q,{}\nQ,{}\r\n", "A".repeat(254), "A".repeat(254)),
        ),
        hostile(
            "megabyte-line",
            format!("{BOOK}{}\nQ,ACME\n", "N".repeat(1 << 20)),
        ),
        hostile("megabyte-line-no-newline", "Z".repeat(1 << 20)),
        hostile(
            "nul-bytes",
            [BOOK.as_bytes(), b"Q,AC\0ME\n\0\n\0\0\0\nQ,ACME\n"].concat(),
        ),
        hostile("binary-noise", noise(65_536, 17)),
        hostile(
            "invalid-utf8",
            [
                BOOK.as_bytes(),
                b"Q,\xff\xfe\nS,\xc3\xa9T,1,1\nN,3,ACME,B,100,10,GTC,\xe2\x82\xac\n",
            ]
            .concat(),
        ),
        hostile("numbers", numbers()),
        hostile("fields", fields()),
        hostile("commands", commands()),
        hostile("ids", ids()),
        hostile("many-symbols", many_symbols(20_000)),
        hostile("deep-book", deep_book(200_000)),
        hostile("sweep-deep-book", sweep_deep_book(100_000)),
    ];
    for (index, seed) in [3_u64, 5, 8, 13].iter().enumerate() {
        let features = [
            Feature::Parsing,
            Feature::Rejections,
            Feature::Cancel,
            Feature::TickLot,
            Feature::Amend,
        ];
        inputs.push(hostile(
            &format!("parsing-stream-{index}"),
            stream(&features, *seed, 2_000),
        ));
    }
    inputs
}

fn noise(length: usize, seed: u64) -> Vec<u8> {
    let mut state = seed;
    (0..length)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

fn numbers() -> String {
    let mut text = BOOK.to_string();
    for value in [
        "0",
        "-5",
        "+5",
        "5.0",
        "1e3",
        " 5",
        "5 ",
        "0x10",
        "007",
        "000000000000000105",
        "0000000000000000105",
        "999999999999999999",
        "9999999999999999999",
        "18446744073709551616",
        "1000000000",
        "1000000005",
        "99999999999999999999999999999999999999",
    ] {
        text.push_str(&format!("N,3,ACME,B,{value},10,GTC,AL\n"));
        text.push_str(&format!("N,4,ACME,S,105,{value},IOC,AL\n"));
        text.push_str(&format!("D,ACME,{value}\n"));
        text.push_str(&format!("N,{value},ACME,B,5,10,GTC,AL\n"));
        text.push_str(&format!("A,1,{value},50\n"));
        text.push_str(&format!("I,9,ACME,S,500,100,{value},ZZ\n"));
    }
    text.push_str("Q,ACME\n");
    text
}

fn fields() -> String {
    let mut text = BOOK.to_string();
    for line in [
        "N,3,ACME,B,100,10,GTC,AL,",
        "N,3,ACME,B,100,10,GTC",
        ",",
        ",,,,,,,",
        "Q,",
        "Q",
        "Q,ACME,",
        "Q,acme",
        "Q,ABCDEFGHI",
        "Q,ABCDEFGH",
        "Q, ACME",
        "S,ACME",
        "S,NEW,0,1",
        "S,NEW,1,0",
        "S,NEW,1,1,1",
        "N,3,ACME,X,100,10,GTC,AL",
        "N,3,ACME,B,100,10,DAY,AL",
        "N,3,ACME,B,100,10,gtc,AL",
        "N,3,ACME,b,100,10,GTC,AL",
        "I,3,ACME,B,100,10,20,AL",
        "M,3,ACME,B,10",
        "C",
        "C,",
        "C,1,1",
        "A,1,100",
        "D,ACME",
        "D,ACME,0",
    ] {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str(&",".repeat(255));
    text.push('\n');
    text.push_str("D,ACME,1000000000\n");
    text
}

fn commands() -> String {
    let mut text = BOOK.to_string();
    for line in [
        "X,1",
        "q,ACME",
        "n,3,ACME,B,100,10,GTC,AL",
        "NN,3",
        "S ,ACME,1,1",
        "QQ,ACME",
        "\t",
        " ",
        "#",
        "--help",
    ] {
        text.push_str(line);
        text.push('\n');
    }
    text
}

fn ids() -> String {
    let mut text = BOOK.to_string();
    text.push_str("N,1,ACME,B,100,10,GTC,AL\nC,999\nA,999,100,10\nC,2\nC,2\nA,2,105,10\n");
    text.push_str(
        "N,999999999999999999,ACME,B,100,10,GTC,AL\nC,999999999999999999\nC,1000000000000000000\n",
    );
    text.push_str(
        "M,5,ACME,S,1000000000,AL\nM,5,ACME,S,10,AL\nN,6,ACME,B,105,1000000000,FOK,AL\nQ,ACME\n",
    );
    text
}

fn many_symbols(count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("S,X{index},1,1\nQ,X{index}\n"));
    }
    text.push_str("S,X0,1,1\nQ,X0\n");
    text
}

fn deep_book(count: usize) -> String {
    let mut text = "S,ACME,1,1\n".to_string();
    for index in 1..=count {
        text.push_str(&format!(
            "N,{index},ACME,B,{},1000000000,GTC,AL\n",
            1 + index
        ));
    }
    text.push_str("D,ACME,1000000000\nQ,ACME\n");
    text
}

fn sweep_deep_book(count: usize) -> String {
    let mut text = "S,ACME,1,1\n".to_string();
    for index in 1..=count {
        text.push_str(&format!(
            "N,{index},ACME,S,{},1,GTC,M{}\n",
            1000 + index,
            index % 1000
        ));
    }
    text.push_str(&format!(
        "M,{},ACME,B,1000000000,TAKER\nD,ACME,10\n",
        count + 1
    ));
    text
}

/// The `REJECT` events of an output: the rejected line numbers with their codes.
fn rejections(output: &[u8]) -> BTreeSet<String> {
    String::from_utf8_lossy(output)
        .lines()
        .filter(|line| line.starts_with("REJECT,"))
        .map(str::to_string)
        .collect()
}

pub fn measure(context: &Context, metrics: &mut Metrics, log: &mut Log) {
    let rubric = context.rubric;
    let corpus = corpus();
    let limit_bytes = u64::from(rubric.measurement.memory_limit_mb) * 1024 * 1024;
    let mut survived = 0;
    let mut clean = 0;
    let mut expected_total = 0;
    let mut matched = 0;
    let mut peak = 0;
    for input in &corpus {
        let run = |engine| {
            process::run(
                engine,
                &[],
                &Input::Bytes(&input.bytes),
                &Output::Keep,
                RUN_TIMEOUT,
            )
        };
        let reference = run(&context.rust_reference_engine);
        let candidate = run(&context.engine);
        let (Ok(reference), Ok(candidate)) = (reference, candidate) else {
            log.line(&format!(
                "hostile input {}: an engine could not start",
                input.name
            ));
            continue;
        };
        let expected = rejections(&reference.stdout);
        let actual = rejections(&candidate.stdout);
        let hits = expected.intersection(&actual).count();
        expected_total += expected.len();
        matched += hits;
        survived += usize::from(!candidate.crashed());
        clean += usize::from(candidate.exited_cleanly());
        peak = peak.max(candidate.peak_rss_bytes);
        log.line(&format!(
            "{} hostile {}: exit {:?}, crashed {}, rejections {hits}/{}, peak {} MB",
            context.language.as_str(),
            input.name,
            candidate.exit,
            candidate.crashed(),
            expected.len(),
            candidate.peak_rss_bytes / (1024 * 1024)
        ));
    }
    let count = corpus.len();
    metrics.add(rubric, "B.no_crash", Some(ratio(survived, count)));
    metrics.add(rubric, "B.rejections", Some(ratio(matched, expected_total)));
    metrics.add(rubric, "B.exit_code", Some(ratio(clean, count)));
    metrics.add(
        rubric,
        "B.memory_bound",
        Some(if peak < limit_bytes { 1.0 } else { 0.0 }),
    );
    log.line(&format!(
        "{} robustness: peak memory on the hostile corpus {} MB",
        context.language.as_str(),
        peak / (1024 * 1024)
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_corpus_holds_the_line_endings_that_cannot_be_committed() {
        let corpus = corpus();
        let find = |name: &str| {
            corpus
                .iter()
                .find(|h| h.name == name)
                .map(|h| h.bytes.clone())
        };

        assert!(find("crlf").is_some_and(|b| b.windows(2).any(|w| w == b"\r\n")));
        assert!(find("no-final-newline").is_some_and(|b| b.last() != Some(&b'\n')));
        assert!(find("trailing-cr-last-line").is_some_and(|b| b.last() == Some(&b'\r')));
        assert!(find("trailing-spaces").is_some_and(|b| b.windows(2).any(|w| w == b" \n")));
    }

    #[test]
    fn the_corpus_is_the_same_every_time() {
        let first: Vec<Vec<u8>> = corpus().into_iter().map(|h| h.bytes).collect();
        let second: Vec<Vec<u8>> = corpus().into_iter().map(|h| h.bytes).collect();

        assert_eq!(first, second);
    }
}
