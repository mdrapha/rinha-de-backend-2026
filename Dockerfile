FROM rust:bookworm AS builder
WORKDIR /app
RUN apt-get update && apt-get install -y --no-install-recommends git && rm -rf /var/lib/apt/lists/*
ARG REPO_URL=https://github.com/mdrapha/rinha-de-backend-2026.git
RUN git clone --branch main --depth 1 ${REPO_URL} .
ENV RUSTFLAGS="-C target-cpu=haswell"
RUN cargo build --release

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
