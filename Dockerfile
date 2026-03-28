# Stage 1: builder
FROM rust:1.82-bookworm AS builder

WORKDIR /build

# Copy manifests first for layer caching — source changes won't bust this layer
COPY Cargo.toml Cargo.lock ./

# Copy workspace members so Cargo can resolve the full workspace graph
COPY apps/ apps/
COPY crates/ crates/

# Copy all remaining source
COPY . .

# Build only the API binary in release mode
RUN cargo build --release --bin noise-api

# Stage 2: runtime
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user (uid 1000)
RUN useradd --uid 1000 --no-create-home --shell /usr/sbin/nologin noise

# Copy the compiled binary from the builder stage
COPY --from=builder /build/target/release/noise-api /usr/local/bin/noise-api

# Create the data directory and hand ownership to the noise user
RUN mkdir -p /data && chown noise:noise /data

EXPOSE 8080

ENV NOISE_DB=/data/noise.db
ENV RUST_LOG=info

USER noise

ENTRYPOINT ["/usr/local/bin/noise-api"]
