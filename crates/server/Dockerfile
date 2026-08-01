FROM rust:1.95-slim-bookworm AS builder
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY .cargo .cargo
COPY vendor vendor
COPY crates/domain crates/domain
COPY crates/server crates/server
RUN cargo build --offline --locked --release -p snapline-server

FROM debian:bookworm-slim
COPY --from=builder /build/target/release/snapline-server /usr/local/bin/snapline-server
USER 10001:10001
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/snapline-server"]
