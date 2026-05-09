# Malison IR Schema

This document describes the JSON emitted by:

```bash
malison ir path/to/main.rite
malison events path/to/main.rite
```

The current schema version is `0.1`. Object keys are emitted in stable order, and arrays are deterministic for the same source and seed.

## Top-Level Object

```json
{
  "ir_version": "0.1",
  "language": "0.1",
  "working": "First Working",
  "tempo_bpm": 128.0,
  "meter": [4, 4],
  "seed": "first",
  "random_streams": [],
  "duration_beats": 4.0,
  "daemons": [],
  "spells": [],
  "rites": [],
  "render_targets": [],
  "events": []
}
```

## Random Streams

Random streams reserve stable identities for deterministic stochastic features.

```json
{
  "id": "spell:kicks",
  "semantic_path": "working:First Working/spell:kicks",
  "seed_hash": "7db32f1e8aef4552"
}
```

The stream hash is derived from the working seed and semantic path. Future probability and humanization features should draw from these streams instead of source locations.

## Daemons

Daemons describe sound-producing units.

```json
{
  "id": "kick",
  "kind": "sample",
  "sample": "samples/kick.wav",
  "params": {
    "gain_db": -3.0
  },
  "source": {
    "file": "examples/first-working/main.rite",
    "line": 9,
    "column": 3
  }
}
```

Parameter names use canonical units where applicable, such as `gain_db`, `cutoff_hz`, `highpass_hz`, `lowpass_hz`, and `tune_semitones`.

## Spells

Spells preserve their declared pattern body and kind.

```json
{
  "id": "kicks",
  "kind": "pattern",
  "body": "X--- x--- x-g- x---",
  "source": {
    "file": "examples/first-working/main.rite",
    "line": 16,
    "column": 3
  }
}
```

## Rites

Rites are ordered arrangement sections measured in beats.

```json
{
  "id": "main",
  "start_beats": 0.0,
  "duration_beats": 4.0,
  "source": {
    "file": "examples/first-working/main.rite",
    "line": 19,
    "column": 3
  }
}
```

## Render Targets

Render targets represent `evoke` declarations.

```json
{
  "id": "wav",
  "kind": "wav",
  "path": "renders/first-working.wav",
  "source": {
    "file": "examples/first-working/main.rite",
    "line": 24,
    "column": 3
  }
}
```

## Events

Events are sorted by `time_beats`, then source order, then `kind`, then `id`.

```json
{
  "id": "evt_bb5d32adc4429162",
  "semantic_path": "rite:main/invoke:0/step:0",
  "kind": "trigger",
  "time_beats": 0.0,
  "duration_beats": 0.25,
  "daemon": "kick",
  "velocity": 1.25,
  "params": {
    "gain_db": -3.0
  },
  "source": {
    "file": "examples/first-working/main.rite",
    "line": 20,
    "column": 5
  }
}
```

Note events additionally include `pitch`:

```json
{
  "pitch": {
    "name": "F1",
    "midi": 29
  }
}
```

Event IDs are derived from `semantic_path`, not source line numbers. Formatting-only edits can change `source` locations while preserving event identity.
