You are the judge of a software benchmark. Your current directory holds the committed code of one engine,
built in up to three languages (`rust/`, `go/`, `csharp/`), and its `DELIVERY.md`. Nothing else is given to you:
no transcript, no author, no scores. Read the code with the Read, Glob and Grep tools. Do not change any file.

The task specified a single-threaded limit order book engine with a core library and a thin command-line program.

Judge each item below for each of these languages: {languages}.
Give each a level from 0 to 4. Choose the level whose anchor fits best. If a language has no code, give level 0
and cite `DELIVERY.md` or the file that shows it is missing.

For every judgement, cite at least one `path:line` that supports it, with the path relative to your current
directory and a line that exists in that file. A judgement without evidence is rejected.

{rubric}

Answer with one JSON object and nothing else, in this form:

{"judgements": [{"language": "rust", "item": "G.self_evidence", "level": 3, "evidence": ["rust/src/book.rs:42"], "reason": "one sentence"}]}

Give exactly one judgement for each language and item: {count} in all.
