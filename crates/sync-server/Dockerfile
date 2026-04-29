FROM rust:1-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release -p snapline-sync-server

FROM debian:bookworm-slim
WORKDIR /app
COPY --from=builder /app/target/release/snapline-sync-server /usr/local/bin/snapline-sync-server
EXPOSE 8080
CMD ["snapline-sync-server"]
