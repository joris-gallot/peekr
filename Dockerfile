# syntax=docker/dockerfile:1

# 1. Build the frontend into web/dist.
FROM node:22-alpine AS web
RUN npm install -g pnpm@10
WORKDIR /app/web
COPY web/package.json web/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY web/ ./
RUN pnpm build

# 2. Build the Rust binary with the frontend embedded (rust-embed reads ../../web/dist).
FROM rust:1-bookworm AS build
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=web /app/web/dist ./web/dist
RUN cargo build --release -p peekr-server --features embed-ui

# 3. Minimal runtime image: just the static binary.
FROM debian:bookworm-slim
COPY --from=build /app/target/release/peekr /usr/local/bin/peekr
EXPOSE 8080
ENTRYPOINT ["peekr"]
