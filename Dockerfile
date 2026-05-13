FROM rust:bookworm AS builder
WORKDIR /app
ENV RUSTFLAGS="-C target-cpu=haswell"

COPY Cargo.toml ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release && rm -rf src

COPY src ./src
RUN touch src/main.rs && cargo build --release

FROM debian:bookworm-slim AS preprocessor
RUN apt-get update \
    && apt-get install -y --no-install-recommends curl ca-certificates \
    && rm -rf /var/lib/apt/lists/*
RUN mkdir -p /data \
    && curl -L -o /data/references.json.gz \
       https://raw.githubusercontent.com/zanfranceschi/rinha-de-backend-2026/main/resources/references.json.gz
COPY --from=builder /app/target/release/rinha /usr/local/bin/rinha
RUN rinha preprocess /data/references.json.gz /data/index.bin \
    && rm /data/references.json.gz

FROM debian:bookworm-slim
COPY --from=builder /app/target/release/rinha /usr/local/bin/rinha
COPY --from=preprocessor /data/index.bin /data/index.bin
CMD ["rinha"]
