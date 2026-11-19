//! The words of the engine's wire protocol, as `task/spec/protocol.md` defines them.
//! Generators and the conformance runner use these names, so a test keeps them equal to the document.

/// An input command, by its code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommandCode {
    Symbol,
    Limit,
    Market,
    Iceberg,
    Cancel,
    Amend,
    Top,
    Depth,
}

impl CommandCode {
    pub const ALL: [CommandCode; 8] = [
        CommandCode::Symbol,
        CommandCode::Limit,
        CommandCode::Market,
        CommandCode::Iceberg,
        CommandCode::Cancel,
        CommandCode::Amend,
        CommandCode::Top,
        CommandCode::Depth,
    ];

    pub fn code(self) -> &'static str {
        match self {
            CommandCode::Symbol => "S",
            CommandCode::Limit => "N",
            CommandCode::Market => "M",
            CommandCode::Iceberg => "I",
            CommandCode::Cancel => "C",
            CommandCode::Amend => "A",
            CommandCode::Top => "Q",
            CommandCode::Depth => "D",
        }
    }

    /// The field names of the command line, its code first.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            CommandCode::Symbol => &["S", "symbol", "tick", "lot"],
            CommandCode::Limit => &["N", "id", "symbol", "side", "price", "qty", "tif", "owner"],
            CommandCode::Market => &["M", "id", "symbol", "side", "qty", "owner"],
            CommandCode::Iceberg => &[
                "I", "id", "symbol", "side", "price", "qty", "display", "owner",
            ],
            CommandCode::Cancel => &["C", "id"],
            CommandCode::Amend => &["A", "id", "price", "qty"],
            CommandCode::Top => &["Q", "symbol"],
            CommandCode::Depth => &["D", "symbol", "levels"],
        }
    }
}

/// An output event, by its tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventKind {
    Symbol,
    Accept,
    Reject,
    Trade,
    Out,
    Amend,
    Top,
    Depth,
    Level,
}

impl EventKind {
    pub const ALL: [EventKind; 9] = [
        EventKind::Symbol,
        EventKind::Accept,
        EventKind::Reject,
        EventKind::Trade,
        EventKind::Out,
        EventKind::Amend,
        EventKind::Top,
        EventKind::Depth,
        EventKind::Level,
    ];

    pub fn tag(self) -> &'static str {
        match self {
            EventKind::Symbol => "SYMBOL",
            EventKind::Accept => "ACCEPT",
            EventKind::Reject => "REJECT",
            EventKind::Trade => "TRADE",
            EventKind::Out => "OUT",
            EventKind::Amend => "AMEND",
            EventKind::Top => "TOP",
            EventKind::Depth => "DEPTH",
            EventKind::Level => "LEVEL",
        }
    }

    /// The field names of the event line, its tag first.
    pub fn fields(self) -> &'static [&'static str] {
        match self {
            EventKind::Symbol => &["SYMBOL", "symbol"],
            EventKind::Accept => &["ACCEPT", "id"],
            EventKind::Reject => &["REJECT", "line", "code"],
            EventKind::Trade => &["TRADE", "symbol", "price", "qty", "taker", "maker"],
            EventKind::Out => &["OUT", "id", "qty", "reason"],
            EventKind::Amend => &["AMEND", "id", "price", "qty"],
            EventKind::Top => &[
                "TOP",
                "symbol",
                "bid_price",
                "bid_qty",
                "ask_price",
                "ask_qty",
            ],
            EventKind::Depth => &["DEPTH", "symbol", "bid_levels", "ask_levels"],
            EventKind::Level => &["LEVEL", "side", "price", "qty", "orders"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RejectCode {
    UnknownCommand,
    Malformed,
    UnknownSymbol,
    DuplicateSymbol,
    DuplicateOrderId,
    UnknownOrder,
    InvalidPrice,
    InvalidQty,
    WouldTrade,
}

impl RejectCode {
    pub const ALL: [RejectCode; 9] = [
        RejectCode::UnknownCommand,
        RejectCode::Malformed,
        RejectCode::UnknownSymbol,
        RejectCode::DuplicateSymbol,
        RejectCode::DuplicateOrderId,
        RejectCode::UnknownOrder,
        RejectCode::InvalidPrice,
        RejectCode::InvalidQty,
        RejectCode::WouldTrade,
    ];

    pub fn text(self) -> &'static str {
        match self {
            RejectCode::UnknownCommand => "UNKNOWN_COMMAND",
            RejectCode::Malformed => "MALFORMED",
            RejectCode::UnknownSymbol => "UNKNOWN_SYMBOL",
            RejectCode::DuplicateSymbol => "DUPLICATE_SYMBOL",
            RejectCode::DuplicateOrderId => "DUPLICATE_ORDER_ID",
            RejectCode::UnknownOrder => "UNKNOWN_ORDER",
            RejectCode::InvalidPrice => "INVALID_PRICE",
            RejectCode::InvalidQty => "INVALID_QTY",
            RejectCode::WouldTrade => "WOULD_TRADE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OutReason {
    Cancelled,
    Unfilled,
    SelfTrade,
}

impl OutReason {
    pub const ALL: [OutReason; 3] = [
        OutReason::Cancelled,
        OutReason::Unfilled,
        OutReason::SelfTrade,
    ];

    pub fn text(self) -> &'static str {
        match self {
            OutReason::Cancelled => "CANCELLED",
            OutReason::Unfilled => "UNFILLED",
            OutReason::SelfTrade => "SELF_TRADE",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeInForce {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}

impl TimeInForce {
    pub const ALL: [TimeInForce; 4] = [
        TimeInForce::GoodTillCancel,
        TimeInForce::ImmediateOrCancel,
        TimeInForce::FillOrKill,
        TimeInForce::PostOnly,
    ];

    pub fn text(self) -> &'static str {
        match self {
            TimeInForce::GoodTillCancel => "GTC",
            TimeInForce::ImmediateOrCancel => "IOC",
            TimeInForce::FillOrKill => "FOK",
            TimeInForce::PostOnly => "POST",
        }
    }
}

/// Longest line, in bytes after the CR is removed, that the engine parses instead of rejecting.
pub const MAX_LINE_BYTES: usize = 256;
/// Upper bound of every integer field except `id`.
pub const MAX_VALUE: u64 = 1_000_000_000;
/// Upper bound of the `id` field.
pub const MAX_ORDER_ID: u64 = 999_999_999_999_999_999;
/// Most lines in one session.
pub const MAX_SESSION_LINES: u64 = 10_000_000;

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    use super::*;

    fn task_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("task")
            .join(relative)
    }

    fn protocol_document() -> String {
        std::fs::read_to_string(task_path("spec/protocol.md")).expect("protocol.md is readable")
    }

    /// The cells of each row of the Markdown table whose header line is `header`, with backticks removed.
    fn table_rows(document: &str, header: &str) -> Vec<Vec<String>> {
        let mut lines = document.lines().skip_while(|line| *line != header);
        assert!(
            lines.next().is_some(),
            "protocol.md has no table with header {header}"
        );
        lines
            .skip(1)
            .take_while(|line| line.starts_with('|'))
            .map(|line| {
                line.trim_matches('|')
                    .split('|')
                    .map(|cell| cell.trim().replace('`', ""))
                    .collect()
            })
            .collect()
    }

    fn first_cells(document: &str, header: &str) -> Vec<String> {
        table_rows(document, header)
            .into_iter()
            .map(|row| row[0].clone())
            .collect()
    }

    /// Every example as a pair of input text and expected output text.
    fn examples() -> Vec<(PathBuf, String, String)> {
        let mut inputs: Vec<PathBuf> = std::fs::read_dir(task_path("examples"))
            .expect("examples folder is readable")
            .map(|entry| entry.expect("directory entry").path())
            .filter(|path| path.extension().is_some_and(|e| e == "in"))
            .collect();
        inputs.sort();
        inputs
            .into_iter()
            .map(|input| {
                let output = input.with_extension("out");
                let read = |path: &Path| {
                    std::fs::read_to_string(path)
                        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
                };
                (input.clone(), read(&input), read(&output))
            })
            .collect()
    }

    fn set<'a>(items: impl IntoIterator<Item = &'a str>) -> BTreeSet<String> {
        items.into_iter().map(String::from).collect()
    }

    #[test]
    fn protocol_document_defines_exactly_these_commands() {
        let rows = table_rows(&protocol_document(), "| Code | Fields, in order | Does |");
        let documented: Vec<(String, String)> = rows
            .into_iter()
            .map(|row| (row[0].clone(), row[1].clone()))
            .collect();
        let expected: Vec<(String, String)> = CommandCode::ALL
            .iter()
            .map(|c| (c.code().to_string(), c.fields().join(",")))
            .collect();
        assert_eq!(documented, expected);
    }

    #[test]
    fn protocol_document_defines_exactly_these_events() {
        let rows = table_rows(
            &protocol_document(),
            "| Event | Fields, in order | Meaning |",
        );
        let documented: Vec<(String, String)> = rows
            .into_iter()
            .map(|row| (row[0].clone(), row[1].clone()))
            .collect();
        let expected: Vec<(String, String)> = EventKind::ALL
            .iter()
            .map(|e| (e.tag().to_string(), e.fields().join(",")))
            .collect();
        assert_eq!(documented, expected);
    }

    #[test]
    fn protocol_document_defines_exactly_these_codes_reasons_and_limits() {
        let document = protocol_document();
        let codes: Vec<&str> = RejectCode::ALL.iter().map(|c| c.text()).collect();
        assert_eq!(first_cells(&document, "| Code | Meaning |"), codes);
        let reasons: Vec<&str> = OutReason::ALL.iter().map(|r| r.text()).collect();
        assert_eq!(first_cells(&document, "| Reason | When |"), reasons);
        let tif_row = table_rows(&document, "| Type | Rule | Range |")
            .into_iter()
            .find(|row| row[0] == "tif")
            .expect("the field types table has a tif row");
        for tif in TimeInForce::ALL {
            assert!(
                tif_row[1].contains(tif.text()),
                "tif row lacks {}",
                tif.text()
            );
        }
        assert!(document.contains(&format!("longer than {MAX_LINE_BYTES} bytes")));
        assert!(document.contains("1 to 1,000,000,000"));
        assert_eq!(MAX_VALUE, 1_000_000_000);
        assert!(document.contains("1 to 999,999,999,999,999,999"));
        assert_eq!(MAX_ORDER_ID, 999_999_999_999_999_999);
        assert!(document.contains("at most 10,000,000 lines"));
        assert_eq!(MAX_SESSION_LINES, 10_000_000);
    }

    #[test]
    fn examples_use_every_command_event_code_reason_and_tif() {
        let mut commands = BTreeSet::new();
        let mut events = BTreeSet::new();
        let mut codes = BTreeSet::new();
        let mut reasons = BTreeSet::new();
        let mut tifs = BTreeSet::new();
        for (_, input, output) in examples() {
            for fields in input
                .lines()
                .map(|line| line.split(',').collect::<Vec<_>>())
            {
                commands.insert(fields[0].to_string());
                if fields[0] == "N" && fields.len() == 8 {
                    tifs.insert(fields[6].to_string());
                }
            }
            for fields in output
                .lines()
                .map(|line| line.split(',').collect::<Vec<_>>())
            {
                events.insert(fields[0].to_string());
                match fields[0] {
                    "REJECT" => codes.insert(fields[2].to_string()),
                    "OUT" => reasons.insert(fields[3].to_string()),
                    _ => false,
                };
            }
        }
        let missing = |want: BTreeSet<String>, have: &BTreeSet<String>| -> Vec<String> {
            want.difference(have).cloned().collect()
        };
        assert_eq!(
            missing(set(CommandCode::ALL.map(CommandCode::code)), &commands),
            Vec::<String>::new()
        );
        assert_eq!(
            missing(set(EventKind::ALL.map(EventKind::tag)), &events),
            Vec::<String>::new()
        );
        assert_eq!(
            missing(set(RejectCode::ALL.map(RejectCode::text)), &codes),
            Vec::<String>::new()
        );
        assert_eq!(
            missing(set(OutReason::ALL.map(OutReason::text)), &reasons),
            Vec::<String>::new()
        );
        assert_eq!(
            missing(set(TimeInForce::ALL.map(TimeInForce::text)), &tifs),
            Vec::<String>::new()
        );
    }

    #[test]
    fn example_outputs_have_the_documented_shape() {
        let semantics = std::fs::read_to_string(task_path("spec/semantics.md"))
            .expect("semantics.md is readable");
        let worked = semantics
            .lines()
            .filter(|line| line.chars().next().is_some_and(|c| c.is_ascii_uppercase()))
            .filter(|line| line.contains(',') && !line.contains(' '));
        let outputs: Vec<String> = examples()
            .into_iter()
            .map(|(_, _, output)| output)
            .collect();
        let lines = outputs.iter().flat_map(|o| o.lines()).chain(worked);
        for line in lines {
            let fields: Vec<&str> = line.split(',').collect();
            let kind = EventKind::ALL
                .into_iter()
                .find(|e| e.tag() == fields[0])
                .unwrap_or_else(|| panic!("{line:?} is not an event"));
            assert_eq!(
                fields.len(),
                kind.fields().len(),
                "{line:?} has the wrong field count"
            );
            assert!(
                fields.iter().all(|f| !f.is_empty()),
                "{line:?} has an empty field"
            );
        }
    }

    #[test]
    fn example_files_are_byte_stable() {
        let all = examples();
        assert!(!all.is_empty(), "task/examples holds no example");
        for (path, input, output) in &all {
            for (name, text) in [("input", input), ("output", output)] {
                let shown = path.display();
                assert!(
                    text.ends_with('\n') && !text.ends_with("\n\n"),
                    "{name} of {shown} must end in exactly one newline"
                );
                assert!(!text.contains('\r'), "{name} of {shown} holds a CR");
                assert!(
                    text.lines()
                        .all(|line| !line.ends_with(' ') && !line.ends_with('\t')),
                    "{name} of {shown} has trailing whitespace"
                );
            }
        }
        let outputs = std::fs::read_dir(task_path("examples"))
            .expect("examples folder is readable")
            .filter(|entry| {
                entry
                    .as_ref()
                    .is_ok_and(|e| e.path().extension().is_some_and(|x| x == "out"))
            })
            .count();
        assert_eq!(outputs, all.len(), "every .out file needs its .in file");
    }
}
