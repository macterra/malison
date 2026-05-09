# Malison Roadmap

This roadmap turns `SPEC.md` into an implementation sequence. It assumes the current baseline:

* `.rite` source files parse and validate for language `0.1`.
* `check`, `events`, and `render` exist.
* The MVP working in `examples/first-working/main.rite` renders to WAV.
* The built-in Rust renderer is the default backend.
* The SuperCollider non-realtime backend can render the same MVP target.

The goal is to keep Malison moving as a compiler and a musical instrument at the same time: every phase should preserve deterministic source-to-audio behavior and leave behind something audible.

## Phase 0: Stabilize The MVP

Status: complete.

Purpose: make the current `.rite -> IR -> WAV` path boringly reliable before adding more language.

### Completed

* Parse one `language 0.1` file.
* Validate one `working` with `tempo`, `meter`, `seed`, daemons, spells, rites, and `evoke wav`.
* Expand rhythm and note patterns to deterministic events.
* Render the MVP example through the Rust backend.
* Render the MVP example through the SuperCollider backend.
* Keep generated render artifacts out of git.
* Add CLI integration tests for `check`, `events`, `render --backend rust`, and `render --backend supercollider --dry-run`.
* Add JSON snapshot tests for `events`.
* Add SuperCollider script snapshot tests.
* Add `cargo run -- scry`.
* Make Phase 0 CLI path validation errors include file paths.
* Add a `--version` smoke test and basic release metadata.

### Remaining Work

None. Richer diagnostics, broader parser coverage, and IR schema refinement continue in Phase 1.

### Exit Criteria

* `cargo test` covers parser, compiler, Rust render, and SC script generation.
* The example renders through both backends from a clean checkout.
* README commands all work exactly as written.

## Phase 1: Real Frontend

Purpose: turn the current hand-rolled parser into a reliable compiler frontend with better diagnostics and clearer grammar boundaries.

### Lexer And Parser

* Decide whether to keep the hand-rolled parser or move to `logos` plus `chumsky`/`lalrpop`.
* Formalize token tests for comments, strings, escapes, fractions, pitches, identifiers, and reserved words. In progress; comments, strings, numbers, pitches, and block-comment errors are covered.
* Reject all reserved future syntax with clear diagnostics. In progress; reserved rite-body syntax is rejected.
* Improve recovery so one bad declaration does not hide every later error.
* Parse quoted rite names in all places the spec allows. Done for rite declarations.
* Add source spans to declarations, not only invocations.

### Diagnostics

* Introduce diagnostic codes such as `E021` unresolved daemon and `E044` routing cycle.
* Print source snippets with caret labels.
* Distinguish parse errors, semantic errors, backend errors, and filesystem errors.
* Add suggestions for nearby names: `basss` -> `bass`. Done for unresolved daemons and spells.
* Ensure `events` writes no non-JSON text to stdout on success. Done.

### Validation

* Validate parameter types and numeric ranges:
  * `pan` in `[-1, 1]`
  * `drive` in `[0, 1]`
  * positive `tempo`
  * supported `meter`
  * positive `every`
  Done.
* Reject invoking sample daemons with note patterns and synth daemons with rhythm patterns. Done.
* Reject empty rites unless explicitly allowed. Done.
* Validate that output paths are writable before rendering. Done for invalid parent paths.

### Exit Criteria

* Invalid source produces precise diagnostics with source locations.
* Parser tests cover valid and invalid forms from the spec.
* No parser behavior depends on the example program alone.

## Phase 2: Stable IR

Purpose: make IR a durable contract between the frontend, renderers, tests, and future tooling.

### IR Schema

* Move IR structs into a dedicated module. Done.
* Add explicit schema version, probably `"ir_version": "0.1"`. Done.
* Include canonical units in field names:
  * `tempo_bpm`
  * `time_beats`
  * `duration_beats`
  * `gain_db`
  * `cutoff_hz`
* Add declaration source spans for daemons, spells, rites, and render targets.
* Add semantic paths for events. Done:
  * `working:First Working/rite:main/invoke:0/step:12`
* Derive event IDs from semantic paths rather than current formatting alone. Done.

### Determinism

* Define stable sort order for all emitted arrays.
* Add snapshot tests proving unrelated whitespace and comments do not change events.
* Add tests proving unrelated declarations do not perturb event IDs.
* Prepare seeded random streams even before probability is implemented.

### Inspectability

* Implement `malison ir <file>`.
* Implement `malison graph <file>` as JSON first, visual output later.
* Implement `malison scry <file>` as human-readable event/routing/automation inspection.

### Exit Criteria

* The IR JSON shape is documented and tested.
* `events` and `ir` have stable snapshot coverage.
* Event source mapping explains why each sound exists.

## Phase 3: Pattern Expressiveness

Purpose: make tracks more musical without adding routing or synthesis complexity too early.

### Rhythm Patterns

* Add accented events: `X`.
* Add ghost notes: `g`.
* Add per-step velocity lowering/raising in IR.
* Add ties or continuations only after note duration semantics are clear.

### Pattern Transforms

Implement deterministic method-chain transforms:

* `.rotate(steps)`
* `.reverse()`
* `.repeat(count)`
* `.every(interval)` if it remains useful after invocation-level `every`

Defer stochastic transforms until random stream identity is settled:

* `.degrade(amount)`
* `.humanize(amount)`
* `.mutate(probability)`
* `.velocity(range)`

### Euclidean Rhythms

* Implement `euclid(pulses, steps)`.
* Implement `.rotate(steps)` for Euclidean patterns.
* Add tests for edge cases: zero pulses, full pulses, invalid step counts.

### Probability And Humanization

* Implement deterministic probability expansion.
* Implement timing humanization with bounded offsets that never cross rite boundaries unless explicitly allowed.
* Implement velocity randomization.
* Add snapshot tests tied to stable seeds.

### Exit Criteria

* The example track uses at least accents or Euclidean rhythm.
* Pattern transforms are deterministic and source-mapped.
* Random edits outside a transformed pattern do not change its random choices.

## Phase 4: Musical Arrangement

Purpose: move beyond one repeating section while keeping the model inspectable.

### Rites

* Support multiple ordered rites in examples and tests.
* Add explicit placement:
  * `rite drop at bar 17 bars 32`
  * `rite reprise at 2:15 bars 16`
* Reject overlaps by default.
* Add explicit layered rites:
  * `rite drone_layer at bar 1 bars 64 layer`

### Invocation Lifecycle

* Define whether invocations are event streams, one-shot events, or persistent processes.
* Implement `banish` only after lifecycle semantics are explicit.
* Add continuous drone behavior through a clear event kind.

### Automation

* Add basic automation curves independent of aesthetic controls:
  * linear
  * exponential
  * stepped
* Lower automation into IR control events.
* Implement automation in Rust and SC backends.

### Exit Criteria

* A second example has intro, drop, collapse, and outro rites.
* Overlap errors are precise.
* Arrangement renders identically in Rust and SC for supported features.

## Phase 5: Sound Sources And Rendering

Purpose: make the sonic palette worth composing with.

### Samples

* Support stereo sample playback.
* Decide on sample-rate conversion strategy.
* Add better errors for unsupported sample formats.
* Add sample start/end offsets.
* Add sample amplitude normalization only if explicitly requested.
* Add sample kits after individual sample playback is solid.

### Built-In Synths

Implement the next smallest useful archetypes:

* `noise_burst`
* `drone`
* `swarm`
* `metal_hit`

Improve `saw_sub`:

* configurable ADSR
* detune
* sub level
* filter resonance
* safer anti-aliasing or band-limited oscillator where practical

### Backend Parity

* Maintain a backend capability table for Rust and SuperCollider.
* Add backend parity tests where exact waveform equality is not required but event support is.
* Make unsupported backend features fail before render starts.
* Add optional retention of generated SC scripts for debugging.

### Exit Criteria

* At least three daemon kinds render in both backends.
* Backend unsupported-feature diagnostics are clear.
* The example track sounds intentionally designed, not only technically correct.

## Phase 6: Routing, Mixing, And Effects

Purpose: introduce the `circle`, `effect`, and `ward` concepts after core event rendering is stable.

### Circles

* Parse circle declarations.
* Implement implicit `master` as an explicit IR node.
* Validate unresolved circles.
* Validate routing cycles.
* Support daemon `out` parameters.

### Effects

Start with low-risk effects:

* `gain`
* `pan`
* `highpass`
* `lowpass`
* `saturator`
* `delay`
* `reverb`
* `limiter`

Each effect needs:

* parameter schema
* default values
* backend capability flag
* Rust implementation or explicit unsupported diagnostic
* SC implementation or explicit unsupported diagnostic

### Wards

* Implement hard validation wards first:
  * limiter ceiling declaration validation
  * loudness target declaration validation
  * unsafe gain rejection
* Add soft wards as processors only after routing and effects are working.

### Exit Criteria

* Drums and bass can route through separate circles.
* A master limiter ward prevents unsafe output.
* Routing graph appears in `ir` and `graph`.

## Phase 7: Aesthetic Control

Purpose: add the distinctive Malison layer: structural dark-electronic controls that lower to concrete parameters.

### Control Streams

* Implement normalized control streams in `[0, 1]`.
* Add `raise` and `lower` for:
  * `tension`
  * `density`
  * `instability`
  * `harshness`
  * `spaciousness`
  * `degradation`
* Decide local versus global scope inside rites.

### Bindings

* Parse `bind target to source from -> to`.
* Represent bindings as IR control edges.
* Compile bindings to automation curves.
* Validate target parameter type and unit compatibility.

### Musical Lowering

* Define default mappings only where they are musically defensible.
* Prefer explicit bindings over hidden magic.
* Add examples where tension opens filters, raises density, increases drive, and widens reverb.

### Exit Criteria

* Aesthetic controls produce inspectable automation, not opaque prompt-like behavior.
* `scry` can explain which control changed which sound.
* A demo track uses tension/degradation in a way audible from source.

## Phase 8: Project System And Libraries

Purpose: make Malison usable across more than one file and more than one sample folder.

### Manifest

* Parse `malison.toml`.
* Respect render defaults:
  * sample rate
  * bit depth
  * backend
* Respect path defaults:
  * samples
  * renders
  * build
* Add manifest validation diagnostics.

### Multi-File Source

* Add imports or includes.
* Decide whether each file may define declarations or only libraries.
* Preserve source maps across files.
* Add semantic paths that include module/import context.

### Libraries

* Add local reusable spells and daemon presets.
* Defer package management until local libraries are stable.
* Keep dependency versions in render metadata for reproducibility.

### Exit Criteria

* A project can split drums, bass, and arrangement into separate files.
* Sample paths resolve through manifest paths.
* Render output is reproducible from source plus manifest.

## Phase 9: Tooling

Purpose: make Malison pleasant to write, inspect, and refactor.

### Formatter

* Implement `malison fmt <file>`.
* Preserve comments.
* Normalize spacing around blocks, declarations, and parameters.
* Add formatting snapshots.

### Language Server

* Diagnostics while editing.
* Symbol lookup for daemons, spells, rites, and circles.
* Go-to definition.
* Hover docs for built-in params and units.
* Completion for declarations and supported backend features.

### Developer Tools

* `malison graph` visual output.
* `malison scry` richer causality reports.
* Audio preview cache.
* Semantic diff for IR/event changes.

### Exit Criteria

* Editing a `.rite` file in VS Code gives useful diagnostics.
* Formatting is stable.
* Users can inspect event and routing causality without reading raw JSON.

## Phase 10: Release Readiness

Purpose: package Malison as a real tool.

### Quality

* Add CI for formatting, clippy, tests, and example renders.
* Add fixture render smoke tests with generated artifacts excluded from git.
* Add platform notes for Linux/macOS/Windows.
* Add deterministic build metadata.

### Distribution

* Publish binary artifacts for releases.
* Document SuperCollider installation as optional.
* Include example projects.
* Add changelog.

### Documentation

* Tutorial: first `.rite`.
* Reference: language `0.1`.
* Reference: IR schema.
* Reference: backend capabilities.
* Guide: debugging with `events`, `ir`, `graph`, and `scry`.

### Exit Criteria

* A new user can install Malison, render the example, and understand the source in under ten minutes.
* The project has a reproducible release process.

## Near-Term Backlog

These are the most useful next tasks, in suggested order:

1. Add CLI integration tests with `assert_cmd`. Done.
2. Add event JSON snapshot tests with `insta`. Done.
3. Implement `malison ir`. Done.
4. Implement `malison scry` as a text event summary.
5. Add rhythm accents with `X` and ghost notes with `g`.
6. Add velocity to event IR and both renderers.
7. Improve diagnostics with source snippets.
8. Add stereo sample support.
9. Add explicit manifest parsing.
10. Build a second example with multiple rites.

## Guiding Rules

* Keep source text as the canonical track.
* Prefer deterministic compilation over clever runtime behavior.
* Every new language feature needs an IR representation.
* Every new backend feature needs a capability check.
* Every deferred construct should fail clearly if used too early.
* Every phase should leave behind something audible or inspectable.
