# benchy

Benchy measures a Claude Code harness on one fixed task and reports what it built, what it cost and how long it took. A harness is plain Claude Code, or Claude Code with a plugin, tool servers or settings on top. Every harness gets the same instruction file and nothing else.

The task: build the same order-matching engine three times, in Rust, Go and C#, in one workspace. The three programs must print identical output for identical input.

## What you get

One run produces three files in `results/<harness>/<run-id>/`:

| File | Content |
|---|---|
| `report.md` | Scoreboard, verdict, a table for every category and metric, correctness by feature group, run profile, judge findings, caveats |
| `scorecard.json` | Every raw value, score and version |
| `scorecard.tsv` | One header line and one data line. Paste the rows of several harnesses under each other in a spreadsheet to compare them |

Each language is scored out of 100: correctness 45, robustness 5, performance 8, tests 12, economy 8, static quality 7, readability and comments 6, ergonomics and design 6, delivery 3. Quality is 0.6 times the mean of the three languages plus 0.4 times the lowest one, so a harness cannot hide one weak language. Cost, busy time and the number of agents are reported beside it.

## Requirements

- Rust 1.97 or later, to build benchy
- Docker, for scoring. The scorer image holds the pinned Rust, Go and .NET toolchains
- Claude Code, at the version the harness manifest names
- Go 1.27.4 and the .NET SDK 10.0.404 on the host, so the harness can build and test its own work
- `ANTHROPIC_API_KEY` for a manifest with `auth = "api-key"`, or `CLAUDE_CODE_OAUTH_TOKEN` (from `claude setup-token`) for `auth = "subscription"`. A run uses its own empty Claude Code configuration, which has no login of its own

## Run a harness

```sh
git clone https://github.com/coderaidershaun/benchy && cd benchy
cargo install --path .

benchy doctor harnesses/claude-code.toml   # checks toolchains, Docker, Claude Code and login
benchy run harnesses/claude-code.toml      # launch, finish, score, judge and report in one go
```

`benchy run` prints the run id and the path of the report. It stops the harness at 100 US dollars or 4 hours; change that with `--max-budget-usd` and `--time-limit-minutes`. Scoring takes about an hour and a quarter, most of it mutation testing of the harness's tests.

A harness that needs a person at the keyboard runs in steps:

```sh
benchy new harnesses/<name>.toml   # prints the shell line and the one message to send
benchy finish <run-id>             # snapshot the committed work, collect cost, time and agents, audit
benchy score <run-id>              # build and measure in the scorer image
benchy judge <run-id>              # three judgements of each judged item, median taken
benchy report <run-id>             # report.md, scorecard.json, scorecard.tsv
```

Run every command from inside the checkout. The harness works in `~/runs/<run-id>/ws`, outside the checkout, so it cannot see the scoring rules or the hidden tests.

## Add a harness

Copy `harnesses/super-rex.toml`, change the label and the plugin, and keep the Claude Code version, model and auth mode of `harnesses/claude-code.toml`. Two scorecards compare only when task, rubric, Claude Code version and model match.

## Layout

| Path | Who reads it | Content |
|---|---|---|
| `task/` | The harness | `TASK.md`, the protocol and semantics, nine examples, the fixed `Makefile` and lint settings. This folder is the whole workspace at the start of a run |
| `validation/` | The operator only | `VALIDATION.md` (how every point is earned), `rubric.toml`, `prices.toml`, the three reference engines, 304 golden sessions, workloads, the judge prompt and the scorer image |
| `harnesses/` | The operator | One manifest for each harness |
| `.claude/skills/` | Claude Code, for the operator | `benchy-run`, `benchy-sessions` (find the sessions of a run and read cost, time and agents from them), `benchy-score` |
| `src/` | | The `benchy` program |
| `.runs/`, `results/` | The operator | Raw data and results of each run |

Never copy anything from `validation/` into a run directory.

## Status

The rubric is provisional (version 0.1.0). Its thresholds, the efficiency score and the overall score wait for a pilot: the control harness run five times to set baselines and the noise floor.

`benchy selftest` checks the rubric itself. It makes six workspaces from the reference engines (the references, their tests deleted, twice the code as dead code, tautological tests, self-trade prevention broken, a filler doc comment on every public item), scores each in the scorer image in the categories it can change, and prints one line for each ordering the rubric must hold: each reference scores at least 95, deleted tests lose only tests points, dead code loses only economy points, tautological tests earn under 1 mutation point, a broken feature scores at least 15 points below its reference, and filler doc comments gain nothing. It makes no paid call: the judged items come from the stored levels in `validation/selftest/judged.toml`. It takes about two hours, most of it mutation testing; `--reference-metrics <metrics.json>` reuses a score of the reference workspace and saves the longest step.
