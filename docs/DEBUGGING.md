# Debugging Guide

Useful inspection commands:

```bash
malison check main.rite
malison ir main.rite
malison events main.rite
malison graph main.rite
malison graph main.rite --format dot
malison scry main.rite
malison diff before.rite after.rite
malison capabilities
```

Use `check` first when a file does not compile. Diagnostics include category codes and source snippets when the error maps to source.

Use `events` when a sound happens at the wrong time. Events include:

* `semantic_path`
* `time_beats`
* `duration_beats`
* `daemon`
* `velocity`
* source location

Use `graph --format dot` to visualize declarations, circles, render targets, events, and controls with Graphviz-compatible tooling.

Successful renders write a deterministic `.malison.json` sidecar next to the WAV with the compiler version, backend, render settings, seed, and event/control counts.

Use `diff` before and after edits to see how event counts and event identities changed.
