---
name: benchy-sessions
description: >
  Find the sessions of a benchy run and read anything from their transcripts:
  layout, record types, recipes, checked facts.
when_to_use: >
  Use when the operator asks about a run that the telemetry does not answer:
  "which agent wrote the C# tests", "why did the run stall", "why is the cost
  gap high", "read the transcripts", "find the sessions of run <id>".
---

# Benchy sessions

Words:

- **Archive**: `.runs/<run-id>/transcripts/` in the benchy checkout. `benchy finish` copies the run's `config/projects/` here.
- **Reply**: one API response. Claude Code writes one line per content block, so one reply = many lines with one `message.id`.

## Find the sessions

1. Read `.runs/<run-id>/run.json`. Field `run_dir` = run directory.
2. IF run is finished, THEN read the archive. ELSE read `<run_dir>/config/projects/`.
3. Every transcript there belongs to the run. The run has its own configuration, so no other session can be there.

CAUTION: Transcripts can hold private data. Do not copy their text into `results/`, commits or reports. Quote numbers and short facts only.

## Layout

| Path under `projects/` | Holds |
| --- | --- |
| `<cwd with / as ->/<session-id>.jsonl` | Main session. Depth 0. |
| `<session-id>/subagents/agent-<id>.jsonl` | One subagent. `<id>` can carry a name: `agent-<name>-<hash>`. |
| `<session-id>/subagents/agent-<id>.meta.json` | `agentType`, `description`, `model`, `spawnDepth`, `parentAgentId`, `toolUseId`, `finished`. |
| `<session-id>/tool-results/` | Large tool outputs, kept out of the transcript. |

## Record types

Each line = one JSON record. Field `type` names it. Most records have `timestamp`, `cwd`, `sessionId`, `version`. Subagent records also have `agentId` and `isSidechain: true`.

| `type` | Read it for |
| --- | --- |
| `user`, content = string | A prompt. `isMeta: true` = written by Claude Code. `origin.kind` = `task-notification`, `coordinator`, `peer` or `plugin` = not the operator. |
| `user`, content = list with `tool_result` | Result of tool call `tool_use_id`. Its `timestamp` = tool end. |
| `api-request` | Request start. Its `id` = `requestRef` of the reply lines. |
| `assistant` | Reply line. `message.id`, `message.model`, `message.usage`, `message.content` (`thinking`, `text`, `tool_use` with `id`, `name`, `input`), `message.stop_reason`. |
| `assistant` with `isApiErrorMessage: true` | Failed request. `message.model` = `<synthetic>`, `error` = `rate_limit`, `authentication_failed`, `server_error`, `invalid_request`. |
| `cost-state` | Claude Code's own session totals: `totalCostUSD`, `totalAPIDuration`, `totalAPIDurationWithoutRetries`, `totalToolDuration`, `totalDuration`, `totalLinesAdded`, `totalLinesRemoved`, `modelUsage` per model. Only in main session files. Use the last one. |
| `system` | `subtype` = `turn_duration`, `compact_boundary`, `stop_hook_summary`, `agents_killed`, `informational`, others. |
| `attachment` | Context that Claude Code added: `skill_listing`, `agent_listing_delta`, `budget_usd`, `session_context`, others. |

Usage fields in `message.usage`: `input_tokens`, `output_tokens`, `cache_read_input_tokens`, `cache_creation_input_tokens`, `cache_creation.ephemeral_5m_input_tokens`, `cache_creation.ephemeral_1h_input_tokens`, `output_tokens_details.thinking_tokens`.

## How the collector counts

- [ ] Cost: each `message.id` counted once across all files, priced with `validation/prices.toml`. Output tokens are split into thinking and the rest, so thinking is not billed twice.
- [ ] Reply time: from its `api-request` (else from the record before it) to its last line.
- [ ] Tool time: from the `tool_use` line to its `tool_result`.
- [ ] Busy time (scored) = union of reply and tool time of all agents. Agent time = sum per agent.
- [ ] Human wait: from the record before an operator prompt to that prompt. The first prompt = kickoff, not counted.
- [ ] Retry time: `totalAPIDuration - totalAPIDurationWithoutRetries` of each session.

## Recipes

Run Python with `-I`. Give the path as an argument.

Tool calls per agent:

```sh
python3 -I -c '
import json, sys, glob, collections
for f in sorted(glob.glob(sys.argv[1] + "/**/*.jsonl", recursive=True)):
    tools = collections.Counter()
    for line in open(f):
        r = json.loads(line)
        if r.get("type") == "assistant":
            for b in r["message"]["content"]:
                if b.get("type") == "tool_use": tools[b["name"]] += 1
    print(f.rsplit("/", 1)[-1], dict(tools))
' .runs/<run-id>/transcripts
```

Gaps over 5 minutes in one file (stalls):

```sh
python3 -I -c '
import json, sys
from datetime import datetime
last = None
for line in open(sys.argv[1]):
    r = json.loads(line); t = r.get("timestamp")
    if not t: continue
    now = datetime.fromisoformat(t.replace("Z", "+00:00"))
    if last and (now - last).total_seconds() > 300: print(last, "->", now, r["type"])
    last = now
' <transcript.jsonl>
```

Which agent wrote a file: search `tool_use` blocks with `name` = `Write` or `Edit` and `input.file_path` that ends with the file name. The file name gives the agent.

IF the collector fails on a new shape, THEN find the line it names, compare it with the tables here, and enter values by hand (skill `benchy-run`, step 3).

## Checked facts

Phase 0 of the plan asks for facts that only a real run can confirm. One toy run was approved: run `78be49536da6`, Claude Code 2.1.327, model `claude-haiku-5-5`, relocated configuration, `--max-budget-usd 2`, subscription login.

| Fact | Status | Evidence |
| --- | --- | --- |
| Login works with a relocated configuration directory | Confirmed: it does not | Toy run ended at its first request with `Not logged in · Please run /login`, `terminal_reason` `api_error`, cost 0. `claude auth status` with the run's `CLAUDE_CONFIG_DIR` gives `loggedIn: false`. Without it: `loggedIn: true`. |
| `CLAUDE_CODE_OAUTH_TOKEN` or `ANTHROPIC_API_KEY` logs a relocated run in | Unconfirmed | No token or key was on the host, and no second paid run was approved. Benchy passes the variable that the manifest's auth mode names. |
| Relocated configuration hides the operator's agents, skills and memory | Confirmed | Toy run saw 5 agent types and 13 skills, all built in. The operator has user agents and 11 user skills. None were listed. No memory was attached. |
| The operator's global git identity reaches the run | Confirmed, then fixed | Toy run context named the operator's git user. Benchy now sets `GIT_CONFIG_GLOBAL` to `config/gitconfig` with a neutral user. The fix is checked with `git config`, not with a paid run. |
| Headless launch with a plugin | Unconfirmed | Not in the approved run. |
| Usage counted once per `message.id` | Unconfirmed by the toy run | The toy run made no reply. Other evidence: a local 2.1.326 session has 389 reply lines with 175 ids. All 214 repeats carry the same usage. |
| Session totals include subagents | Unconfirmed by the toy run | Other evidence: in that session, `modelUsage` of the main model equals the deduplicated sum of the main file and its 10 subagent files: input 312, output 106,680, cache read 9,792,217, cache write 552,806. |
| Session totals also hold calls with no transcript | Seen in local evidence | In that session, side models show more tokens in `modelUsage` than in any transcript, for example input 53,782 against 66. So the transcript total can be higher than the collector's cost. |
| Collector cost within 1% of Claude Code's own figure | Unconfirmed | Toy run spent 0, so both totals were 0. Also: `validation/prices.toml` is provisional, and Claude Code's own prices differ. A fit of local `cost-state` records gives `claude-fable-5-2` at 8 / 40 / 0.2 / 10 / 16 USD per million (input, output, cache read, 5-minute write, 1-hour write). The file has 5 / 25 / 0.5 / 6.25 / 10. |
| `output_tokens` includes thinking tokens | Unconfirmed | The collector assumes it, as the API documents. `modelUsage` gives thinking apart from output. |
| A failed request is an `assistant` record with model `<synthetic>` | Confirmed | Toy run: `isApiErrorMessage: true`, `error` = `authentication_failed`. |
| `cost-state` is written even when the session fails | Confirmed | Toy run: one `cost-state`, `totalCostUSD` 0. |
| `claude -p --output-format json` says `subtype: success` on an error | Confirmed | Toy run: `subtype` `success`, `is_error` true. Benchy reads `is_error` and `result`. |
| Busy time from `api-request` and reply lines | Shape confirmed, timing unconfirmed | Toy run wrote `api-request` before its reply. Local sessions: `api-request.id` = `requestRef` of the reply lines. |
