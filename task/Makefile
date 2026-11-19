# Fixed build, test and lint targets for the three engines.
# The scorer replaces this file with its own copy, so an edit here changes nothing that is scored.

BIN := $(CURDIR)/bin

RUST_LINTS := $(shell grep -v '^\#' lint/rust/clippy.txt)
GO_CHECKS := $(shell grep -v '^\#' lint/go/staticcheck.txt)

CARGO := CARGO_TARGET_DIR=target RUSTFLAGS= cargo
GO := CGO_ENABLED=0 GOFLAGS=-mod=readonly GOWORK=off go
DOTNET := DOTNET_CLI_TELEMETRY_OPTOUT=1 DOTNET_NOLOGO=1 dotnet
DOTNET_RELEASE := -c Release -p:PublishAot=false -p:PublishReadyToRun=false -p:PublishTrimmed=false -p:RestoreLockedMode=true

.PHONY: build-rust build-go build-csharp test-rust test-go test-csharp lint-rust lint-go lint-csharp clean

build-rust:
	cd rust && $(CARGO) build --release --locked --bin engine
	mkdir -p $(BIN)/rust
	cp rust/target/release/engine $(BIN)/rust/engine

build-go:
	mkdir -p $(BIN)/go
	cd go && $(GO) build -trimpath -pgo=off -o $(BIN)/go/engine ./cmd/engine

build-csharp:
	$(DOTNET) publish csharp/Engine/Engine.csproj $(DOTNET_RELEASE) -o $(BIN)/csharp

test-rust: build-rust
	cd rust && $(CARGO) test --release --locked

test-go: build-go
	cd go && $(GO) test -count=1 ./...

test-csharp: build-csharp
	cd csharp && $(DOTNET) test $(DOTNET_RELEASE)

lint-rust:
	cd rust && $(CARGO) fmt --all --check -- --config-path ../lint/rust/rustfmt.toml
	cd rust && $(CARGO) clippy --release --locked --all-targets -- $(RUST_LINTS)

lint-go:
	cd go && test -z "$$(gofmt -l .)" || { gofmt -l .; exit 1; }
	cd go && $(GO) vet ./...
	cd go && staticcheck -checks "$(GO_CHECKS)" ./...

lint-csharp:
	cd csharp && $(DOTNET) format whitespace --verify-no-changes
	cd csharp && $(DOTNET) build -c Release --no-incremental @../lint/csharp/analyzers.rsp

clean:
	rm -rf $(BIN) rust/target
	find csharp -type d \( -name bin -o -name obj \) -prune -exec rm -rf {} +
