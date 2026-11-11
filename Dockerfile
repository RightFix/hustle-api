# syntax=docker/dockerfile:1

FROM rust:1.86-bookworm AS builder
WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && printf 'fn main(){}' > src/main.rs \
    && cargo build --release \
    && rm -rf src target/release/deps/hustle_backend* target/release/hustle-backend*

# Build application
COPY src ./src
COPY migrations ./migrations
RUN cargo build --release

FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app

COPY --from=builder /app/target/release/hustle-backend /usr/local/bin/hustle-backend
COPY --from=builder /app/migrations /app/migrations

ENV RUST_LOG=hustle_backend=info,tower_http=info
EXPOSE 8000
VOLUME ["/app/uploads"]

CMD ["hustle-backend"]
