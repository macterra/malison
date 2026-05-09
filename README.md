# Malison

Executable scores for dark electronic music.

## Status

This repository currently contains the Malison language specification and an early Rust implementation of the version `0.1` compiler surface.

Implemented commands:

```bash
cargo run -- check path/to/main.rite
cargo run -- events path/to/main.rite
cargo run -- render path/to/main.rite
```

The version `0.1` renderer currently uses the built-in Rust backend. It supports WAV sample triggers and the `saw_sub` synth needed by the MVP target.

Try the included MVP working:

```bash
cargo run -- render examples/first-working/main.rite --force
```

This writes:

```text
examples/first-working/renders/first-working.wav
```

`render --backend supercollider --dry-run` emits the generated SuperCollider source stub. Full SuperCollider execution is not implemented yet.

## Development

```bash
cargo test
```
