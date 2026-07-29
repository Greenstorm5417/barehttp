# Pinned builder; interop server only (not the barehttp MSRV).
FROM rust:1.85-bookworm AS builder
WORKDIR /app
COPY Cargo.toml ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update \
  && apt-get install -y --no-install-recommends wget ca-certificates \
  && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/barehttp-interop-axum /usr/local/bin/server
EXPOSE 8080
USER nobody
CMD ["/usr/local/bin/server"]
