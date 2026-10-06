# Stage 1: Build binary using official Rust image
FROM rust:1.80-slim as builder

WORKDIR /usr/src/gdb
COPY . .

RUN apt-get update && apt-get install -y pkg-config libssl-dev protobuf-compiler cmake make clang && \
    cargo build --release --bin gdb-server --bin gdb-cli --bin gdb-studio

# Stage 2: Minimal runtime image
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates libssl3 curl && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/gdb/target/release/gdb-server /usr/local/bin/gdb-server
COPY --from=builder /usr/src/gdb/target/release/gdb-cli /usr/local/bin/gdb-cli
COPY --from=builder /usr/src/gdb/target/release/gdb-studio /usr/local/bin/gdb-studio

EXPOSE 8848 8847 3000

ENTRYPOINT ["gdb-server"]
CMD ["--node-id", "1", "--partitions", "8", "--port", "8848", "--http-port", "8847"]
