//! Large seeded sessions for timing engines: throughput workloads and the book-size scaling probe.

use std::io::{self, Write};

use serde::Deserialize;

use super::rng::Rng;

/// The shape of a throughput workload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Shape {
    /// Every command type, near the touch, as a busy market would send them.
    Mix,
    /// Deep books of small orders, swept many levels at a time by large orders.
    Sweep,
    /// Deep books under frequent depth queries and large fill-or-kill orders.
    DepthAndFok,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Throughput {
    pub name: String,
    pub shape: Shape,
    pub seed: u64,
    pub lines: u64,
    pub symbols: usize,
}

const MID: u64 = 100_000;

impl Throughput {
    pub fn write(&self, out: &mut impl Write) -> io::Result<()> {
        let mut rng = Rng::new(self.seed);
        for symbol in 0..self.symbols {
            writeln!(out, "S,S{symbol},1,1")?;
        }
        let symbols = u64::try_from(self.symbols).expect("a usize fits in u64");
        for id in 1..=self.lines - symbols {
            let symbol = rng.between(0, symbols - 1);
            let side = *rng.pick(&["B", "S"]);
            let owner = rng.between(0, 49);
            let recent = rng.between(id.saturating_sub(5000).max(1), id);
            let roll = rng.between(0, 99);
            match self.shape {
                Shape::Mix => match roll {
                    0..45 => {
                        let price = near_touch(&mut rng, side, 30, 10);
                        let qty = rng.between(1, 100);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},GTC,O{owner}")?;
                    }
                    45..65 => writeln!(out, "C,{recent}")?,
                    65..75 => {
                        let price = near_touch(&mut rng, side, 30, 0);
                        let qty = rng.between(1, 100);
                        writeln!(out, "A,{recent},{price},{qty}")?;
                    }
                    75..83 => {
                        let price = near_touch(&mut rng, side, 5, 20);
                        let qty = rng.between(1, 200);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},IOC,O{owner}")?;
                    }
                    83..87 => {
                        let qty = rng.between(1, 300);
                        writeln!(out, "M,{id},S{symbol},{side},{qty},O{owner}")?;
                    }
                    87..92 => {
                        let price = near_touch(&mut rng, side, 30, 5);
                        let qty = rng.between(50, 500);
                        let display = rng.between(1, 50);
                        writeln!(
                            out,
                            "I,{id},S{symbol},{side},{price},{qty},{display},O{owner}"
                        )?;
                    }
                    92..96 => {
                        let price = near_touch(&mut rng, side, 0, 20);
                        let qty = rng.between(1, 400);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},FOK,O{owner}")?;
                    }
                    96..98 => writeln!(out, "Q,S{symbol}")?,
                    _ => writeln!(out, "D,S{symbol},10")?,
                },
                Shape::Sweep => match roll {
                    0..75 => {
                        let price = near_touch(&mut rng, side, 500, 0);
                        let qty = rng.between(1, 20);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},GTC,O{owner}")?;
                    }
                    75..85 => {
                        let price = near_touch(&mut rng, side, 500, 0);
                        let qty = rng.between(20, 100);
                        let display = rng.between(1, 10);
                        writeln!(
                            out,
                            "I,{id},S{symbol},{side},{price},{qty},{display},O{owner}"
                        )?;
                    }
                    85..90 => {
                        let qty = rng.between(200, 3000);
                        writeln!(out, "M,{id},S{symbol},{side},{qty},O{owner}")?;
                    }
                    90..95 => {
                        let price = near_touch(&mut rng, side, 0, 300);
                        let qty = rng.between(200, 3000);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},IOC,O{owner}")?;
                    }
                    _ => writeln!(out, "C,{recent}")?,
                },
                Shape::DepthAndFok => match roll {
                    0..60 => {
                        let price = near_touch(&mut rng, side, 200, 0);
                        let qty = rng.between(1, 50);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},GTC,O{owner}")?;
                    }
                    60..75 => writeln!(out, "D,S{symbol},25")?,
                    75..90 => {
                        let price = near_touch(&mut rng, side, 0, 100);
                        let qty = rng.between(50, 3000);
                        writeln!(out, "N,{id},S{symbol},{side},{price},{qty},FOK,O{owner}")?;
                    }
                    _ => writeln!(out, "C,{recent}")?,
                },
            }
        }
        Ok(())
    }
}

/// A price up to `behind` ticks on the passive side of the middle, or up to `through` ticks across it.
fn near_touch(rng: &mut Rng, side: &str, behind: u64, through: u64) -> u64 {
    let offset = rng.between(0, behind + through);
    if side == "B" {
        MID + through - offset
    } else {
        MID - through + offset
    }
}

/// The scaling probe: books of growing size, then the same number of operations on each.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scaling {
    pub seed: u64,
    pub operations: u64,
    pub book_sizes: Vec<u64>,
}

const LOWEST_ASK: u64 = 500_000_001;
const HIGHEST_PRICE: u64 = 1_000_000_000;

impl Scaling {
    /// Writes a session that first rests `book_size` orders and then runs `operations` adds,
    /// cancels, amends and queries, none of which trade, so the book keeps about its size.
    /// The time per operation is the time of this session less that of the one with no operations.
    pub fn write(&self, book_size: u64, operations: u64, out: &mut impl Write) -> io::Result<()> {
        let mut rng = Rng::new(self.seed ^ book_size);
        let mut live: Vec<(u64, &str)> = Vec::new();
        let mut next_id = 1;
        writeln!(out, "S,PROBE,1,1")?;
        for index in 0..book_size + operations {
            let roll = if index < book_size {
                0
            } else {
                rng.between(0, 99)
            };
            match roll {
                0..40 => {
                    let side = *rng.pick(&["B", "S"]);
                    let price = passive_price(&mut rng, side);
                    let qty = rng.between(1, 100);
                    writeln!(out, "N,{next_id},PROBE,{side},{price},{qty},GTC,OWNER")?;
                    live.push((next_id, side));
                    next_id += 1;
                }
                40..80 if !live.is_empty() => {
                    let (id, _) = live.swap_remove(rng.below(live.len()));
                    writeln!(out, "C,{id}")?;
                }
                80..95 if !live.is_empty() => {
                    let (id, side) = *rng.pick(&live);
                    let price = passive_price(&mut rng, side);
                    let qty = rng.between(1, 100);
                    writeln!(out, "A,{id},{price},{qty}")?;
                }
                _ => writeln!(out, "Q,PROBE")?,
            }
        }
        Ok(())
    }
}

/// Bids sit below `LOWEST_ASK` and asks at or above it, so no probe order ever trades.
fn passive_price(rng: &mut Rng, side: &str) -> u64 {
    if side == "B" {
        rng.between(1, LOWEST_ASK - 1)
    } else {
        rng.between(LOWEST_ASK, HIGHEST_PRICE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throughput(shape: Shape) -> Throughput {
        Throughput {
            name: "test".to_owned(),
            shape,
            seed: 5,
            lines: 5000,
            symbols: 4,
        }
    }

    #[test]
    fn one_seed_gives_the_same_workload_twice() {
        for shape in [Shape::Mix, Shape::Sweep, Shape::DepthAndFok] {
            let (mut first, mut second) = (Vec::new(), Vec::new());
            throughput(shape).write(&mut first).unwrap();
            throughput(shape).write(&mut second).unwrap();
            assert_eq!(first, second);
            assert_eq!(first.iter().filter(|&&byte| byte == b'\n').count(), 5000);
        }
    }

    #[test]
    fn one_seed_gives_the_same_scaling_probe_twice() {
        let probe = Scaling {
            seed: 9,
            operations: 1000,
            book_sizes: vec![100],
        };
        let (mut first, mut second) = (Vec::new(), Vec::new());
        probe.write(100, 1000, &mut first).unwrap();
        probe.write(100, 1000, &mut second).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.iter().filter(|&&byte| byte == b'\n').count(), 1101);
        let mut base = Vec::new();
        probe.write(100, 0, &mut base).unwrap();
        assert!(
            first.starts_with(&base),
            "the base is the probe's first part"
        );
    }
}
