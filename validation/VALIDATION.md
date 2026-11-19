# Benchy scoring specification

This document says how benchy scores a run. `rubric.toml` holds every number; the tables inside the HTML comment markers named `rubric` below are generated from it, and `cargo test` fails when they differ. To regenerate them after a change to `rubric.toml`, run `BENCHY_BLESS=1 cargo test validation_document`.

The numbers are provisional until the pilot freezes rubric version 1.0. A scorecard records the rubric version it used, and scorecards compare only when that version matches.

Nothing in this folder is copied into a workspace. A harness sees only the task pack in `task/`.

## 1. Structure

```
Language score  (0-100, one each for Rust, Go, C#)
Quality         = mean_weight x mean of the three + lowest_weight x lowest of the three
Efficiency      (0-100) from cost and elapsed busy time
Overall         = Quality x (base + efficiency_weight x Efficiency / 100)
```

- **Headline**: Quality, cost and elapsed busy time, as three numbers with the measured run-to-run spread. Overall is shown beneath them.
- **Lowest language**: carries 40% because all three must be exceptional. It does not carry more because per-language tooling is never perfectly level (section 9).
- **Overall**: a product, so a cheap run cannot buy its way past poor code. Efficiency moves the result by at most 15%. Worked check: quality 85 at high cost (Efficiency 7) gives 73; quality 65 at very low cost (Efficiency 86) gives 64. The better engines win. A unit test of the rubric checks both figures.
- **Efficiency**: cost and elapsed busy time, equal weight, each on a log scale. Best is 0.25 times the pilot baseline's median; worst is 8 times. The values are frozen in the rubric version. Until the pilot sets the baselines, Efficiency is not computed and Overall is not shown.
- **Cost** is priced from the transcripts with `prices.toml`, tokens counted once per message id. **Elapsed busy time** is the union, across all sessions and subagents, of the intervals in which an API request or a tool is running.

<!-- rubric:formulas -->
| Key | Value | Meaning |
|---|---|---|
| `quality.mean_weight` | 0.6 | Weight of the mean of the three language scores in Quality |
| `quality.lowest_weight` | 0.4 | Weight of the lowest language score in Quality |
| `overall.base` | 0.85 | Share of Quality that Overall keeps at Efficiency 0 |
| `overall.efficiency_weight` | 0.15 | Share of Quality that Efficiency 100 adds |
| `efficiency.cost_weight` | 0.5 | Weight of cost in Efficiency |
| `efficiency.time_weight` | 0.5 | Weight of elapsed busy time in Efficiency |
| `efficiency.best_factor` | 0.25 | Multiple of the baseline median that scores 100 |
| `efficiency.worst_factor` | 8 | Multiple of the baseline median that scores 0 |
| `efficiency.baseline_cost_usd` | set by the pilot | Median cost of the pilot baseline runs, in US dollars |
| `efficiency.baseline_busy_seconds` | set by the pilot | Median elapsed busy time of the pilot baseline runs, in seconds |
| `correctness_scaling.floor` | 0.5 | Golden pass rate at or below which scaled categories earn nothing |
| `correctness_scaling.span` | 0.45 | Rise in pass rate above the floor that gives full credit |
| `correctness_scaling.categories` | C, D, E, F, G, H, I | Categories multiplied by the correctness factor |
| `build_gate.categories` | A, B, C, D | Categories that score 0 when a language does not build |
| `comment_density.density` | 0.25 | Comment density above which the cap applies |
| `comment_density.factor` | 0.5 | Share of its points that the capped metric can still earn |
| `comment_density.metric` | `G.comment_quality` | Metric that the cap applies to |
| `caps.cost_usd` | 100 | Cost in US dollars at which a headless run stops |
| `caps.wall_hours` | 4 | Wall-clock hours after which a headless run stops |
<!-- /rubric -->

## 2. Gates and scaling

| Rule | Effect |
|---|---|
| Confirmed read of benchy material | Run void. No score. |
| A language does not build from a clean checkout with the pristine `Makefile` | That language scores 0 in the categories of `build_gate.categories`. |
| Correctness scaling | For each language, the categories of `correctness_scaling.categories` are multiplied by `clamp((p - floor) / span, 0, 1)`, where `p` is the golden pass rate. Below 50% nothing else counts; full credit needs 95%. |
| Performance validity | A performance workload scores only if its output matches the reference byte for byte. A fast wrong answer earns nothing. |
| Not finished (cap reached) | Scored as committed, flagged, Efficiency scored at the cap. |

Why the scaling is this steep: with a gentler rule, an engine with icebergs and self-trade prevention entirely wrong but tidy code outscored a fully correct engine that was 60% over par on size. With this rule the broken engine scores about 60 and the correct one about 84.

## 3. Language score

<!-- rubric:categories -->
| | Category | Points | Measured | Judged |
|---|---|---|---|---|
| A | Correctness | 45 | 45 | 0 |
| B | Robustness | 5 | 5 | 0 |
| C | Performance | 8 | 8 | 0 |
| D | Tests the harness wrote | 12 | 12 | 0 |
| E | Economy | 8 | 7 | 1 |
| F | Static maintainability | 7 | 7 | 0 |
| G | Readability and comments | 6 | 0 | 6 |
| H | Ergonomics and design | 6 | 2 | 4 |
| I | Delivery | 3 | 2 | 1 |
| | Total | 100 | 88 | 12 |
<!-- /rubric -->

Every metric earns all its points at its "full at" value and none at its "zero at" value. Between them, points follow a straight line on the stated scale: `linear` on the raw value, `log` on its logarithm. Either value may be the larger, so the same rule serves metrics where less is better. Judged metrics use the judge's median level on the 0 to 4 scale as their raw value.

The settings that shape the measurements themselves:

<!-- rubric:measurement -->
| Key | Value | Meaning |
|---|---|---|
| `judge.model` | claude-opus-5-5 | Model that every judgement uses |
| `judge.samples` | 3 | Independent judgements per item; the median counts |
| `judge.scale_max` | 4 | Highest level of the judge scale, which starts at 0 |
| `measurement.fuzz_streams` | 2000 | Seeded short fuzz streams per language |
| `measurement.mutants_per_language` | 150 | Seeded mutants sampled from each core library |
| `measurement.test_runs` | 5 | Runs of each test suite for the stability check |
| `measurement.performance_runs` | 5 | Alternating runs of reference and candidate per workload |
| `measurement.min_workload_seconds` | 5 | Shortest run time of a workload on the reference |
| `measurement.max_cyclomatic` | 10 | Cyclomatic complexity above which a function counts as complex |
| `measurement.max_function_lines` | 60 | Lines above which a function counts as complex |
| `measurement.max_nesting` | 4 | Nesting depth above which a function counts as complex |
| `measurement.memory_limit_mb` | 512 | Peak memory bound for any hostile input, in MB |
<!-- /rubric -->

### A. Correctness

Hidden golden sessions are grouped by feature, and the pass rate is the mean over groups, so a large group cannot hide a missed rule. Differential fuzz runs many short, independent, seeded streams, generated per feature set, and scores the share whose output is byte-identical to the reference. Determinism and invariant checks run each input twice and check that the book is never crossed, quantity is conserved and each price level is first in, first out.

<!-- rubric:A -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `A.golden` Golden sessions | 27 | measured | Share of hidden golden sessions passed, averaged over feature groups | 1 | 0 | linear |
| `A.fuzz` Differential fuzz | 14 | measured | Share of seeded short streams whose output is byte-identical to the reference | 1 | 0 | linear |
| `A.invariants` Determinism and invariants | 4 | measured | Share of determinism and book-invariant checks passed | 1 | 0 | linear |
<!-- /rubric -->

### B. Robustness

A corpus of malformed and hostile input: no crash, the rejection the protocol states, a clean exit code and bounded memory.

<!-- rubric:B -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `B.no_crash` No crash | 2 | measured | Share of hostile inputs that the program survives | 1 | 0.5 | linear |
| `B.rejections` Correct rejection | 1 | measured | Share of hostile lines that get the rejection code the protocol states | 1 | 0.5 | linear |
| `B.exit_code` Clean exit | 1 | measured | Share of hostile inputs after which the program exits with code 0 | 1 | 0.5 | linear |
| `B.memory_bound` Bounded memory | 1 | measured | 1 when peak memory on every hostile input stays under memory_limit_mb, else 0 | 1 | 0 | linear |
<!-- /rubric -->

### C. Performance

See section 6.

<!-- rubric:C -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `C.throughput` Throughput | 3 | measured | Median run time on the throughput workloads, as a multiple of the language's reference | 1.25 | 10 | log |
| `C.scaling` Scaling | 4 | measured | Growth exponent of time per operation as the book grows | 0.25 | 1 | linear |
| `C.peak_memory` Peak memory | 1 | measured | Peak resident memory, as a multiple of the language's reference | 1.5 | 6 | log |
<!-- /rubric -->

### D. Tests the harness wrote

See section 4.

<!-- rubric:D -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `D.pass_stable` Pass and stable | 2 | measured | 1 when the suite passes every one of test_runs runs, Go also shuffled and with the race detector, else 0 | 1 | 0 | linear |
| `D.mutation` Mutation score | 5 | measured | Share of compilable sampled mutants that the language's make test target detects | 0.8 | 0 | linear |
| `D.mix` Mix of kinds | 2 | measured | Share of the three kinds present: unit tests of book logic, end-to-end tests through the program, invariant or property tests | 1 | 0 | linear |
| `D.test_ratio` Test-to-code ratio | 1 | measured | Test lines per production line | 2.5 | 6 | linear |
| `D.kill_density` Kills per 100 test lines | 1 | measured | Mutants killed per 100 test lines | 10 | 2 | linear |
| `D.suite_time` Suite run time | 0.5 | measured | Seconds that one run of the suite takes, build excluded | 10 | 120 | log |
| `D.assertion_free` Tests with no assertion | 0.5 | measured | Share of tests that assert nothing | 0 | 0.2 | linear |
<!-- /rubric -->

### E. Economy

See section 5.

<!-- rubric:E -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `E.production_lines` Production lines | 3 | measured | Production lines without blanks, comments and tests, as a multiple of the language's reference | 1.3 | 2.5 | linear |
| `E.runtime_deps` Runtime dependencies | 1 | measured | Direct runtime dependencies | 2 | 6 | linear |
| `E.duplication` Duplication | 2 | measured | Share of production lines in duplicated blocks | 0.03 | 0.15 | linear |
| `E.unused_code` Unused code | 1 | measured | Unused items that the compiler and analysers report | 0 | 10 | linear |
| `E.unrequested` Unrequested work | 1 | judged | Judge level for features outside the spec and abstraction nobody needed | 4 | 0 | linear |
<!-- /rubric -->

### F. Static maintainability

See section 7.

<!-- rubric:F -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `F.lint` Lint findings | 2 | measured | Findings of the published lint configuration per 1,000 production lines | 0 | 10 | linear |
| `F.format` Formatting | 1 | measured | Files that the formatter would change | 0 | 1 | linear |
| `F.complexity` Complexity | 2 | measured | Share of functions over max_cyclomatic, max_function_lines or max_nesting | 0.02 | 0.2 | linear |
| `F.hazards` Hazards | 2 | measured | Hazards per 1,000 production lines: bare unwrap, stubs, discarded errors, empty catch blocks, lint suppressions, unsafe | 0 | 5 | linear |
<!-- /rubric -->

### G. Readability and comments

See section 8.

<!-- rubric:G -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `G.self_evidence` Self-evidence | 3 | judged | Judge level: with comments ignored, a competent reader can follow the code | 4 | 0 | linear |
| `G.comment_quality` Comment quality | 3 | judged | Judge level: comments explain why, not what, with no narration and no over-commenting | 4 | 0 | linear |
<!-- /rubric -->

### H. Ergonomics and design

The judged part is in section 8. The measured part runs the commands of `DELIVERY.md` as written, checks that each program prints help text for `--help`, and checks that each core library does no input or output.

<!-- rubric:H -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `H.api_design` API design | 2 | judged | Judge level: the core library is hard to misuse and errors are meaningful values | 4 | 0 | linear |
| `H.idiom` Idiom | 2 | judged | Judge level: the code reads as native to its language | 4 | 0 | linear |
| `H.build_contract` Build and program contract | 2 | measured | Share of three checks passed: DELIVERY.md commands run as written, help text exists, the core library does no input or output | 1 | 0 | linear |
<!-- /rubric -->

### I. Delivery

Hygiene: work committed on the default branch, no uncommitted work, no stray files or build output, lockfiles present. Claims: the judge compares `DELIVERY.md` with what the scorer found.

<!-- rubric:I -->
| Metric | Points | Source | Measure | Full at | Zero at | Scale |
|---|---|---|---|---|---|---|
| `I.hygiene` Hygiene | 2 | measured | Share of four checks passed: work committed on the default branch, no uncommitted work, no stray files or build output, lockfiles present | 1 | 0 | linear |
| `I.claims` Claims | 1 | judged | Judge level: DELIVERY.md matches what the scorer measured | 4 | 0 | linear |
<!-- /rubric -->

## 4. Tests

The question is whether the tests would catch a bug, and whether they cost more than they are worth. `TASK.md` does not prescribe test types; choosing the mix is part of what is measured.

- **Pass and stable**: `make test-<lang>` passes every one of `measurement.test_runs` runs; Go also passes once more with shuffled order and the race detector. Any failure or flake scores 0.
- **Mutation score**: share of injected bugs the suite detects.
- **Mix**: presence of each kind, detected from test code: unit tests on book logic; end-to-end tests through the program; invariant or property tests (the fitness functions: book never crossed, quantity conserved, first-in first-out within a price level, determinism).
- **Economy**: test lines to production lines with no lower bound, mutants killed per 100 test lines, suite run time, and tests with no assertion.
- **No tests**: a language with no test that the scorer can find scores 0 in every metric of this category, so an empty suite earns nothing for being fast, short or free of assertion-free tests.

How mutation testing is made fair and repeatable:

- **One mutator for all three languages**, built into benchy, with one operator set: flip a comparison (`<` and `>`, `<=` and `>=`, `==` and `!=`), shift a boundary by one (`<` and `<=`, `>` and `>=`), swap an arithmetic operator (`+` and `-`, `*` and `/`, also in compound assignments), negate the condition of an `if` that fits on one line, and change a constant (an integer literal by one, 1 to 0, `true` and `false`). Operators are found in code with comments and strings blanked, and binary operators only with a space on each side, as all three formatters write them, so generics, arrows, pointers and signs are never mutated. The three off-the-shelf tools inject different bugs, so their scores would not compare.
- **A seeded sample of mutants per language** (`measurement.mutants_per_language`), drawn with a fixed seed from the production lines of the core library, so the same source always gives the same sample. Comments add no candidate, so a comment never moves the sample. No time budget, so the score does not depend on machine speed.
- **Dead code is left out.** Lines inside an item that the compiler or analysers report as unused give no mutant: no test can catch a change to code that never runs, so dead code costs points in economy and nowhere else.
- **The kill command is `make test-<lang>`**, which rebuilds first. End-to-end tests that run the program therefore see the mutant. The scorer runs `make build-<lang>` first: a mutant that does not build says nothing about the tests and is left out. A mutant whose tests run five times longer than the unmutated suite's first run, and at least three minutes, counts as caught, as a suite run with a time limit would report it.
- **Score**: killed over mutants that build. The mutants run only when the unmutated suite passes every stable run, because a failing suite would catch every mutant.

How the kinds and assertions are read from test code:

- A test is a Rust function marked `#[test]` (or in a `proptest!` block), a Go `TestXxx` or `FuzzXxx` function in a `_test.go` file, or a C# method marked `[Fact]`, `[Theory]`, `[Test]`, `[TestMethod]` or `[TestCase]`. A test reaches the helpers it calls, by name, through any depth.
- **Invariant or property**: the test or its helpers use a property-testing library, or a random or seeded source inside a loop.
- **End-to-end**: otherwise, the test starts the program as a process, or calls the program's own code: Rust test code in the program crate, a Go test in package `main`, or a C# type declared in the program project.
- **Unit**: otherwise, the test calls the core library: Rust test code in the library or a test file that names the library crate, a Go test outside package `main`, or a C# type declared in the core library.
- **Asserts**: the test or one of its helpers holds an assertion: `assert!` and its kin or a panic in Rust, `t.Error`, `t.Fatal` and their kin or a panic in Go, `Assert.`, `Should` or a `throw` in C#. A Rust test marked `#[should_panic]` asserts.
- **Suite run time** is the median of the stable runs after the first, whose builds have nothing left to do.

Coverage is reported, not scored: mutation score answers the same question better, and branch coverage is not available evenly across the three toolchains. Test-only dependencies are free, so a property-testing library costs nothing. A small property test that kills most mutants scores full marks; there is no reward for bulk.

## 5. Economy: more code is worse

- **Production lines** are counted without blanks, comments and tests, against the reference solution for that language. There is no bonus for less than par.
- **Runtime dependencies**: zero is possible by design.
- **Duplication** is measured by one clone detector for all three languages.
- **Dead weight**: measured, unused code reported by the compiler and analysers; judged, features outside the spec and abstraction with one implementation and no reason.

Par comes only from the reviewed reference solutions, never from the spread of runs under test. Otherwise scoring would be relative by the back door.

## 6. Performance

Measured through standard input and output, with care that it measures the engine and not the plumbing:

- **Per-language reference.** Each language is compared with its own reference solution, so C# is not judged against Rust on start-up or memory.
- **Workloads run at least `measurement.min_workload_seconds`** on the reference. Start-up, measured on empty input, is subtracted.
- **Alternating runs** (`measurement.performance_runs`) of reference and candidate in the same session; the median counts and the spread is reported.
- **Inside the scorer container** the workspace is copied in, not mounted, to avoid file-sharing noise.
- **The scaling probe carries the most weight** because it is independent of the machine: time per operation is measured at growing book sizes, and the growth exponent is scored. It exposes a linear-time cancel or insert, which is the design error that matters.

## 7. Static maintainability

| Language | Lint | Format |
|---|---|---|
| Rust | `cargo clippy` with the arguments in `task/lint/rust/clippy.txt` | `cargo fmt --check` with `task/lint/rust/rustfmt.toml` |
| Go | `go vet` and `staticcheck` with the checks in `task/lint/go/staticcheck.txt` | `gofmt -l` |
| C# | Built-in analysers with the settings in `task/lint/csharp/analyzers.rsp`, nullable enabled | `dotnet format whitespace --verify-no-changes` |

- **Curated, published and calibrated.** The lint sets ship in the task pack, and `make lint-<lang>` runs them. They are tuned so each reference solution scores at least 95 in its own language; no language is marked harder by its tooling. The scorer uses its own pristine copy of `lint/`, so editing the workspace copy changes nothing.
- **Dead code is scored once, in economy.** Lint findings leave out the unused items that the compiler and analysers report, the share of complex functions leaves out unused functions, and the per-1,000-line rates leave their lines out, so dead code neither costs points here nor dilutes a rate into a better score.
- **Complexity** uses one tool for all three languages: per-function complexity, length and nesting, against the limits in the measurement table.
- **Hazards** count only what is unambiguously worse: `unwrap()` with no message, `todo!` and unimplemented stubs, discarded errors, empty catch blocks, lint suppressions, `unsafe`. An `expect` with a message that states an invariant is good practice and is not counted.

## 8. Readability, comments and the judge

Two measured comment metrics from the first draft are gone. A density floor forced filler onto self-evident code, and "share of public items documented" paid for `/// Returns the price` on `price()`. One measured rule remains: comment density above `comment_density.density` caps the comment-quality score at `comment_density.factor` of its points. Comment density is the share of code and comment lines in production files that hold only a comment; blank lines and test code are left out.

The judge is Claude, run by `benchy judge` with a fixed model, fixed prompt and structured output. The judge model is part of the rubric version, and workspace tarballs are kept so old runs can be judged again under a new one.

- It sees the committed code and `DELIVERY.md` only: no transcript, no harness name, no scores.
- Each item has a 0 to 4 scale with a written anchor per level. Anchors derive from published guides (the Rust API Guidelines, Effective Go, the .NET design guidelines), not from one team's house style.
- It must cite `file:line` evidence for each score.
- Independent judgements (`judge.samples`); the median counts and the spread is reported.
- Calibration samples with known scores run as a CI check on the judge prompt. They come from no harness under test.

| Item | Metric | Asks |
|---|---|---|
| Self-evidence | `G.self_evidence` | With comments ignored, can a competent reader follow the code? Are names exact? Do types carry meaning (price, quantity and order id as distinct types)? |
| Comment quality | `G.comment_quality` | Do comments explain why, not what? Any narration, stale notes, or chatter about the editing process? Is the code over-commented? |
| API design | `H.api_design` | Is the core library hard to misuse? Are illegal states unrepresentable? Are errors meaningful values? |
| Idiom | `H.idiom` | Does each implementation read as native to its language, or as one language transliterated into another? |
| Unrequested work | `E.unrequested` | Features outside the spec; abstraction nobody needed. |
| Claims | `I.claims` | Does `DELIVERY.md` match what the scorer measured? |

Known limit: a harness's style rules leave fingerprints (file-header comments, marker comments), so the judge is not perfectly blind. Several samples of one model reduce noise, not bias. This is why the judge holds 12 points and no more.

## 9. Keeping three languages level

If one language always scores lowest because its tools are harsher, the "lowest language" term measures tooling, not the harness. Three defences:

- A reviewed reference solution per language; thresholds tuned so each scores at least 95.
- One mutator, one clone detector and one complexity tool across all three.
- A pilot check: the lowest language must not be the same one in more than 70% of runs for tooling reasons.

## 10. Statistical validity

Agents are stochastic, so a difference means nothing without a noise floor.

- The pilot runs the baseline at least 5 times and publishes the standard deviation of Quality, cost and elapsed time here.
- Every report prints scores with that spread.
- A single-run report says "indistinguishable from baseline" when the difference is under 2 standard deviations.
- "Harness A beats harness B" needs at least 3 runs each with every A run above every B run, or 5 runs each with a rank test.

| Measure | Baseline standard deviation |
|---|---|
| Quality | not measured yet (pilot) |
| Cost | not measured yet (pilot) |
| Elapsed busy time | not measured yet (pilot) |

## 11. Every factor, in one list

- **Correctness**: golden pass rate per feature group, fuzz agreement, determinism, invariants, agreement across the three languages.
- **Robustness**: crash rate on bad input, rejection accuracy, exit codes, memory bound.
- **Performance**: throughput, scaling exponent, peak memory, start-up time (reported).
- **Tests**: pass rate, flakiness, mutation score, kinds present, test-to-code ratio, kills per 100 test lines, run time, assertion-free tests, coverage (reported).
- **Size**: production lines, test lines, comment lines, files, runtime dependencies.
- **Waste**: duplication, unused code, needless abstraction, unrequested features, stray files.
- **Maintainability**: lint findings, formatting, complexity, function length, nesting, hazards, suppressions.
- **Readability**: naming, self-evidence, comment quality, over-commenting.
- **Ergonomics**: API design, idiom, accurate build instructions, help text, library and program separation.
- **Delivery**: committed work, repo hygiene, lockfiles, claims accuracy.
- **Efficiency (scored)**: cost, elapsed busy time.
- **Run profile (reported)**: wall time, agent time, tokens, cache share, agents, depth, concurrency, turns, tool mix, retries, operator effort.

## 12. Self-test

`benchy selftest` makes six workspaces from the reference solutions, scores each in the categories its change can touch, and checks that the rubric orders them as follows. Judged items come from the stored levels in `validation/selftest/judged.toml`, so the self-test makes no paid call.

- Each reference solution scores at least 95.
- The reference with its tests deleted loses only category D.
- The reference padded with twice its code as dead code loses only category E.
- The reference with tautological tests earns under 1 of the 5 mutation points.
- The reference with self-trade prevention broken scores at least 15 points below its reference, in each language.
- The reference with a filler doc comment on every public item gains nothing.

With the provisional correctness scaling, one broken feature costs about 20 points in the self-test, and about 12 when the harness's own tests still pass. The design target of a total under 65 is not met, and the scaling is a decision for the pilot.
