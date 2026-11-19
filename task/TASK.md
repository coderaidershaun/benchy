# Task: one matching engine, three languages

Build a limit order book matching engine three times: once in Rust, once in Go and once in C#. Each of the three must be work you would be proud to hand to a strong team in that language.

## What to build

A single-threaded, deterministic matching engine for several symbols, with:

- price-time priority, integer prices and quantities, and tick and lot validation;
- limit orders that rest (`GTC`), market orders, immediate-or-cancel, fill-or-kill and post-only orders;
- cancel, and amend with its priority rules;
- self-trade prevention;
- iceberg orders;
- rejections with fixed reason codes, and top-of-book and depth queries.

Two documents define it completely:

- `spec/protocol.md` fixes every byte the program reads and writes.
- `spec/semantics.md` gives every matching rule, each with a worked example.

The three programs must write byte-identical output for the same input. `examples/` holds public sessions: each `NAME.in` is an input and `NAME.out` is its exact expected output. Together they show every command, every event and every rejection code at least once. They are not a full test of the rules.

In each language, build the engine as a core library that does no input or output, and a thin command-line program around it that reads standard input and writes standard output.

## Where the code goes

| Language | Location | Required |
|---|---|---|
| Rust | `rust/` | `rust/Cargo.toml`, and a binary target named `engine`. `rust/Cargo.lock` is committed. |
| Go | `go/` | `go/go.mod`, and the program's `main` package in `go/cmd/engine`. `go/go.sum` is committed if the module has dependencies. |
| C# | `csharp/` | Exactly one solution file directly in `csharp/` that holds every C# project, and the program project at `csharp/Engine/Engine.csproj`, targeting `net10.0`. A project that uses NuGet packages sets `RestorePackagesWithLockFile` and commits its `packages.lock.json`. |

Inside each location, arrange the library, the program and the tests as is natural for that language.

## Building and testing

The `Makefile` has fixed targets. Do not edit it: the scorer builds every language with its own copy of this `Makefile` and this `lint/` folder, from a clean checkout.

| Target | Does |
|---|---|
| `make build-rust`, `make build-go`, `make build-csharp` | Builds the release program to `bin/rust/engine`, `bin/go/engine` or `bin/csharp/Engine`. |
| `make test-rust`, `make test-go`, `make test-csharp` | Builds, then runs every test of that language. |
| `make lint-rust`, `make lint-go`, `make lint-csharp` | Checks formatting, then runs the linter with the configuration in `lint/`. |
| `make clean` | Removes build output. |

Release settings are fixed by the `Makefile`:

- **Rust**: `cargo build --release --locked`. Profile settings in `rust/Cargo.toml`, such as `lto`, `codegen-units` and `panic`, are allowed. Extra compiler flags are not: the `Makefile` clears `RUSTFLAGS`.
- **Go**: `go build -trimpath -pgo=off` with `CGO_ENABLED=0`. Profile-guided optimisation is not used.
- **C#**: `dotnet publish -c Release`, framework-dependent. Runtime settings in the project file, such as `TieredPGO`, `TieredCompilation`, `InvariantGlobalization` and `ServerGarbageCollector`, are allowed. Native ahead-of-time compilation, ReadyToRun and trimming are not: the `Makefile` turns them off.

Tests may use any test-only dependency. Choose the kinds of tests and how many: tests that would catch a real bug are worth more than many tests that would not.

## Toolchains

Use these versions. The scorer uses the same ones.

| Tool | Version | Container image with it |
|---|---|---|
| Rust, with `cargo fmt` and `cargo clippy` | 1.97.1 | `rust:1.97.1` |
| Go, with `gofmt` and `go vet` | 1.27.4 | `golang:1.27.4` |
| staticcheck | 2026.2.2 (module version `v0.8.2`) | install with `go install honnef.co/go/tools/cmd/staticcheck@v0.8.2` |
| .NET SDK, with `dotnet format` | 10.0.404 | `mcr.microsoft.com/dotnet/sdk:10.0.404` |

If a toolchain is missing on this machine, the container image in the table has it. The `Makefile` targets run inside those images; the .NET image needs `make` installed first.

## What good looks like

- **Correct.** Every rule of the spec, every edge case, every byte of output. Correctness comes first: nothing else makes up for wrong output.
- **Robust.** Bad input never crashes the program. It gets the rejection the protocol states.
- **Fast.** Sensible data structures, so that each operation stays cheap as the book grows. Output may be buffered.
- **Idiomatic.** Each implementation reads as native to its language, not as one language translated into another.
- **Tested.** Tests that would catch a bug, at the levels where bugs hide.
- **Lean.** No more code, dependencies or abstraction than the job needs. No features outside the spec.
- **Clear.** Exact names, types that carry meaning, and comments only where the code cannot speak for itself.
- **Clean to lint.** `make lint-rust`, `make lint-go` and `make lint-csharp` pass with no findings and no suppressions.

## What to deliver

- The deliverable is the committed head of the default branch of this directory. Uncommitted or unmerged work is not scored, and it is reported.
- Commit no build output.
- Add a short `DELIVERY.md` at the top of this directory. It says what was built, what is incomplete or known to be wrong, and how to build and test each language. Write it plainly: it is checked against what the code does.
