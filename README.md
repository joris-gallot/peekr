# peekr

Self-hosted, real-time Docker log viewer.

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
