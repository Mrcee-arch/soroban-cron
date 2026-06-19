# ─────────────────────────────────────────────────────────────────────────────
# Stage 1 — builder
# Compiles the Engine binary for x86_64-unknown-linux-gnu.
# The Rust toolchain is NOT present in the final image.
# ─────────────────────────────────────────────────────────────────────────────
FROM rust:1.78-slim AS builder

# Install C linker and OpenSSL headers (required by reqwest/native-tls)
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy manifests first for layer-cache efficiency
COPY Cargo.toml Cargo.lock* ./
COPY chronos-types/Cargo.toml    chronos-types/Cargo.toml
COPY the-anchor/Cargo.toml       the-anchor/Cargo.toml
COPY the-engine/Cargo.toml       the-engine/Cargo.toml
COPY tests/integration/Cargo.toml tests/integration/Cargo.toml

# Create stub lib/main files so `cargo fetch` doesn't fail on missing sources
RUN mkdir -p chronos-types/src the-anchor/src the-engine/src tests/integration/src \
    && echo "" > chronos-types/src/lib.rs \
    && echo "#![no_std]" > the-anchor/src/lib.rs \
    && echo "fn main(){}" > the-engine/src/main.rs \
    && echo "" > tests/integration/src/lib.rs

# Pre-fetch dependencies
RUN cargo fetch

# Copy actual source
COPY chronos-types/ chronos-types/
COPY the-anchor/    the-anchor/
COPY the-engine/    the-engine/
COPY tests/         tests/

# Build release binary
RUN cargo build --release --package the-engine

# ─────────────────────────────────────────────────────────────────────────────
# Stage 2 — runtime
# Minimal Debian image containing only the binary and its runtime deps.
# ─────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Non-root user for security
RUN useradd -m -u 1001 -s /bin/sh engine
USER engine

COPY --from=builder /build/target/release/the-engine /usr/local/bin/the-engine

# Health-check endpoint
EXPOSE 8080

# Required environment variables — must be supplied at runtime
# ENV KEEPER_KEYPAIR=
# ENV RPC_ENDPOINT_URL=
# ENV ANCHOR_CONTRACT_ADDRESS=

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:8080/health || exit 1

ENTRYPOINT ["/usr/local/bin/the-engine"]
