# Malison

Executable scores for dark electronic music.

## Status

This repository currently contains the Malison language specification and an early Rust implementation of the version `0.1` compiler surface.

Implemented commands:

```bash
cargo run -- check path/to/main.rite
cargo run -- events path/to/main.rite
cargo run -- render path/to/main.rite --dry-run
```

`render --dry-run` emits the generated SuperCollider source stub. Full audio rendering is not implemented yet.

## Development

```bash
cargo test
```
