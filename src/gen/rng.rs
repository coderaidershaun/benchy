//! A small seeded random source, so that one seed gives the same commands on every machine.

/// SplitMix64: fast, good enough for test data, and fixed forever so seeds stay reproducible.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng { state: seed }
    }

    /// A seed for stream `index` of a run with `seed`, independent of every other stream.
    pub fn derive(seed: u64, index: u64) -> Rng {
        let mut base = Rng::new(seed ^ index.wrapping_mul(0xA076_1D64_78BD_642F));
        Rng::new(base.next_u64())
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A value in `low..=high`.
    pub fn between(&mut self, low: u64, high: u64) -> u64 {
        low + self.next_u64() % (high - low + 1)
    }

    pub fn below(&mut self, bound: usize) -> usize {
        let bound = u64::try_from(bound).expect("a usize fits in u64");
        usize::try_from(self.next_u64() % bound).expect("the value is below a usize bound")
    }

    /// True with a chance of `percent` in 100.
    pub fn percent(&mut self, percent: u64) -> bool {
        self.next_u64() % 100 < percent
    }

    pub fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}
