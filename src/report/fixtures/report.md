# claude-code run 5d3a9c1e7f20

| Field | Value |
|---|---|
| Date | 2026-11-18T09:00:00.000Z |
| Task version | unversioned |
| Rubric version | 0.1.0 (Provisional) |
| Judge model | claude-opus-5-5 |
| Claude Code | 2.1.327 |
| Model | claude-opus-5-5 |
| Auth mode | api-key |
| Isolation level | clean-config |
| Validity | valid |
| Completion | finished |
| Audit flags | 0 |
| Operator replies | 0 |

## Scoreboard

| Measure | Value | Baseline spread |
|---|---|---|
| Quality | 29.86 (mean 49.77, lowest csharp 0.00) | no baseline yet |
| Cost | 46.26 USD | no baseline yet |
| Elapsed busy time | 1h 30m 20s | no baseline yet |
| Overall | absent | absent until the pilot sets the rubric baselines |
| Efficiency | absent | absent until the pilot sets the rubric baselines |
| Agents | 3 |  |
| Peak concurrency | 3 |  |
| Operator replies | 0 |  |

## Verdict

No baseline spread is published yet, so this run cannot be called better or worse than the baseline. The strongest language was Rust with 83.59 of 100, and its best category was B Robustness with 4.94 of 5.00. The largest loss was 27.00 points on Golden sessions in C#, because the language did not build from a clean checkout. The second largest loss was 14.00 points on Differential fuzz in C#, because the language did not build from a clean checkout. The third largest loss was 5.00 points on Mutation score in Rust, because it was not measured.

## Score breakdown

| Category | Rust | Go | C# | Mean | Maximum |
|---|---|---|---|---|---|
| A Correctness | 44.39 | 39.82 | 0.00 | 28.07 | 45.00 |
| B Robustness | 4.94 | 4.80 | 0.00 | 3.25 | 5.00 |
| C Performance | 7.73 | 4.20 | 0.00 | 3.98 | 8.00 |
| D Tests the harness wrote | 0.00 | 0.00 | 0.00 | 0.00 | 12.00 |
| E Economy | 7.65 | 4.36 | 0.00 | 4.00 | 8.00 |
| F Static maintainability | 6.63 | 4.27 | 0.00 | 3.63 | 7.00 |
| G Readability and comments | 4.50 | 3.00 | 0.00 | 2.50 | 6.00 |
| H Ergonomics and design | 5.50 | 3.07 | 0.00 | 2.86 | 6.00 |
| I Delivery | 2.25 | 2.20 | 0.00 | 1.48 | 3.00 |
| Total | 83.59 | 65.71 | 0.00 | 49.77 | 100.00 |
| Correctness factor | 1.000 | 0.800 | 0.000 |  |  |

## Category detail

Each cell is raw value / points. Full points at the full threshold, none at the zero threshold. Points are after the build gate and the correctness scaling.

### A Correctness (45.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Golden sessions (measured) | 1 / 0 (linear) | 0.98 / 26.46 | 0.86 / 23.22 | not measured / 0.00 | 27.00 |
| Differential fuzz (measured) | 1 / 0 (linear) | 0.995 / 13.93 | 0.9 / 12.60 | not measured / 0.00 | 14.00 |
| Determinism and invariants (measured) | 1 / 0 (linear) | 1 / 4.00 | 1 / 4.00 | not measured / 0.00 | 4.00 |

### B Robustness (5.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| No crash (measured) | 1 / 0.5 (linear) | 1 / 2.00 | 1 / 2.00 | not measured / 0.00 | 2.00 |
| Correct rejection (measured) | 1 / 0.5 (linear) | 0.97 / 0.94 | 0.9 / 0.80 | not measured / 0.00 | 1.00 |
| Clean exit (measured) | 1 / 0.5 (linear) | 1 / 1.00 | 1 / 1.00 | not measured / 0.00 | 1.00 |
| Bounded memory (measured) | 1 / 0 (linear) | 1 / 1.00 | 1 / 1.00 | not measured / 0.00 | 1.00 |

### C Performance (8.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Throughput (measured) | 1.25 / 10 (log) | 1.1 / 3.00 | 2 / 1.86 | not measured / 0.00 | 3.00 |
| Scaling (measured) | 0.25 / 1 (linear) | 0.3 / 3.73 | 0.6 / 1.71 | not measured / 0.00 | 4.00 |
| Peak memory (measured) | 1.5 / 6 (log) | 1.2 / 1.00 | 2 / 0.63 | not measured / 0.00 | 1.00 |

### D Tests the harness wrote (12.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Pass and stable (measured) | 1 / 0 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 2.00 |
| Mutation score (measured) | 0.8 / 0 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 5.00 |
| Mix of kinds (measured) | 1 / 0 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 2.00 |
| Test-to-code ratio (measured) | 2.5 / 6 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 1.00 |
| Kills per 100 test lines (measured) | 10 / 2 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 1.00 |
| Suite run time (measured) | 10 / 120 (log) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 0.50 |
| Tests with no assertion (measured) | 0 / 0.2 (linear) | not measured / 0.00 | not measured / 0.00 | not measured / 0.00 | 0.50 |

### E Economy (8.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Production lines (measured) | 1.3 / 2.5 (linear) | 1.4 / 2.75 | 1.9 / 1.20 | not measured / 0.00 | 3.00 |
| Runtime dependencies (measured) | 2 / 6 (linear) | 0 / 1.00 | 0 / 0.80 | not measured / 0.00 | 1.00 |
| Duplication (measured) | 0.03 / 0.15 (linear) | 0.02 / 2.00 | 0.06 / 1.20 | not measured / 0.00 | 2.00 |
| Unused code (measured) | 0 / 10 (linear) | 1 / 0.90 | 3 / 0.56 | not measured / 0.00 | 1.00 |
| Unrequested work (judged) | 4 / 0 (linear) | 4 / 1.00 | 3 / 0.60 | 3 / 0.00 | 1.00 |

### F Static maintainability (7.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Lint findings (measured) | 0 / 10 (linear) | 0.5 / 1.90 | 2 / 1.28 | not measured / 0.00 | 2.00 |
| Formatting (measured) | 0 / 1 (linear) | 0 / 1.00 | 0 / 0.80 | not measured / 0.00 | 1.00 |
| Complexity (measured) | 0.02 / 0.2 (linear) | 0.03 / 1.89 | 0.08 / 1.07 | not measured / 0.00 | 2.00 |
| Hazards (measured) | 0 / 5 (linear) | 0.4 / 1.84 | 1.5 / 1.12 | not measured / 0.00 | 2.00 |

### G Readability and comments (6.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Self-evidence (judged) | 4 / 0 (linear) | 3 / 2.25 | 3 / 1.80 | 2 / 0.00 | 3.00 |
| Comment quality (judged) | 4 / 0 (linear) | 3 / 2.25 | 2 / 1.20 | 2 / 0.00 | 3.00 |

### H Ergonomics and design (6.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| API design (judged) | 4 / 0 (linear) | 3 / 1.50 | 2 / 0.80 | 2 / 0.00 | 2.00 |
| Idiom (judged) | 4 / 0 (linear) | 4 / 2.00 | 3 / 1.20 | 2 / 0.00 | 2.00 |
| Build and program contract (measured) | 1 / 0 (linear) | 1 / 2.00 | 0.6667 / 1.07 | not measured / 0.00 | 2.00 |

### I Delivery (3.00 points)

| Metric | Full / zero | Rust | Go | C# | Max |
|---|---|---|---|---|---|
| Hygiene (measured) | 1 / 0 (linear) | 0.75 / 1.50 | 1 / 1.60 | not measured / 0.00 | 2.00 |
| Claims (judged) | 4 / 0 (linear) | 3 / 0.75 | 3 / 0.60 | 1 / 0.00 | 1.00 |

## Correctness map

| Language | Build | Golden pass rate | Fuzz agreement | Invariants |
|---|---|---|---|---|
| rust | built | 98.0% | 0.995 | 1 |
| go | built | 86.0% | 0.9 | 1 |
| csharp | failed | 0.0% | not measured | not measured |

Golden sessions passed out of all, by feature group. The golden pass rate is the mean of the group rates.

| Feature group | Rust | Go | C# |
|---|---|---|---|
| amend | 25 / 25 | 25 / 25 | not measured |
| cancel | 19 / 19 | 19 / 19 | not measured |
| fok | 19 / 19 | 19 / 19 | not measured |
| iceberg | 23 / 23 | 23 / 23 | not measured |
| ioc | 18 / 20 ✗ | 12 / 20 ✗ | not measured |
| limit | 16 / 20 ✗ | 10 / 20 ✗ | not measured |
| market | 20 / 20 | 8 / 20 ✗ | not measured |
| parsing | 21 / 21 | 21 / 21 | not measured |
| post-only | 19 / 19 | 19 / 19 | not measured |
| priority | 19 / 19 | 19 / 19 | not measured |
| queries | 20 / 20 | 8 / 20 ✗ | not measured |
| rejections | 21 / 21 | 21 / 21 | not measured |
| self-trade | 21 / 21 | 21 / 21 | not measured |
| symbols | 19 / 19 | 19 / 19 | not measured |
| tick-lot | 18 / 18 | 18 / 18 | not measured |

## Tests

| Measure | Rust | Go | C# | What the value is |
|---|---|---|---|---|
| Pass and stable | not measured | not measured | not measured | 1 when the suite passes every one of test_runs runs, Go also shuffled and with the race detector, else 0 |
| Mutation score | not measured | not measured | not measured | Share of compilable sampled mutants that the language's make test target detects |
| Mix of kinds | not measured | not measured | not measured | Share of the three kinds present: unit tests of book logic, end-to-end tests through the program, invariant or property tests |
| Test-to-code ratio | not measured | not measured | not measured | Test lines per production line |
| Kills per 100 test lines | not measured | not measured | not measured | Mutants killed per 100 test lines |
| Suite run time | not measured | not measured | not measured | Seconds that one run of the suite takes, build excluded |
| Tests with no assertion | not measured | not measured | not measured | Share of tests that assert nothing |

## Size and economy

| Measure | Rust | Go | C# | What the value is |
|---|---|---|---|---|
| Production lines | 1.4 | 1.9 | not measured | Production lines without blanks, comments and tests, as a multiple of the language's reference |
| Runtime dependencies | 0 | 0 | not measured | Direct runtime dependencies |
| Duplication | 0.02 | 0.06 | not measured | Share of production lines in duplicated blocks |
| Unused code | 1 | 3 | not measured | Unused items that the compiler and analysers report |
| Unrequested work | 4 | 3 | 3 | Judge level for features outside the spec and abstraction nobody needed |

Comment density is the share of code and comment lines that hold only a comment. Above 25%, `G.comment_quality` earns at most 50% of its points.

| Language | Comment density | Over the cap |
|---|---|---|
| rust | 11.0% | no |
| go | 31.0% | yes |
| csharp | not measured |  |

## Run profile

| Measure | Value |
|---|---|
| Started | 2026-11-18T09:00:00.000Z |
| Ended | 2026-11-18T11:01:50.000Z |
| Busy time (scored) | 1h 30m 20s |
| Wall-clock time | 2h 01m 50s |
| Summed agent time | 3h 18m 00s |
| Lost to retries | 0h 00m 42s |
| Waiting for a human | 0h 00m 00s |
| Cost in Claude Code | 46.2637 USD |
| Transcripts' own total | 46.6000 USD |
| Spend outside Claude Code | 0.00 USD |
| Turns | 612 |
| API errors / retries | 2 / 2 |
| Builds / test runs | 58 / 44 |

### Tokens and cost by model

| Model | Input | Output | Thinking | Cache read | Cache write 5m | Cache write 1h | USD |
|---|---|---|---|---|---|---|---|
| claude-opus-5-5 | 4200 | 310000 | 52000 | 41000000 | 1900000 | 0 | 24.9568 |
| claude-sonnet-5-5 | 900 | 88000 | 9000 | 6200000 | 400000 | 0 | 2.5918 |

### Agent tree

| Agent | Type | Model | Tokens | Busy | Started |
|---|---|---|---|---|---|
| s1 | main | claude-opus-5-5 | 38000000 | 1h 28m 20s | 2026-11-18T09:00:00.000Z |
|   a1 | general-purpose | claude-sonnet-5-5 | 4100000 | 0h 55m 00s | 2026-11-18T09:20:00.000Z |
|   a2 | general-purpose | claude-sonnet-5-5 | 3500000 | 0h 54m 40s | 2026-11-18T09:21:00.000Z |

### Tool mix

| Tool | Calls |
|---|---|
| Agent | 2 |
| Bash | 240 |
| Edit | 130 |
| Read | 180 |
| Write | 41 |

## Judge findings

Model claude-opus-5-5, 3 judgements per item; the median counts.

| Item | Language | Level | Spread | Evidence | Reason |
|---|---|---|---|---|---|
| Unrequested work | rust | 4 of 4 | 0 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Unrequested work | go | 3 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| Unrequested work | csharp | 3 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Self-evidence | rust | 3 of 4 | 1 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Self-evidence | go | 3 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| Self-evidence | csharp | 2 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Comment quality | rust | 3 of 4 | 1 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Comment quality | go | 2 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| Comment quality | csharp | 2 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |
| API design | rust | 3 of 4 | 1 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| API design | go | 2 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| API design | csharp | 2 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Idiom | rust | 4 of 4 | 0 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Idiom | go | 3 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| Idiom | csharp | 2 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Claims | rust | 3 of 4 | 1 | rust/src/book.rs:12, DELIVERY.md:3 | synthetic fixture judgement |
| Claims | go | 3 of 4 | 1 | go/book/book.go:12, DELIVERY.md:3 | synthetic fixture judgement |
| Claims | csharp | 1 of 4 | 1 | csharp/Engine/Book.cs:12, DELIVERY.md:3 | synthetic fixture judgement |

## Validity and caveats

- Status: valid.
- Completion: finished.
- Uncommitted work at the end: yes, and it is not scored.
- Telemetry source: transcripts.
- This conclusion rests on 1 run.
- rust: category D (Tests the harness wrote) was not measured and scores 0
- go: category D (Tests the harness wrote) was not measured and scores 0
- csharp: dotnet build failed: CS0246 in Engine/Program.cs
- csharp: category A (Correctness) was not measured and scores 0
- csharp: category B (Robustness) was not measured and scores 0
- csharp: category C (Performance) was not measured and scores 0
- csharp: category D (Tests the harness wrote) was not measured and scores 0
- csharp: category F (Static maintainability) was not measured and scores 0
- csharp: did not build, so categories A, B, C, D score 0

## Reproduce

- Commands: `benchy score 5d3a9c1e7f20`, `benchy judge 5d3a9c1e7f20`, `benchy report 5d3a9c1e7f20`
- Workspace hash: `9c1f0e2a7b3d`
- Scorer image: `sha256:5e1d0c0ffee`
- Benchy version: `fixture`
