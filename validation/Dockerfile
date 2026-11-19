# The scorer image: the pinned toolchains and analysers of toolchains.toml, with benchy and its scoring data.
# Build it from the root of the benchy checkout: docker build -f validation/Dockerfile -t benchy-scorer .

FROM rust:1.97.1@sha256:b1b3c9c0d921d7fa0a6d1f9ec7e4eab87f8c8ec97644c3d791450f131dec813f AS rust
RUN rustup component add clippy rustfmt

FROM golang:1.27.4@sha256:205c7636d74e0ffbe3d007162df7d74f859f5a8c8322dfb9f907977686c126cf AS go
RUN CGO_ENABLED=0 GOBIN=/opt/go-tools go install honnef.co/go/tools/cmd/staticcheck@v0.8.2

# The .NET SDK image is the base because the SDK needs the native libraries its own image carries.
FROM mcr.microsoft.com/dotnet/sdk:10.0.404@sha256:fe0fd4e93bd2fbb47c883b7a6369e5c8d3db8bf01cdc5981f6ee35fbdf3ff4fb AS toolchains
RUN apt-get update \
    && apt-get install -y --no-install-recommends gcc libc6-dev make git python3 python3-venv \
    && rm -rf /var/lib/apt/lists/*
RUN python3 -m venv /opt/lizard \
    && /opt/lizard/bin/pip install --no-cache-dir lizard==1.24.3
COPY --from=rust /usr/local/rustup /usr/local/rustup
COPY --from=rust /usr/local/cargo /usr/local/cargo
COPY --from=go /usr/local/go /usr/local/go
COPY --from=go /opt/go-tools/staticcheck /usr/local/bin/staticcheck
RUN useradd --create-home --uid 10001 scorer
ENV RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/home/scorer/.cargo \
    GOTOOLCHAIN=local \
    GOPATH=/home/scorer/go \
    DOTNET_CLI_TELEMETRY_OPTOUT=1 \
    DOTNET_NOLOGO=1 \
    DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1 \
    PATH=/usr/local/cargo/bin:/usr/local/go/bin:/opt/lizard/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

FROM toolchains AS build
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src src
RUN cargo build --release --locked --bin benchy

FROM toolchains
COPY --from=build /build/target/release/benchy /usr/local/bin/benchy
COPY task /opt/benchy/task
COPY validation /opt/benchy/validation
RUN mkdir -p /work/in /work/out && chown -R scorer:scorer /work
USER scorer
WORKDIR /opt/benchy
HEALTHCHECK NONE
ENTRYPOINT ["benchy"]
