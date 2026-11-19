//! Busy intervals and their union, so agents that work at the same time are counted once in the scored time.

use super::timestamp::Millis;

/// A half-open span of time `[start, end)` in milliseconds. An empty or reversed span counts as nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Interval {
    pub start: Millis,
    pub end: Millis,
}

impl Interval {
    pub fn new(start: Millis, end: Millis) -> Interval {
        Interval { start, end }
    }
}

/// Sorted, non-overlapping intervals that cover the same time as the input.
pub fn merge(intervals: &[Interval]) -> Vec<Interval> {
    let mut sorted: Vec<Interval> = intervals
        .iter()
        .copied()
        .filter(|i| i.end > i.start)
        .collect();
    sorted.sort_by_key(|i| i.start);
    let mut merged: Vec<Interval> = Vec::with_capacity(sorted.len());
    for interval in sorted {
        match merged.last_mut() {
            Some(last) if interval.start <= last.end => last.end = last.end.max(interval.end),
            _ => merged.push(interval),
        }
    }
    merged
}

/// Total time covered by at least one interval.
pub fn covered_millis(intervals: &[Interval]) -> Millis {
    merge(intervals).iter().map(|i| i.end - i.start).sum()
}

/// The largest number of groups busy at one moment. Each group is one agent's intervals.
pub fn peak_overlap(groups: &[Vec<Interval>]) -> u32 {
    let mut edges: Vec<(Millis, i32)> = Vec::new();
    for group in groups {
        for interval in merge(group) {
            edges.push((interval.start, 1));
            edges.push((interval.end, -1));
        }
    }
    // At equal times an end sorts before a start, so touching intervals do not overlap.
    edges.sort();
    let mut running = 0i32;
    let mut peak = 0i32;
    for (_, step) in edges {
        running += step;
        peak = peak.max(running);
    }
    u32::try_from(peak).expect("a running count of open intervals is never negative")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn interval() -> impl Strategy<Value = Interval> {
        (0i64..10_000, 0i64..2_000).prop_map(|(start, length)| Interval::new(start, start + length))
    }

    fn agents() -> impl Strategy<Value = Vec<Vec<Interval>>> {
        prop::collection::vec(prop::collection::vec(interval(), 0..8), 1..6)
    }

    fn wall(groups: &[Vec<Interval>]) -> Millis {
        let all: Vec<&Interval> = groups
            .iter()
            .flatten()
            .filter(|i| i.end > i.start)
            .collect();
        let first = all.iter().map(|i| i.start).min().unwrap_or(0);
        let last = all.iter().map(|i| i.end).max().unwrap_or(0);
        last - first
    }

    proptest! {
        #[test]
        fn busy_time_is_at_most_wall_time_and_at_most_summed_agent_time(groups in agents()) {
            let all: Vec<Interval> = groups.iter().flatten().copied().collect();
            let busy = covered_millis(&all);
            let summed: Millis = groups.iter().map(|g| covered_millis(g)).sum();
            prop_assert!(busy <= wall(&groups));
            prop_assert!(busy <= summed);
            prop_assert!(busy >= groups.iter().map(|g| covered_millis(g)).max().unwrap_or(0));
        }

        #[test]
        fn agents_working_in_parallel_are_not_summed(start in 0i64..1_000, length in 1i64..5_000, agent_count in 1usize..8) {
            let groups: Vec<Vec<Interval>> = (0..agent_count)
                .map(|_| vec![Interval::new(start, start + length)])
                .collect();
            let all: Vec<Interval> = groups.iter().flatten().copied().collect();
            prop_assert_eq!(covered_millis(&all), length);
            prop_assert_eq!(peak_overlap(&groups), u32::try_from(agent_count).unwrap());
        }

        #[test]
        fn merged_intervals_are_sorted_and_apart(intervals in prop::collection::vec(interval(), 0..20)) {
            let merged = merge(&intervals);
            for pair in merged.windows(2) {
                prop_assert!(pair[0].end < pair[1].start);
            }
            prop_assert_eq!(covered_millis(&merged), covered_millis(&intervals));
        }
    }

    #[test]
    fn touching_intervals_do_not_count_as_concurrent() {
        let groups = vec![vec![Interval::new(0, 10)], vec![Interval::new(10, 20)]];
        assert_eq!(peak_overlap(&groups), 1);
    }
}
