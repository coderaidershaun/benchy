---
name: benchy-score
description: >
  Score, judge and report one finished benchy run, read the scorecard, and
  handle anomalies: void runs, failed builds, invalid judge output.
when_to_use: >
  Use when the operator says "score the run", "judge the run", "write the
  report", "benchy score", "benchy judge", "benchy report", or asks why a
  score is low or a report looks wrong.
---

# Benchy score

Words:

- **Operator data**: `.runs/<run-id>/` in the benchy checkout.
- **Measured scores**: `.runs/<run-id>/metrics.json`, written by `benchy score`. Points before scaling and gates.
- **Judged scores**: `.runs/<run-id>/judge.json`, written by `benchy judge`.
- **Results**: `results/<label>/<run-id>/` with `report.md`, `scorecard.json`, `scorecard.tsv`. Commit these only.

Run every command from inside the benchy checkout. `benchy run` does all steps. Use the steps below for an interactive run or to repeat one step.

## Steps

1. Run `benchy finish <run-id>` (skill `benchy-run`).
2. Run `benchy score <run-id>`. It writes `metrics.json`.
3. Run `benchy judge <run-id>`. It calls `claude -p` 3 times with the judge model of `validation/rubric.toml`.
4. Run `benchy report <run-id>`. It prints Quality, cost, busy time, Overall and the path of `report.md`.
5. Read `report.md`, sections Verdict and Validity and caveats first.

CAUTION: `benchy judge` makes paid calls. Do not run it in a loop.

## How the totals are made

- [ ] Build gate: IF a language does not build, THEN its categories A to D score 0.
- [ ] Correctness factor = `clamp((p - 0.5) / 0.45, 0, 1)`, `p` = golden pass rate. Categories C to I are multiplied by it.
- [ ] Judged item points = level / 4 × item points. The median of 3 judgements counts.
- [ ] Quality = 0.6 × mean of the 3 languages + 0.4 × lowest.
- [ ] Efficiency and Overall stay `absent` UNTIL the pilot writes `baseline_cost_usd` and `baseline_busy_seconds` in `validation/rubric.toml`.
- [ ] IF a category has no metric in `metrics.json`, THEN it scores 0, and the caveats name it as not measured.

## Self-test

- [ ] Run `benchy selftest` from inside the checkout. It scores six synthetic workspaces in the scorer image and makes no paid call.
- [ ] Read one line for each ordering: `ok` or `FAILED`, then the numbers it compared. Exit code 0 means every ordering holds.
- [ ] The six orderings: each reference scores at least 95; tests deleted loses only D; twice the code as dead code loses only E; tautological tests earn under 1 of the 5 mutation points; self-trade prevention broken scores at least 15 points below its reference; a filler doc comment on every public item gains nothing.
- [ ] IF you have a fresh `metrics.json` of `benchy score --reference`, THEN pass it with `--reference-metrics <path>` to skip the longest step.
- [ ] IF an ordering fails, THEN read `score.log` beside each `metrics.json` under `--scratch <dir>`, and report the numbers. Do not change the rubric to make it pass.

## Category D facts

| Fact | Status |
| --- | --- |
| Mutants come from live lines of the core library, with one fixed seed; unused items give none | Confirmed by the code and its tests |
| A mutant that does not build is left out; one whose tests hang counts as caught | Confirmed by the code |
| Mutants run only when the unmutated suite passes every stable run | Confirmed by the code |
| A language with no test that the scorer finds scores 0 in all of D | Confirmed by the code |

## Anomalies

| Report says | Do |
| --- | --- |
| Validity `needs-review` | Read `.runs/<run-id>/audit.json`. Write `.runs/<run-id>/review.toml` with `status = "valid"` or `status = "void"` and `note = "<why>"`. Run `benchy report` again. |
| Validity `void` | The harness read benchy material. The run has no score. Keep the results, and do not compare the run. |
| `did not build, so categories A, B, C, D score 0` | Read the build note of that language in the caveats. Rebuild from a clean checkout of `workspace.tar` to confirm. |
| `not judged: every judged item scores 0` | Run `benchy judge <run-id>`, then `benchy report <run-id>`. |
| `entered by hand` | Telemetry came partly from `manual.toml`. Say so when you share the result. |
| Judge stops with `gave no valid answer in 3 attempts` | Read the reason. IF the model cites lines that do not exist, THEN run `benchy judge` again one time. IF it fails again, THEN report the run as not judged. |
| `metrics.json` is missing | Run `benchy score <run-id>`. |

## Judge facts

| Fact | Status |
| --- | --- |
| The judge sees only `rust/`, `go/`, `csharp/` and `DELIVERY.md`, unpacked outside the checkout | Confirmed by the code. Tested with a stand-in program. |
| A reply with no citation, or a citation of a line that does not exist, is rejected and asked again, max 3 attempts | Confirmed by tests with a stand-in program. |
| Calibration samples in `validation/judge/calibration/` give back their known levels through the pipeline | Confirmed by a test with no paid call. This checks the pipeline, not the model. |
| Repeat judging of one workspace varies by at most one level per item (plan phase 5) | Unconfirmed. It needs paid judge calls, and none was approved. |
| The judge model gives the known levels on the calibration samples | Unconfirmed, for the same reason. |
| The judge runs with the operator's Claude Code configuration | Known limit. Its memory or instructions can reach the judge. A clean judge configuration needs a login token. |
