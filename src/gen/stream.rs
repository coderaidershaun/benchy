//! Short random command streams that use only a chosen set of engine features.
//! An engine with one broken feature still gives the right output for every stream that leaves it out.

use std::fmt::Write;

use serde::Deserialize;

use super::rng::Rng;

/// A feature of the spec that a stream may use. Resting limit orders are always used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Feature {
    Market,
    Ioc,
    Fok,
    PostOnly,
    Iceberg,
    Cancel,
    Amend,
    SelfTrade,
    Queries,
    TickLot,
    Symbols,
    Parsing,
    Rejections,
}

impl Feature {
    pub const ALL: [Feature; 13] = [
        Feature::Market,
        Feature::Ioc,
        Feature::Fok,
        Feature::PostOnly,
        Feature::Iceberg,
        Feature::Cancel,
        Feature::Amend,
        Feature::SelfTrade,
        Feature::Queries,
        Feature::TickLot,
        Feature::Symbols,
        Feature::Parsing,
        Feature::Rejections,
    ];
}

#[derive(Debug, Clone, Copy)]
enum Kind {
    Limit,
    Market,
    Ioc,
    Fok,
    PostOnly,
    Iceberg,
    Cancel,
    Amend,
    Query,
    Malformed,
    Rejection,
}

impl Kind {
    /// How often a command of this kind is chosen, against the others in the stream.
    fn weight(self) -> u64 {
        match self {
            Kind::Limit => 40,
            Kind::Iceberg | Kind::Cancel | Kind::Amend | Kind::Malformed => 12,
            Kind::Market | Kind::Ioc | Kind::Fok | Kind::PostOnly | Kind::Query => 9,
            Kind::Rejection => 6,
        }
    }

    fn of(feature: Feature) -> Option<Kind> {
        match feature {
            Feature::Market => Some(Kind::Market),
            Feature::Ioc => Some(Kind::Ioc),
            Feature::Fok => Some(Kind::Fok),
            Feature::PostOnly => Some(Kind::PostOnly),
            Feature::Iceberg => Some(Kind::Iceberg),
            Feature::Cancel => Some(Kind::Cancel),
            Feature::Amend => Some(Kind::Amend),
            Feature::Queries => Some(Kind::Query),
            Feature::Parsing => Some(Kind::Malformed),
            Feature::Rejections => Some(Kind::Rejection),
            Feature::SelfTrade | Feature::TickLot | Feature::Symbols => None,
        }
    }
}

struct Symbol {
    name: &'static str,
    tick: u64,
    lot: u64,
}

const SYMBOL_NAMES: [&str; 3] = ["ACME", "BOLT", "CRUX"];
const SHARED_OWNERS: [&str; 3] = ["AL", "BO", "CY"];

/// Writes one stream of `lines` commands, each ending in LF.
pub fn stream(features: &[Feature], seed: u64, lines: usize) -> String {
    let mut writer = StreamWriter::new(features, Rng::new(seed));
    writer.write(lines);
    writer.text
}

struct StreamWriter<'a> {
    features: &'a [Feature],
    rng: Rng,
    kinds: Vec<Kind>,
    symbols: Vec<Symbol>,
    next_id: u64,
    text: String,
}

impl<'a> StreamWriter<'a> {
    fn new(features: &'a [Feature], rng: Rng) -> StreamWriter<'a> {
        let mut kinds = vec![Kind::Limit];
        kinds.extend(features.iter().filter_map(|&feature| Kind::of(feature)));
        StreamWriter {
            features,
            rng,
            kinds,
            symbols: Vec::new(),
            next_id: 1,
            text: String::new(),
        }
    }

    fn uses(&self, feature: Feature) -> bool {
        self.features.contains(&feature)
    }

    fn write(&mut self, lines: usize) {
        let symbol_count = if self.uses(Feature::Symbols) { 3 } else { 1 };
        for name in &SYMBOL_NAMES[..symbol_count] {
            let (tick, lot) = if self.uses(Feature::TickLot) {
                (
                    *self.rng.pick(&[1, 5, 10, 25]),
                    *self.rng.pick(&[1, 10, 100]),
                )
            } else {
                (1, 1)
            };
            self.line(format_args!("S,{name},{tick},{lot}"));
            self.symbols.push(Symbol { name, tick, lot });
        }
        let total_weight: u64 = self.kinds.iter().map(|kind| kind.weight()).sum();
        for _ in symbol_count..lines {
            let mut roll = self.rng.between(0, total_weight - 1);
            let mut chosen = Kind::Limit;
            for &kind in &self.kinds {
                if roll < kind.weight() {
                    chosen = kind;
                    break;
                }
                roll -= kind.weight();
            }
            self.command(chosen);
        }
    }

    fn line(&mut self, args: std::fmt::Arguments<'_>) {
        self.text
            .write_fmt(args)
            .expect("writing to a String cannot fail");
        self.text.push('\n');
    }

    fn command(&mut self, kind: Kind) {
        let symbol = self.rng.below(self.symbols.len());
        let side = *self.rng.pick(&["B", "S"]);
        match kind {
            Kind::Limit => {
                let (id, name, price, qty, owner) = self.order_fields(symbol, side, 10);
                self.line(format_args!(
                    "N,{id},{name},{side},{price},{qty},GTC,{owner}"
                ));
            }
            Kind::Ioc | Kind::Fok | Kind::PostOnly => {
                let tif = match kind {
                    Kind::Ioc => "IOC",
                    Kind::Fok => "FOK",
                    _ => "POST",
                };
                let (id, name, price, qty, owner) = self.order_fields(symbol, side, 20);
                self.line(format_args!(
                    "N,{id},{name},{side},{price},{qty},{tif},{owner}"
                ));
            }
            Kind::Market => {
                let (id, name, _, qty, owner) = self.order_fields(symbol, side, 15);
                self.line(format_args!("M,{id},{name},{side},{qty},{owner}"));
            }
            Kind::Iceberg => {
                let (id, name, price, qty, owner) = self.order_fields(symbol, side, 30);
                let lot = self.symbols[symbol].lot;
                let display = lot * self.rng.between(1, (qty / lot).max(1));
                self.line(format_args!(
                    "I,{id},{name},{side},{price},{qty},{display},{owner}"
                ));
            }
            Kind::Cancel => {
                let id = self.earlier_id();
                self.line(format_args!("C,{id}"));
            }
            Kind::Amend => {
                let id = self.earlier_id();
                let price = self.price(symbol, side);
                let qty = self.qty(symbol, 12);
                self.line(format_args!("A,{id},{price},{qty}"));
            }
            Kind::Query => {
                let name = self.symbols[symbol].name;
                if self.rng.percent(50) {
                    self.line(format_args!("Q,{name}"));
                } else {
                    let levels = self.rng.between(1, 6);
                    self.line(format_args!("D,{name},{levels}"));
                }
            }
            Kind::Malformed => {
                let line = self.malformed_line(symbol);
                self.line(format_args!("{line}"));
            }
            Kind::Rejection => self.rejection(symbol),
        }
    }

    /// Fields of a new order: id, symbol name, price, quantity of up to `max_lots` lots, owner.
    fn order_fields(
        &mut self,
        symbol: usize,
        side: &str,
        max_lots: u64,
    ) -> (u64, &'static str, u64, u64, String) {
        let id = self.next_id;
        self.next_id += 1;
        let owner = if self.uses(Feature::SelfTrade) {
            (*self.rng.pick(&SHARED_OWNERS)).to_owned()
        } else {
            format!("U{id}")
        };
        let price = self.price(symbol, side);
        let qty = self.qty(symbol, max_lots);
        (id, self.symbols[symbol].name, price, qty, owner)
    }

    /// A price a few ticks from 100 ticks, mostly on the passive side so that books build up,
    /// and sometimes off the tick grid when tick and lot are used.
    fn price(&mut self, symbol: usize, side: &str) -> u64 {
        let tick = self.symbols[symbol].tick;
        let ticks = if side == "B" {
            self.rng.between(94, 101)
        } else {
            self.rng.between(99, 106)
        };
        let price = tick * ticks;
        if tick > 1 && self.rng.percent(8) {
            price + self.rng.between(1, tick - 1)
        } else {
            price
        }
    }

    fn qty(&mut self, symbol: usize, max_lots: u64) -> u64 {
        let lot = self.symbols[symbol].lot;
        let qty = lot * self.rng.between(1, max_lots);
        if lot > 1 && self.rng.percent(8) {
            qty + self.rng.between(1, lot - 1)
        } else {
            qty
        }
    }

    /// A recent id, which is often still live, or now and then the next id, which is not used yet.
    fn earlier_id(&mut self) -> u64 {
        self.rng
            .between(self.next_id.saturating_sub(8).max(1), self.next_id)
    }

    fn rejection(&mut self, symbol: usize) {
        let name = self.symbols[symbol].name;
        let side = *self.rng.pick(&["B", "S"]);
        match self.rng.below(4) {
            0 if self.next_id > 1 => {
                let id = self.rng.between(1, self.next_id - 1);
                self.line(format_args!("N,{id},{name},{side},100,1,GTC,DUP"));
            }
            1 => {
                let id = self.next_id;
                self.line(format_args!("N,{id},NOPE,{side},100,1,GTC,ZED"));
            }
            2 => self.line(format_args!("S,{name},1,1")),
            _ => {
                let id = self.next_id + 1000;
                self.line(format_args!("C,{id}"));
            }
        }
    }

    /// A line that breaks one parsing rule, built from a valid order line.
    fn malformed_line(&mut self, symbol: usize) -> String {
        let (id, name, price, qty, owner) = self.order_fields(symbol, "B", 10);
        let mut fields: Vec<String> = ["N", &id.to_string(), name, "B", &price.to_string()]
            .iter()
            .map(|field| (*field).to_owned())
            .chain([qty.to_string(), "GTC".to_owned(), owner])
            .collect();
        let field = self.rng.between(1, 7) as usize;
        match self.rng.below(12) {
            0 => fields[field] = String::new(),
            1 => fields[field] = format!("+{}", fields[field]),
            2 => fields[field] = format!(" {}", fields[field]),
            3 => fields[field] = fields[field].to_lowercase(),
            4 => fields[field] = "0".to_owned(),
            5 => fields[field] = "1000000001".to_owned(),
            6 => fields[field] = format!("{:0>19}", 1),
            7 => fields.push("X".to_owned()),
            8 => {
                fields.pop();
            }
            9 => fields[0] = (*self.rng.pick(&["n", "X", "", "NN", "Z"])).to_owned(),
            10 => fields[field] = "A".repeat(self.rng.between(250, 300) as usize),
            _ => fields[field] = format!("{:0>18}", self.rng.between(1, 99)),
        }
        fields.join(",")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_seed_gives_the_same_stream_twice() {
        let first = stream(&Feature::ALL, 7, 200);
        let second = stream(&Feature::ALL, 7, 200);
        assert_eq!(first, second);
        assert_ne!(first, stream(&Feature::ALL, 8, 200));
    }

    #[test]
    fn a_stream_uses_only_its_features() {
        let text = stream(&[Feature::Market], 3, 500);
        for line in text.lines() {
            let code = line.split(',').next().unwrap_or_default();
            assert!(["S", "N", "M"].contains(&code), "unexpected line {line}");
            if code == "N" {
                assert!(line.ends_with(&format!(",GTC,U{}", line.split(',').nth(1).unwrap())));
            }
        }
        assert_eq!(text.lines().count(), 500);
    }

    #[test]
    fn stream_lines_are_byte_stable_text() {
        let text = stream(&Feature::ALL, 11, 2000);
        assert!(text.ends_with('\n'));
        for line in text.lines() {
            assert!(
                !line.ends_with(' ') && !line.contains('\r'),
                "line {line:?}"
            );
        }
    }
}
