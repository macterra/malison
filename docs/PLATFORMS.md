# Platform Notes

Malison is a Rust CLI and should build anywhere Rust supports `hound` WAV IO.

## Linux

The Rust backend has no external audio-engine dependency.

For the SuperCollider backend, install `sclang` and `scsynth` and make sure both are on `PATH`.

## macOS

The Rust backend should work with the standard Rust toolchain.

For SuperCollider rendering, install SuperCollider and ensure `sclang` is available on `PATH`.

## Windows

The Rust backend should work with the standard Rust toolchain.

SuperCollider support depends on `sclang` being available on `PATH`; this has less project test coverage than Linux.

## Shared Samples

Shared sample libraries can live outside an individual track folder. Add them to `malison.toml`:

```toml
[paths]
samples = "samples"
sample_libraries = ["/Volumes/audio/samples", "../../shared-samples"]
```
