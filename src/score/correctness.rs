//! Category A: golden sessions by feature group, differential fuzz against the Rust reference, and determinism and book invariants.
//! The invariant checks follow the engine's own events with a small book model, so they hold for any correct engine.

use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::process::Log;
use super::{Context, Metrics};
use crate::conform;
use crate::r#gen::{Feature, FuzzConfig, stream};
use benchy::contract::GroupRate;

const SESSION_TIMEOUT: Duration = Duration::from_secs(60);
const INVARIANT_SEED: u64 = 7_340_017;
const INVARIANT_SESSIONS: u64 = 120;
const INVARIANT_LINES: usize = 150;
/// No icebergs, amends or self-trade prevention, so price-time priority is strict and every fill is visible.
const INVARIANT_FEATURES: [Feature; 7] = [
    Feature::Market,
    Feature::Ioc,
    Feature::Fok,
    Feature::PostOnly,
    Feature::Cancel,
    Feature::Queries,
    Feature::Symbols,
];

/// The golden sessions an engine passed: the mean rate over feature groups, and the count of each group.
#[derive(Debug, Default)]
pub struct Golden {
    pub rate: f64,
    pub groups: Vec<GroupRate>,
}

/// Measures category A and returns the golden result.
pub fn measure(context: &Context, metrics: &mut Metrics, log: &mut Log) -> Golden {
    let rubric = context.rubric;
    let (golden, groups) = match golden_rate(context, log) {
        Some((rate, groups)) => (Some(rate), groups),
        None => (None, Vec::new()),
    };
    metrics.add(rubric, "A.golden", golden);
    let fuzz = fuzz_rate(context, log);
    metrics.add(rubric, "A.fuzz", fuzz);
    let invariants = invariant_rate(&context.engine);
    log.line(&format!(
        "{} invariants: {invariants:.4}",
        context.language.as_str()
    ));
    metrics.add(rubric, "A.invariants", Some(invariants));
    Golden {
        rate: golden.unwrap_or(0.0),
        groups,
    }
}

fn golden_rate(context: &Context, log: &mut Log) -> Option<(f64, Vec<GroupRate>)> {
    let dirs = [context.benchy.join("validation/scenarios")];
    let sessions = match conform::discover(&dirs) {
        Ok(sessions) => sessions,
        Err(error) => {
            log.line(&format!("golden sessions could not be read: {error}"));
            return None;
        }
    };
    let outcomes = conform::run_sessions(&context.engine, &sessions, SESSION_TIMEOUT);
    let report = conform::Report::new(&outcomes);
    log.line(&format!(
        "{} golden sessions:\n{report}",
        context.language.as_str()
    ));
    let groups = report
        .groups
        .iter()
        .map(|(group, count)| GroupRate {
            group: group.clone(),
            passed: u32::try_from(count.passed).unwrap_or(u32::MAX),
            total: u32::try_from(count.total).unwrap_or(u32::MAX),
        })
        .collect();
    Some((report.mean_group_rate(), groups))
}

fn fuzz_rate(context: &Context, log: &mut Log) -> Option<f64> {
    let path = context.benchy.join("validation/workloads/fuzz.toml");
    let config: FuzzConfig = match std::fs::read_to_string(&path)
        .map_err(|error| error.to_string())
        .and_then(|text| toml::from_str(&text).map_err(|error| error.to_string()))
    {
        Ok(config) => config,
        Err(error) => {
            log.line(&format!("{} could not be read: {error}", path.display()));
            return None;
        }
    };
    let streams: Vec<Vec<u8>> = config
        .sessions()
        .into_iter()
        .map(|session| session.input.into_bytes())
        .collect();
    let reference = context.rust_reference_engine.clone();
    let candidate = context.engine.clone();
    let same = super::parallel_map(&streams, |input| {
        let expected = conform::run_engine(&reference, input, SESSION_TIMEOUT);
        let actual = conform::run_engine(&candidate, input, SESSION_TIMEOUT);
        matches!((expected, actual), (Ok(e), Ok(a)) if e == a)
    });
    let identical = same.iter().filter(|&&same| same).count();
    log.line(&format!(
        "{} fuzz: {identical} of {} streams identical to the Rust reference",
        context.language.as_str(),
        streams.len()
    ));
    Some(ratio(identical, streams.len()))
}

pub fn ratio(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64
}

/// Runs the invariant sessions and returns the share of checks passed: for each session, the
/// same output twice, every depth snapshot uncrossed and ordered, and the events consistent with a book.
pub fn invariant_rate(engine: &Path) -> f64 {
    let sessions: Vec<Vec<u8>> = (0..INVARIANT_SESSIONS)
        .map(|index| {
            with_depth_queries(&stream(
                &INVARIANT_FEATURES,
                INVARIANT_SEED + index,
                INVARIANT_LINES,
            ))
        })
        .collect();
    let engine: PathBuf = engine.to_path_buf();
    let results = super::parallel_map(&sessions, |input| {
        let first = conform::run_engine(&engine, input, SESSION_TIMEOUT);
        let second = conform::run_engine(&engine, input, SESSION_TIMEOUT);
        match (first, second) {
            (Ok(first), Ok(second)) => {
                let text = String::from_utf8_lossy(&first);
                let input = String::from_utf8_lossy(input);
                [
                    first == second,
                    depth_is_ordered(&text),
                    BookModel::follows(&input, &text),
                ]
            }
            _ => [false, false, false],
        }
    });
    let passed = results.iter().flatten().filter(|&&ok| ok).count();
    ratio(passed, results.len() * 3)
}

/// Adds a full depth query of every defined symbol after each line.
fn with_depth_queries(session: &str) -> Vec<u8> {
    let mut symbols: Vec<String> = Vec::new();
    let mut out = String::new();
    for line in session.lines() {
        out.push_str(line);
        out.push('\n');
        if let Some(rest) = line.strip_prefix("S,") {
            let name = rest.split(',').next().unwrap_or_default().to_string();
            if !symbols.contains(&name) {
                symbols.push(name);
            }
        }
        for symbol in &symbols {
            out.push_str(&format!("D,{symbol},1000000\n"));
        }
    }
    out.into_bytes()
}

/// Every depth snapshot lists bids from high to low and asks from low to high, with positive
/// quantities and order counts, and the best bid is below the best ask.
fn depth_is_ordered(output: &str) -> bool {
    let lines: Vec<&str> = output.lines().collect();
    let mut index = 0;
    while index < lines.len() {
        let fields: Vec<&str> = lines[index].split(',').collect();
        index += 1;
        if fields.first() != Some(&"DEPTH") || fields.len() != 4 {
            continue;
        }
        let (Ok(bids), Ok(asks)) = (fields[2].parse::<usize>(), fields[3].parse::<usize>()) else {
            return false;
        };
        let Some(levels) = lines.get(index..index + bids + asks) else {
            return false;
        };
        index += bids + asks;
        let mut prices = Vec::new();
        for (position, level) in levels.iter().enumerate() {
            let parts: Vec<&str> = level.split(',').collect();
            let side = if position < bids { "B" } else { "S" };
            let numbers: Vec<u64> = parts
                .iter()
                .skip(2)
                .filter_map(|p| p.parse().ok())
                .collect();
            if parts.len() != 5 || parts[0] != "LEVEL" || parts[1] != side || numbers.len() != 3 {
                return false;
            }
            if numbers[1] == 0 || numbers[2] == 0 {
                return false;
            }
            prices.push(numbers[0]);
        }
        let (bid_prices, ask_prices) = prices.split_at(bids);
        let descending = bid_prices.windows(2).all(|pair| pair[0] > pair[1]);
        let ascending = ask_prices.windows(2).all(|pair| pair[0] < pair[1]);
        let uncrossed = match (bid_prices.first(), ask_prices.first()) {
            (Some(bid), Some(ask)) => bid < ask,
            _ => true,
        };
        if !(descending && ascending && uncrossed) {
            return false;
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Buy,
    Sell,
}

/// The resting orders of one symbol: each price level is a queue of (order id, open quantity) in arrival order.
#[derive(Debug, Default)]
struct Book {
    bids: BTreeMap<u64, VecDeque<(u64, u64)>>,
    asks: BTreeMap<u64, VecDeque<(u64, u64)>>,
}

impl Book {
    fn side(&mut self, side: Side) -> &mut BTreeMap<u64, VecDeque<(u64, u64)>> {
        match side {
            Side::Buy => &mut self.bids,
            Side::Sell => &mut self.asks,
        }
    }

    fn best(&self, side: Side) -> Option<(u64, &VecDeque<(u64, u64)>)> {
        match side {
            Side::Buy => self.bids.iter().next_back().map(|(p, q)| (*p, q)),
            Side::Sell => self.asks.iter().next().map(|(p, q)| (*p, q)),
        }
    }

    fn levels(&self, side: Side, limit: usize) -> Vec<(u64, u64, usize)> {
        let summary = |(price, queue): (&u64, &VecDeque<(u64, u64)>)| {
            (
                *price,
                queue.iter().map(|(_, qty)| qty).sum::<u64>(),
                queue.len(),
            )
        };
        match side {
            Side::Buy => self.bids.iter().rev().take(limit).map(summary).collect(),
            Side::Sell => self.asks.iter().take(limit).map(summary).collect(),
        }
    }
}

/// An engine's events replayed against a model book: every fill takes the oldest order at the
/// best price, never more than is open, and what rests, trades and leaves adds up to what was ordered.
struct BookModel<'a> {
    books: BTreeMap<String, Book>,
    live: BTreeMap<u64, (String, Side, u64)>,
    events: std::iter::Peekable<std::str::Lines<'a>>,
}

impl<'a> BookModel<'a> {
    fn follows(input: &str, output: &'a str) -> bool {
        let mut model = BookModel {
            books: BTreeMap::new(),
            live: BTreeMap::new(),
            events: output.lines().peekable(),
        };
        for (index, line) in input.lines().enumerate() {
            if !model.command(index + 1, line) {
                return false;
            }
        }
        model.events.next().is_none()
    }

    fn next_fields(&mut self) -> Option<Vec<&'a str>> {
        self.events.next().map(|event| event.split(',').collect())
    }

    /// Follows the events of one command. False when they do not fit the model.
    fn command(&mut self, number: usize, line: &str) -> bool {
        let fields: Vec<&str> = line.split(',').collect();
        let Some(first) = self.events.peek() else {
            return false;
        };
        if first.starts_with("REJECT,") {
            let event = self.next_fields().unwrap_or_default();
            return event.get(1) == Some(&number.to_string().as_str());
        }
        match fields.first().copied() {
            Some("S") => {
                self.books.entry(fields[1].to_string()).or_default();
                self.next_fields()
                    .is_some_and(|e| e.first() == Some(&"SYMBOL"))
            }
            Some("N") if fields.len() == 8 => {
                let price = fields[4].parse().ok();
                self.order(&fields[1..4], price, fields[5], fields[6])
            }
            Some("M") if fields.len() == 6 => self.order(&fields[1..4], None, fields[4], "MARKET"),
            Some("C") => self.cancel(fields.get(1).copied().unwrap_or_default()),
            Some("Q") => self.top(fields.get(1).copied().unwrap_or_default()),
            Some("D") if fields.len() == 3 => {
                let levels = fields[2].parse().unwrap_or(usize::MAX);
                self.depth(fields[1], levels)
            }
            _ => false,
        }
    }

    fn order(&mut self, head: &[&str], price: Option<u64>, qty: &str, tif: &str) -> bool {
        let (Ok(id), Ok(qty)) = (head[0].parse::<u64>(), qty.parse::<u64>()) else {
            return false;
        };
        let symbol = head[1].to_string();
        let side = if head[2] == "B" {
            Side::Buy
        } else {
            Side::Sell
        };
        let accepted = self.next_fields();
        if accepted.as_deref() != Some(&["ACCEPT", head[0]][..]) {
            return false;
        }
        let mut remaining = qty;
        let mut unfilled = None;
        let id_text = head[0];
        while let Some(event) = self.events.peek().map(|e| e.split(',').collect::<Vec<_>>()) {
            match event.as_slice() {
                ["TRADE", sym, at, filled, taker, maker] if *taker == id_text && *sym == symbol => {
                    self.events.next();
                    let (Ok(at), Ok(filled), Ok(maker)) =
                        (at.parse(), filled.parse(), maker.parse())
                    else {
                        return false;
                    };
                    if filled > remaining || !self.fill(&symbol, side, price, at, filled, maker) {
                        return false;
                    }
                    remaining -= filled;
                }
                ["OUT", out_id, left, "UNFILLED"] if *out_id == id_text => {
                    self.events.next();
                    unfilled = left.parse::<u64>().ok();
                    break;
                }
                _ => break,
            }
        }
        let rests = matches!(tif, "GTC" | "POST");
        match (rests, unfilled) {
            (true, None) => {
                if remaining > 0 {
                    let Some(price) = price else { return false };
                    let book = self.books.entry(symbol.clone()).or_default();
                    book.side(side)
                        .entry(price)
                        .or_default()
                        .push_back((id, remaining));
                    self.live.insert(id, (symbol, side, price));
                }
                true
            }
            (false, Some(left)) => {
                left == remaining && remaining > 0 && (tif != "FOK" || left == qty)
            }
            (false, None) => remaining == 0,
            (true, Some(_)) => false,
        }
    }

    /// Applies one fill of the incoming order against the best opposite level.
    fn fill(
        &mut self,
        symbol: &str,
        side: Side,
        limit: Option<u64>,
        at: u64,
        filled: u64,
        maker: u64,
    ) -> bool {
        let opposite = if side == Side::Buy {
            Side::Sell
        } else {
            Side::Buy
        };
        let Some(book) = self.books.get_mut(symbol) else {
            return false;
        };
        let Some((best, queue)) = book.best(opposite) else {
            return false;
        };
        let within_limit = limit.is_none_or(|limit| match side {
            Side::Buy => at <= limit,
            Side::Sell => at >= limit,
        });
        let Some(&(front_id, front_qty)) = queue.front() else {
            return false;
        };
        if best != at || front_id != maker || filled > front_qty || !within_limit {
            return false;
        }
        let levels = book.side(opposite);
        let queue = levels.get_mut(&at).expect("the best level exists");
        if filled == front_qty {
            queue.pop_front();
            self.live.remove(&maker);
            if queue.is_empty() {
                levels.remove(&at);
            }
        } else if let Some(front) = queue.front_mut() {
            front.1 -= filled;
        }
        true
    }

    fn cancel(&mut self, id_text: &str) -> bool {
        let Ok(id) = id_text.parse::<u64>() else {
            return false;
        };
        let event = self.next_fields().unwrap_or_default();
        let Some((symbol, side, price)) = self.live.remove(&id) else {
            return false;
        };
        let levels = self.books.entry(symbol).or_default().side(side);
        let Some(queue) = levels.get_mut(&price) else {
            return false;
        };
        let Some(position) = queue.iter().position(|(order, _)| *order == id) else {
            return false;
        };
        let (_, open) = queue
            .remove(position)
            .expect("the position is in the queue");
        if queue.is_empty() {
            levels.remove(&price);
        }
        event == ["OUT", id_text, &open.to_string(), "CANCELLED"]
    }

    fn top(&mut self, symbol: &str) -> bool {
        let event = self.next_fields().unwrap_or_default();
        let book = self.books.entry(symbol.to_string()).or_default();
        let best = |side| {
            book.levels(side, 1)
                .first()
                .map_or("0,0".to_string(), |(price, qty, _)| {
                    format!("{price},{qty}")
                })
        };
        event.join(",") == format!("TOP,{symbol},{},{}", best(Side::Buy), best(Side::Sell))
    }

    fn depth(&mut self, symbol: &str, levels: usize) -> bool {
        let header = self.next_fields().unwrap_or_default();
        let book = self.books.entry(symbol.to_string()).or_default();
        let bids = book.levels(Side::Buy, levels);
        let asks = book.levels(Side::Sell, levels);
        if header.join(",") != format!("DEPTH,{symbol},{},{}", bids.len(), asks.len()) {
            return false;
        }
        let wanted: Vec<String> = bids
            .iter()
            .map(|(p, q, n)| format!("LEVEL,B,{p},{q},{n}"))
            .chain(asks.iter().map(|(p, q, n)| format!("LEVEL,S,{p},{q},{n}")))
            .collect();
        wanted
            .iter()
            .all(|level| self.events.next() == Some(level.as_str()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_consistent_session_passes_the_book_model() {
        let input = "S,ACME,1,1\nN,1,ACME,S,10,5,GTC,A\nN,2,ACME,S,10,5,GTC,B\nN,3,ACME,B,10,7,IOC,C\nD,ACME,5\n";
        let output = "SYMBOL,ACME\nACCEPT,1\nACCEPT,2\nACCEPT,3\nTRADE,ACME,10,5,3,1\nTRADE,ACME,10,2,3,2\n\
                      DEPTH,ACME,0,1\nLEVEL,S,10,3,1\n";

        assert!(BookModel::follows(input, output));
        assert!(depth_is_ordered(output));
    }

    #[test]
    fn a_fill_out_of_arrival_order_fails_the_book_model() {
        let input =
            "S,ACME,1,1\nN,1,ACME,S,10,5,GTC,A\nN,2,ACME,S,10,5,GTC,B\nN,3,ACME,B,10,5,IOC,C\n";
        let output = "SYMBOL,ACME\nACCEPT,1\nACCEPT,2\nACCEPT,3\nTRADE,ACME,10,5,3,2\n";

        assert!(!BookModel::follows(input, output));
    }

    #[test]
    fn a_crossed_depth_snapshot_fails() {
        let output = "DEPTH,ACME,1,1\nLEVEL,B,11,1,1\nLEVEL,S,10,1,1\n";

        assert!(!depth_is_ordered(output));
    }
}
