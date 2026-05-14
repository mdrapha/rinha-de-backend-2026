FROM --platform=linux/amd64 rust:1.83-slim AS build
RUN apt-get update && apt-get install -y --no-install-recommends \
    gcc libc6-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /src

COPY Cargo.toml ./
RUN mkdir -p src/bin && \
    echo 'fn main() {}' > src/main.rs && \
    echo 'fn main() {}' > src/proxy.rs && \
    echo 'fn main() {}' > src/build_index.rs && \
    cargo fetch && rm -rf src

COPY src ./src
COPY resources ./resources

ENV RUSTFLAGS="-C target-cpu=haswell -C target-feature=+avx2,+fma,+f16c,+bmi2,+popcnt -C link-arg=-s"

COPY data/ ./data/
RUN if [ ! -f data/index.bin.gz ]; then \
        cargo build --release --bin build_index && \
        ./target/release/build_index; \
    fi

RUN cargo build --release --bin rinha && \
    cargo build --release --bin proxy && \
    strip target/release/rinha target/release/proxy

FROM debian:12-slim AS api
COPY --from=build /src/target/release/rinha /rinha
ENTRYPOINT ["/rinha"]

FROM debian:12-slim AS proxy
COPY --from=build /src/target/release/proxy /proxy
ENTRYPOINT ["/proxy"]
