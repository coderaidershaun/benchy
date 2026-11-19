//! Properties that hold for every session: the book never crosses, quantity is conserved,
//! each price level fills first in first out, and the same input always gives the same output.

use std::collections::HashMap;

use engine::{BestLevel, Engine, Event, OrderId, Qty};

/// A small fixed random source, so that every run of these tests sees the same sessions.
struct Rng(u64);

impl Rng {
    fn below(&mut self, bound: u64) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0 % bound
    }

    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[usize::try_from(self.below(items.len() as u64)).unwrap()]
    }
}

#[derive(Clone, Copy)]
struct Mix {
    icebergs_and_amends: bool,
}

/// A random session on one symbol with three owners, using every command that `mix` allows.
fn random_session(seed: u64, commands: u64, mix: Mix) -> Vec<String> {
    let mut rng = Rng(seed);
    let mut lines = vec!["S,ACME,1,1".to_owned()];
    for id in 1..=commands {
        let side = rng.pick(&["B", "S"]);
        let price = 95 + rng.below(11);
        let qty = 1 + rng.below(20);
        let owner = rng.pick(&["AL", "BO", "CY"]);
        let earlier = 1 + rng.below(id);
        let kinds = if mix.icebergs_and_amends { 7 } else { 5 };
        let line = match rng.below(kinds) {
            0 | 1 => {
                let tif = rng.pick(&["GTC", "GTC", "IOC", "FOK", "POST"]);
                format!("N,{id},ACME,{side},{price},{qty},{tif},{owner}")
            }
            2 => format!("M,{id},ACME,{side},{qty},{owner}"),
            3 => format!("C,{earlier}"),
            4 => "Q,ACME".to_owned(),
            5 => format!("A,{earlier},{price},{qty}"),
            _ => {
                let display = 1 + rng.below(qty);
                format!("I,{id},ACME,{side},{price},{qty},{display},{owner}")
            }
        };
        lines.push(line);
    }
    lines
}

fn run(lines: &[String]) -> Vec<Event> {
    let mut engine = Engine::new();
    let mut events = Vec::new();
    for line in lines {
        engine.handle_line(line.as_bytes(), &mut events);
    }
    events
}

#[test]
fn the_book_is_never_crossed() {
    let mix = Mix {
        icebergs_and_amends: true,
    };
    for seed in 1..50 {
        let mut engine = Engine::new();
        let mut events = Vec::new();
        for line in random_session(seed, 400, mix) {
            engine.handle_line(line.as_bytes(), &mut events);
            engine.handle_line(b"Q,ACME", &mut events);
            let Some(&Event::Top { bid, ask, .. }) = events.last() else {
                panic!("Q writes TOP");
            };
            if let (Some(BestLevel { price: bid, .. }), Some(BestLevel { price: ask, .. })) =
                (bid, ask)
            {
                assert!(bid < ask, "seed {seed}: crossed after {line}");
            }
        }
    }
}

#[test]
fn every_accepted_quantity_trades_or_leaves_with_out() {
    let mix = Mix {
        icebergs_and_amends: false,
    };
    for seed in 1..50 {
        let mut lines = random_session(seed, 400, mix);
        let ids = lines.len();
        lines.extend((1..=ids).map(|id| format!("C,{id}")));
        let mut entered: HashMap<OrderId, Qty> = HashMap::new();
        for line in &lines {
            let fields: Vec<&str> = line.split(',').collect();
            let qty = match fields[0] {
                "N" => fields[5],
                "M" => fields[4],
                _ => continue,
            };
            entered.insert(fields[1].parse().unwrap(), qty.parse().unwrap());
        }
        let mut accounted: HashMap<OrderId, Qty> = HashMap::new();
        let mut accepted = Vec::new();
        for event in run(&lines) {
            match event {
                Event::Accept(id) => accepted.push(id),
                Event::Trade {
                    qty, taker, maker, ..
                } => {
                    *accounted.entry(taker).or_default() += qty;
                    *accounted.entry(maker).or_default() += qty;
                }
                Event::Out { id, qty, .. } => *accounted.entry(id).or_default() += qty,
                _ => {}
            }
        }
        for id in accepted {
            assert_eq!(
                accounted.get(&id).copied().unwrap_or_default(),
                entered[&id],
                "seed {seed}: order {id}"
            );
        }
    }
}

#[test]
fn each_price_level_fills_first_in_first_out() {
    let mix = Mix {
        icebergs_and_amends: false,
    };
    for seed in 1..50 {
        let lines = random_session(seed, 400, mix);
        let mut last_maker_at_price: HashMap<u64, OrderId> = HashMap::new();
        for event in run(&lines) {
            if let Event::Trade { price, maker, .. } = event {
                let last = last_maker_at_price.entry(price).or_default();
                assert!(
                    *last <= maker,
                    "seed {seed}: {maker} traded after {last} at {price}"
                );
                *last = maker;
            }
        }
    }
}

#[test]
fn the_same_input_gives_the_same_output() {
    let mix = Mix {
        icebergs_and_amends: true,
    };
    for seed in 1..20 {
        let lines = random_session(seed, 1000, mix);
        let first: Vec<String> = run(&lines).iter().map(ToString::to_string).collect();
        let second: Vec<String> = run(&lines).iter().map(ToString::to_string).collect();
        assert_eq!(first, second);
    }
}
