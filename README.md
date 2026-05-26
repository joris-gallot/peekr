# peekr

Self-hosted, real-time Docker log viewer.

## Run with Docker

The image bundles the frontend into the binary (rust-embed), so one container
serves everything. Mount the Docker socket so peekr can read containers.

```sh
docker build -t peekr .
docker run -p 8080:8080 -v /var/run/docker.sock:/var/run/docker.sock peekr
```

Open http://localhost:8080. Bind address is configurable via `PEEKR_ADDR`
(default `0.0.0.0:8080`).

## Development

Two processes; Vite proxies `/api` to the backend.

```sh
cargo run -p peekr-server   # backend on :8080
cd web && pnpm dev          # frontend on :5174
```

To run the single-binary build locally, build the frontend then enable the
embed feature:

```sh
cd web && pnpm build
cargo run -p peekr-server --features embed-ui   # serves UI + API on :8080
```

## Tests

### Backend integration tests

The integration tests drive the real Docker daemon, so they need log-emitter fixtures running. They are `#[ignore]`d by default (a plain `cargo test` skips them and stays green).

```sh
# 1. start the fixtures (json / plain / stderr / multiline / burst emitters)
docker compose -f fixtures/compose.yaml up -d

# 2. run the ignored tests
cargo test -p peekr-server -- --ignored

# 3. stop the fixtures
docker compose -f fixtures/compose.yaml down
```

The same fixtures are handy in dev: bring them up and point peekr at the
`peekr-fx-*` containers to exercise JSON, plain, stderr, multi-line and
high-volume log rendering.
