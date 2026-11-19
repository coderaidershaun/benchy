---
name: benchy-run
description: >
  Operator walk-through of one benchy run: check host, create isolated run,
  launch or hand over Claude Code, finish, read telemetry and audit.
when_to_use: >
  Use when the operator says "run benchy", "benchmark a harness", "start a
  benchy run", "finish the run", "benchy new", "benchy finish", or asks how to
  measure a Claude Code harness with benchy.
---

# Benchy run

Words:

- **Run root**: folder that holds run directories. Default `~/runs`. Change it with `--run-root <dir>` or `BENCHY_RUN_ROOT`.
- **Run directory**: `<run root>/<run-id>/`. It holds `ws/` (workspace) and `config/` (clean Claude Code configuration) only.
- **Operator data**: `.runs/<run-id>/` in the benchy checkout. Never in reach of the harness.

Run every `benchy` command from inside the benchy checkout.

## 1. Check the host

1. Run `benchy doctor harnesses/<name>.toml`.
2. IF a line says `FAIL`, THEN fix it before you continue.
3. IF auth line fails for `subscription`, THEN run `claude setup-token` and export `CLAUDE_CODE_OAUTH_TOKEN`. A relocated configuration has no login of its own.
4. IF auth line fails for `api-key`, THEN export `ANTHROPIC_API_KEY`.
5. IF `inherited` fails, THEN move that `CLAUDE.md` or choose another run root. Claude Code reads it in every run.

CAUTION: Do not put the run root inside the benchy checkout. Benchy refuses it, because the harness could then read scores and hidden tests.

## 2a. Headless harness (default)

1. Run `benchy run harnesses/<name>.toml`.
2. Caps come from `validation/rubric.toml`. IF you want other caps, THEN add `--max-budget-usd <usd>` and `--time-limit-minutes <min>`.
3. Benchy creates the run, starts `claude -p` with the kickoff message, waits, then finishes the run.
4. IF output says `claude reported an error`, THEN read `.runs/<run-id>/claude-output.json`. IF error is `Not logged in`, THEN go to step 1.3.

## 2b. Interactive harness

1. Run `benchy new harnesses/<name>.toml`.
2. Paste the printed shell line. It starts Claude Code with the run's environment only.
3. Send the printed kickoff message. Send nothing else.
4. Reply only with `Use your best judgement and TASK.md.` or `Approved, continue.` Each reply is counted.
5. WHEN harness reports done, exit Claude Code.
6. Run `benchy finish <run-id>`.

## 3. Finish

`benchy finish <run-id>`:

- [ ] Refuses while a process of the run is alive. Stop it, then run `finish` again.
- [ ] Records the committed head of `main` and lists uncommitted paths. Uncommitted work is not scored.
- [ ] Writes `workspace.tar`, `transcripts/`, `telemetry.json`, `audit.json` and `run.json` in `.runs/<run-id>/`.

IF `finish` says the telemetry could not be collected, THEN:

1. Read the cause. It names the file, line and Claude Code version.
2. Write `.runs/<run-id>/manual.toml` with at least `cost_usd` and `busy_seconds`. Optional: `wall_seconds`, `agent_seconds`, `[[agent]]` tables.
3. Run `benchy finish <run-id>` again. Telemetry is then marked `manual`, and the report flags it.

To correct one collected value, write only that value in `manual.toml`. Telemetry is then marked `mixed`.

## 4. Review

- [ ] `cost gap` line: IF gap is over 1%, THEN find the cause with skill `benchy-sessions` before you trust the cost.
- [ ] `audit` line: read every finding. IF a finding is `Benchy` and the agent read benchy material, THEN the run is void.
- [ ] `the run hit a cap`: run is not finished. It is scored as committed and flagged.
- [ ] Scoring steps (`score`, `judge`, `report`) come after `finish`.

CAUTION: Do not commit anything from `.runs/`. Raw transcripts can hold private data. Only derived numbers go in `results/`.
