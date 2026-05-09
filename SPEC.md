# Malison: Executable Scores for Dark Electronic Music

## Status

First draft. This document defines the initial design for **Malison**, a domain-specific language and compiler for composing dark electronic music as executable programs.

Malison is intended to produce rendered audio from source files. Its first implementation target is offline rendering through an existing synthesis backend, with SuperCollider as the likely initial backend. The long-term design preserves backend independence through a stable intermediate representation.

## 1. Purpose

Malison is a programmable composition system for dark electronic music. A Malison source file describes a track using explicit musical, structural, sonic, and procedural constructs. The compiler expands that source into deterministic musical events, synthesis instructions, automation curves, routing graphs, and render directives, then produces audio.

The central premise is simple:

> The source file is the canonical track. Audio is a render artifact.

This makes music composable with software-engineering affordances: version control, reproducible builds, semantic diffs, libraries, reusable abstractions, testable compilation, and deterministic seeded variation.

Malison is designed especially for industrial, dark ambient, EBM, noise, ritual electronics, drone, and maximalist distorted electronic music. It should also be general enough to compose other electronic forms without collapsing into a genre toy.

## 2. Design Goals

### 2.1 Track-as-program

A Malison file should be an executable score. It should contain enough information to regenerate the track deterministically, including tempo, meter, scale, instruments, sample references, patterns, automation, routing, arrangement, effects, randomness seeds, and output settings.

The user should be able to run:

```bash
malison render src/main.rite --out renders/wound-engine.wav
```

and receive the same render from the same source, seed, dependency versions, and backend configuration.

### 2.2 Deterministic generativity

Malison should support randomness, mutation, degradation, probability, humanization, variation, and stochastic arrangement. These features must be deterministic under an explicit seed.

The same source plus the same seed must generate the same expanded event graph.

### 2.3 Aesthetic-level composition

Malison should expose high-level musical/aesthetic concepts as first-class constructs when they are musically useful. Examples include tension, density, instability, decay, harshness, spaciousness, pressure, brittleness, and degradation.

These concepts should lower to concrete synthesis, pattern, effect, and mix parameters. The compiler should preserve the distinction between aesthetic intent and backend implementation.

### 2.4 Backend independence

Malison must avoid defining its semantics in terms of SuperCollider, MIDI, VSTs, Ableton Live, WebAudio, or any specific audio engine.

SuperCollider may be the first backend. It must not become the language.

The compiler pipeline should be:

```text
.rite source
  -> lexer/parser
  -> AST
  -> semantic validation
  -> Malison IR
  -> backend generator
  -> audio render
```

### 2.5 Inspectability

The compiler should allow the user to inspect each stage:

```bash
malison parse src/main.rite
malison ir src/main.rite
malison events src/main.rite
malison graph src/main.rite
malison render src/main.rite
```

The user should be able to understand why a sound occurs at a given time and where it came from in the source.

### 2.6 Dark without gimmickry

Malison uses dark ritual language as an aesthetic frame, but the system must remain technically serious. Names should support comprehension. They should not turn the language into costume.

The language may use terms such as `working`, `rite`, `spell`, `daemon`, `circle`, `ward`, `bind`, `invoke`, `scry`, and `evoke` where they map cleanly onto composition concepts.

## 3. Core Concepts

### 3.1 Working

A **working** is the top-level composition unit. It usually corresponds to a track.

```malison
working "Wound Engine" {
  tempo 132
  scale F phrygian
  seed "iron-altar"

  rite drop bars 32 {
    invoke kick
    invoke bass with bassline every 1/8
  }
}
```

A working may define global tempo, meter, scale, seed, libraries, instruments, patterns, routing, arrangement, render targets, and metadata.

### 3.2 Rite

A **rite** is an arrangement section: intro, drop, breakdown, return, outro, interlude, coda, or any user-defined structural region.

```malison
rite intro bars 16 {
  invoke kick gain -8 highpass 180
  invoke drone rise 16
}

rite drop bars 32 {
  invoke kick
  invoke hats
  invoke bass with bassline every 1/8
  raise tension 0.35 -> 0.9
}
```

Rites are ordered by source position unless explicitly scheduled.

### 3.3 Spell

A **spell** is a reusable musical generator. It may represent a rhythm, note pattern, modulation pattern, transformation, sample selection rule, or compound musical behavior.

```malison
spell kick = pattern "x--- x--- x-x- x---"
spell hats = euclid(11, 16).brittle(0.35).humanize(0.014)
spell bassline = notes "F1 - F1 Gb1 | Ab1 - F1 Eb1"
```

Spells are declarative. They do not produce sound until invoked.

### 3.4 Daemon

A **daemon** is a sound-producing or sound-processing unit. It may compile to a synth definition, sample player, effect chain, bus processor, or backend-specific audio graph.

```malison
daemon bass = saw_sub {
  detune 0.08
  drive 0.45
  cutoff bind tension(180, 1200)
  envelope attack 0.01 decay 0.18 sustain 0.7 release 0.08
}
```

Daemons are not autonomous agents. The term denotes a persistent audio process or reusable sonic entity.

### 3.5 Circle

A **circle** is a routing context: a bus, group, or submix.

```malison
circle drums {
  ward limiter ceiling -1.0
  effect room "concrete" decay 2.8 wet 0.12
}
```

Circles may contain routing rules, effects, safety constraints, sends, sidechains, and mix settings.

### 3.6 Ward

A **ward** is a protective or limiting constraint. Wards may prevent clipping, illegal routing, excessive gain, excessive brightness, backend-incompatible values, or unsafe output levels.

```malison
ward master limiter ceiling -1.0
ward loudness max -9 LUFS
ward frequency sub max 0.9
```

The compiler should treat hard wards as validation constraints. Soft wards may compile to audio processors.

### 3.7 Binding

A **binding** connects one parameter to another source: tension, envelope, LFO, pattern, automation curve, sidechain, random stream, or user-defined control signal.

```malison
bind bass.cutoff to tension 180 -> 1200
bind reverb.wet to breakdown 0.1 -> 0.45
bind hats.density to tension curve exponential
```

Bindings should be visible in the IR as control edges.

### 3.8 Evocation

An **evocation** is a render target.

```malison
evoke wav "renders/wound-engine.wav"
evoke stems "renders/stems/"
```

The primary CLI may use ordinary `render` terminology, but the language may use `evoke` as a render directive.

## 4. Example Program

```malison
working "Wound Engine" {
  tempo 132
  meter 4/4
  scale F phrygian
  seed "black-furnace"

  metadata {
    artist "Malison Example"
    tags ["industrial", "dark-electronic", "ritual"]
  }

  circle master {
    ward limiter ceiling -1.0
    effect tape_drive amount 0.18
  }

  circle drums -> master {
    effect room "concrete" decay 2.4 wet 0.12
  }

  circle bass -> master {
    effect saturator amount 0.35
    bind gain to sidechain(kick) amount 0.55
  }

  daemon kick = sample "kick_909_dark" {
    tune -2
    drive 0.22
    out drums
  }

  daemon hats = samplekit "corroded_hats" {
    out drums
    brittle 0.35
  }

  daemon bass = saw_sub {
    out bass
    detune 0.08
    drive 0.45
    cutoff 240
    envelope attack 0.01 decay 0.18 sustain 0.7 release 0.08
  }

  daemon drone = swarm {
    out master
    root F2
    voices 9
    spread 31 cents
    instability 0.28
  }

  spell kickline = pattern "x--- x--- x-x- x---"
  spell hatline = euclid(11, 16).humanize(0.014).velocity(rand(0.35, 0.8))
  spell bassline = notes "F1 - F1 Gb1 | Ab1 - F1 Eb1"

  rite invocation bars 16 {
    invoke kick with kickline gain -8 highpass 180
    invoke drone rise 16 gain -10
    raise tension 0.1 -> 0.35
  }

  rite possession bars 32 {
    invoke kick with kickline
    invoke hats with hatline
    invoke bass with bassline every 1/8
    bind bass.cutoff to tension 180 -> 1200 curve exponential
    raise tension 0.35 -> 0.9
  }

  rite collapse bars 16 {
    banish kick
    invoke drone gain -4 smear 0.45
    invoke bass with bassline.degrade(0.22) every 1/8
    lower tension 0.9 -> 0.25
  }

  evoke wav "renders/wound-engine.wav"
}
```

## 5. Language Surface

### 5.1 File extension

The recommended public file extension is:

```text
.rite
```

Rationale: it reinforces the central composition unit, is distinctive in repository listings, and avoids collision with other `.mal`-using tools or file types.

Alternative extensions:

```text
.malison
.working
.mal
```

For a public developer tool, `.rite` is the default source extension.

### 5.2 Version profile

The first compiler should implement an explicit language profile:

```malison
language 0.1
```

If omitted, the compiler should assume the latest stable profile supported by the tool. Examples in this document that use deferred features should be treated as design sketches unless they appear in the MVP section.

Version `0.1` includes:

* one `working` per file
* global `tempo`, `meter`, and `seed`
* `daemon` declarations for samples and one built-in synth
* `spell` declarations for rhythm and note patterns
* ordered `rite` blocks with fixed bar durations
* `invoke` statements
* event parameters for `gain`, `pan`, `highpass`, `lowpass`, and `cutoff`
* `evoke wav`

Version `0.1` does not include aesthetic controls, circles, wards, explicit layering, sidechains, method-chain pattern transforms, package dependencies, or low-level synthesis graphs. Those constructs remain reserved syntax.

### 5.3 Comments

```malison
// single-line comment

/*
  block comment
*/
```

### 5.4 Identifiers

Identifiers are lowercase or snake_case by convention:

```malison
kick
bassline
black_furnace
main_drop
```

Quoted names are allowed for user-facing labels:

```malison
working "Black Circuit"
rite "machine prayer" bars 24 { ... }
```

### 5.5 Numeric units

Malison should support explicit musical and audio units:

```malison
132 bpm
16 bars
1/8
31 cents
180 hz
-9 dB
-14 LUFS
0.014 sec
```

Bare numbers are allowed in context-sensitive positions where the unit is unambiguous.

The compiler should canonicalize units during semantic validation:

```text
bpm      -> beats per minute
bars     -> beats, after meter resolution
beat     -> beats
sec      -> seconds
hz       -> hertz
dB       -> decibels
LUFS     -> loudness units relative to full scale
cents    -> cents
```

Parameter schemas decide which units are valid. For version `0.1`, bare `gain` values are decibels, `cutoff` accepts hertz, and `pan` accepts a unitless value in `[-1, 1]`. Later profiles may add explicit linear-gain syntax.

### 5.6 Time model

The primary musical time unit is the beat. Bars are resolved from meter. Audio time in seconds is derived after tempo resolution.

The compiler should preserve both musical and absolute time in IR when possible:

```json
{
  "beat": 64.0,
  "seconds": 29.0909
}
```

For version `0.1`, tempo and meter are constant for the whole working. Later versions may add tempo and meter automation.

All scheduled musical events use half-open time intervals:

```text
[start_beat, end_beat)
```

Events that start at the same beat are ordered deterministically by source order, then by event kind, then by stable generated ID.

### 5.7 Scales and pitch

The first non-MVP pitch system should support:

```malison
scale F minor
scale F phrygian
scale C chromatic
```

Pitch literals:

```malison
F1
Gb1
C#3
A4
```

Microtonal support may be deferred, but the IR should not make it impossible.

### 5.8 Minimal grammar profile

The first implementation should publish a grammar for the supported profile. This sketch is intentionally small:

```ebnf
file        = [ language_decl ], working ;
language_decl = "language", version ;
working     = "working", string, working_block ;
working_block = "{", working_statement*, "}" ;
rite_block  = "{", rite_statement*, "}" ;

working_statement
            = tempo_decl
            | meter_decl
            | seed_decl
            | daemon_decl
            | spell_decl
            | rite_decl
            | evoke_decl ;

rite_statement
            = invoke_stmt ;

tempo_decl  = "tempo", number, [ "bpm" ] ;
meter_decl  = "meter", integer, "/", integer ;
seed_decl   = "seed", string ;

daemon_decl = "daemon", ident, "=", daemon_kind, [ string ], [ daemon_block ] ;
daemon_block = "{", param*, "}" ;
daemon_kind = "sample" | "saw_sub" ;

spell_decl  = "spell", ident, "=", pattern_expr ;
pattern_expr = "pattern", string
             | "notes", string ;

rite_decl   = "rite", ident_or_string, "bars", integer, rite_block ;
invoke_stmt = "invoke", ident, [ "with", ident ], [ "every", duration ], param* ;
evoke_decl  = "evoke", "wav", string ;

param       = ident, value ;
value       = number | string | pitch | duration ;
duration    = fraction | number, ( "beat" | "beats" | "bars" | "sec" ) ;
```

Within a `rite` block, version `0.1` allows only `invoke_stmt`. Later profiles may add `bind`, `raise`, `lower`, and `banish`.

## 6. Pattern System

The pattern system is central to Malison. Patterns produce timed symbolic events before they become audio events.

### 6.1 String rhythm patterns

```malison
pattern "x--- x--- x-x- x---"
```

Recommended interpretation:

```text
x = event
- = rest
space = visual separator
```

In version `0.1`, each non-space character is one pattern step. A rhythm pattern has no absolute duration until it is invoked. The invocation interval determines the step duration:

```malison
invoke kick with kickline every 1/16
```

If `every` is omitted for a rhythm pattern, the default step duration is `1/16` in the current meter. This default is intentionally conventional rather than inferred from the string length.

Additional symbols may be added later:

```text
g = ghost note
X = accented event
. = tie or continuation
? = probabilistic event
```

### 6.2 Euclidean rhythms

```malison
euclid(11, 16)
euclid(5, 13).rotate(2)
```

### 6.3 Note patterns

```malison
notes "F1 - F1 Gb1 | Ab1 - F1 Eb1"
```

The pipe character is a visual bar separator.

In version `0.1`, note patterns use the same step model as rhythm patterns:

```text
pitch literal = note event
-             = rest
|             = visual bar separator
```

A note event's duration is one step unless the invoking daemon defines a default envelope release that extends past the step boundary. Sustained notes and ties are deferred.

When a note pattern is invoked without `every`, the default step duration is `1/8`.

### 6.4 Pattern transforms

Initial transforms:

```malison
.rotate(steps)
.reverse()
.degrade(amount)
.humanize(amount)
.velocity(range)
.brittle(amount)
.mutate(probability)
.repeat(count)
.every(interval)
```

Transform semantics must be deterministic under seed.

Pattern transforms are deferred from version `0.1` except for backend-internal expansion of plain rhythm and note patterns. Method-chain syntax is reserved so early examples do not force parser compatibility breaks later.

### 6.5 Probability

```malison
invoke snare every 1/4 probability 0.75
spell ghosts = pattern "g--- --g-".probability(0.4)
```

Probabilistic expansion happens during IR generation, using a named random stream derived from the working seed.

Probability is deferred from version `0.1`.

## 7. Arrangement System

A working contains rites. Rites are sequenced by source order unless explicit placement is used.

```malison
rite intro bars 16 { ... }
rite drop bars 32 { ... }
rite breakdown bars 16 { ... }
```

Future explicit placement:

```malison
rite drop at bar 17 bars 32 { ... }
rite reprise at 2:15 bars 16 { ... }
```

The compiler should reject overlapping rites unless the user declares intentional layering.

```malison
rite drone_layer at bar 1 bars 64 layer {
  invoke drone
}
```

For version `0.1`, rites are strictly sequential and cannot overlap. A rite's start beat is the sum of the durations of all previous rites. A rite's duration is:

```text
bars * meter_numerator
```

For example, `rite main bars 16` in `4/4` lasts `64` beats.

### 7.1 Invocation semantics

An `invoke` statement schedules events for a daemon within the containing rite.

```malison
invoke kick with kicks every 1/16 gain -6
invoke bass with bassline every 1/8 cutoff 300
```

For version `0.1`:

* invoking a sample daemon with a rhythm pattern creates one sample trigger per active rhythm step
* invoking a synth daemon with a note pattern creates one note event per note step
* invoking a daemon without a pattern creates one event at the start of the rite
* event generation stops at the rite boundary
* per-invocation parameters are copied onto each generated event

Daemons are reusable definitions, not automatically running processes. Lifecycle controls such as `banish`, persistent drones, and continuous automation are deferred until the language has explicit process semantics.

## 8. Tension and Aesthetic Control

Malison should support global or local aesthetic control signals.

```malison
raise tension 0.2 -> 0.8 over 32 bars
lower density 0.9 -> 0.3 over 8 bars
```

These are not vague prompts. They are named control signals that can be bound to concrete parameters.

```malison
bind bass.cutoff to tension 180 -> 1200
bind hats.density to tension 0.2 -> 0.9
bind reverb.wet to spaciousness 0.1 -> 0.5
```

The first implementation should treat aesthetic variables as normalized control streams in `[0, 1]` unless otherwise specified.

Built-in controls:

```text
tension
density
instability
harshness
spaciousness
decay
pressure
brittleness
degradation
```

Tension and other aesthetic controls are deferred from version `0.1`. They should enter the language only after plain event generation, parameter binding, and automation curves are stable, because otherwise the compiler has no clear target for lowering aesthetic intent.

## 9. Synthesis and Sound Sources

Malison must support several sound-source categories.

### 9.1 Sample playback

```malison
daemon kick = sample "kick_909_dark" {
  tune -2
  highpass 30
}
```

Samples resolve through project paths and installed libraries.

For version `0.1`, `sample` accepts only an explicit project-relative file path. Sample identifiers resolved through libraries or search paths are deferred until later profiles.

Sample daemon parameters:

```text
gain      bare dB
pan       unitless [-1, 1]
tune      semitones
highpass  hz
lowpass   hz
```

### 9.2 Sample kits

```malison
daemon hats = samplekit "corroded_hats" {
  brittle 0.35
}
```

### 9.3 Built-in synth archetypes

Initial archetypes:

```text
sine
saw
saw_sub
fm_bell
noise_burst
swarm
drone
acid
metal_hit
```

Version `0.1` should implement only `saw_sub`.

Required `saw_sub` semantics:

```text
input event: pitch, start beat, duration beats, params
oscillator: saw plus optional sub oscillator one octave below
envelope: fixed ADSR, attack 0.01 sec, decay 0.18 sec, sustain 0.65, release 0.08 sec
output: mono or stereo signal routed to master
```

Required `saw_sub` parameters:

```text
gain      bare dB
pan       unitless [-1, 1]
cutoff    hz
drive     unitless [0, 1]
```

The exact DSP implementation may vary by backend, but pitch, timing, envelope shape, and accepted parameter ranges are part of Malison semantics.

### 9.4 Explicit synthesis definitions

Later versions may allow lower-level synthesis graphs:

```malison
daemon bass = synth {
  osc saw freq note detune 0.08
  osc sine freq note -12 gain 0.5
  filter ladder_lowpass cutoff 240 resonance 0.42
  drive tanh amount 0.45
  envelope adsr 0.01 0.18 0.7 0.08
}
```

This should be deferred until the higher-level language proves itself.

## 10. Routing and Mixing

Routing is defined through circles.

```malison
circle drums -> master { ... }
circle bass -> master { ... }
circle send_reverb -> master { ... }
```

Daemons choose output circles:

```malison
daemon kick = sample "kick" { out drums }
```

The compiler must validate routing graphs:

* no unresolved circles
* no illegal cycles unless feedback is explicitly supported
* master output exists
* wards are compatible with their target
* sidechain sources exist

In version `0.1`, routing is implicit. All daemons output to `master`, and `master` renders to the selected `evoke wav` target. Explicit `circle` declarations and `out` parameters are reserved for later profiles.

## 11. Effects

Effects may appear inside daemons or circles.

```malison
effect saturator amount 0.35
effect delay time 3/16 feedback 0.42 wet 0.25
effect reverb room "concrete" decay 3.2 wet 0.18
effect spectral_smear amount 0.4
```

The first backend should implement a minimal effect vocabulary and reject unsupported effects clearly.

Initial effects:

```text
gain
pan
filter
highpass
lowpass
saturator
distortion
delay
reverb
compressor
limiter
bitcrush
spectral_smear
```

Effects are deferred from version `0.1`, except for simple per-event parameters implemented directly by supported daemon schemas: `gain`, `pan`, `highpass`, `lowpass`, and `cutoff`.

## 12. Compiler Architecture

### 12.1 Frontend

The frontend performs:

1. Lexing
2. Parsing
3. AST construction
4. Name resolution
5. Type/unit checking
6. Semantic validation
7. Diagnostics

### 12.2 Malison IR

The IR is the canonical backend-neutral representation.

It should represent:

* metadata
* tempo/meter/scale
* resolved dependencies
* random seed streams
* daemons
* circles
* spells
* rites
* expanded events
* automation curves
* routing graph
* effect graph
* render targets
* source maps

### 12.3 Backend generation

Initial backend:

```text
SuperCollider non-realtime render
```

Possible future backends:

```text
MIDI + stems
Ableton project export
VST host automation
WebAudio
Rust DSP engine
SuperCollider realtime session
```

Backend generation must not change musical semantics. If a backend cannot represent a requested behavior, compilation should fail with a specific diagnostic.

Each backend must publish a capability table used by semantic validation:

```json
{
  "backend": "supercollider-nrt",
  "profile": "0.1",
  "sample_rates": [44100, 48000, 96000],
  "bit_depths": [16, 24, 32],
  "daemons": ["sample", "saw_sub"],
  "params": {
    "sample": ["gain", "pan", "tune", "highpass", "lowpass"],
    "saw_sub": ["gain", "pan", "cutoff", "drive"]
  },
  "effects": []
}
```

The language spec defines accepted constructs and semantics. A backend capability table defines whether a particular backend can realize those constructs. Backend-specific escape hatches must be explicit and should not appear in version `0.1`.

## 13. Intermediate Representation Sketch

A simplified IR fragment:

```json
{
  "working": "Wound Engine",
  "language": "0.1",
  "tempo": 132,
  "meter": [4, 4],
  "scale": { "root": "F", "mode": "phrygian" },
  "seed": "black-furnace",
  "duration_beats": 256,
  "circles": [
    { "id": "master", "out": null },
    { "id": "drums", "out": "master" }
  ],
  "daemons": [
    {
      "id": "kick",
      "kind": "sample",
      "sample": "kick_909_dark",
      "out": "drums"
    }
  ],
  "events": [
    {
      "id": "evt_000001",
      "kind": "trigger",
      "time_beats": 0,
      "duration_beats": 0.25,
      "daemon": "kick",
      "params": { "gain_db": -6.0 },
      "source": "src/main.rite:42:5"
    },
    {
      "id": "evt_bassline_main_0000",
      "kind": "note",
      "time_beats": 64,
      "duration_beats": 0.5,
      "daemon": "bass",
      "pitch": { "name": "F1", "midi": 29 },
      "params": { "cutoff_hz": 300 },
      "source": "src/main.rite:53:5"
    }
  ],
  "automation": [
    {
      "target": "bass.cutoff",
      "start_beats": 64,
      "duration_beats": 64,
      "from": 180,
      "to": 1200,
      "curve": "exponential"
    }
  ]
}
```

## 14. Determinism

Malison must define deterministic behavior for:

* pattern expansion
* probabilistic events
* random velocities
* humanization
* mutation
* sample selection
* generated IDs
* event ordering
* backend render configuration

Recommended rule:

```text
Every stochastic operation receives a derived random stream:
hash(working_seed, semantic_path, operation_kind, local_index)
```

This prevents unrelated edits from unnecessarily changing all downstream random choices.

`semantic_path` should be based on stable language entities rather than physical line numbers. Examples:

```text
working:Wound Engine/rite:main/invoke:2/pattern_step:14
working:Wound Engine/spell:hatline/transform:degrade/step:7
```

Source locations remain in the IR for diagnostics and inspection, but they should not be the primary source of random-stream identity. If two operations have the same semantic path after a refactor, the compiler should preserve their random stream identity.

Generated IDs should be deterministic and derived from semantic path plus local index. They should not depend on traversal order alone.

## 15. Diagnostics

Diagnostics should be precise and source-mapped.

Examples:

```text
error[E021]: unresolved daemon `basss`
  src/main.rite:52:10
  invoke basss with bassline
         ^^^^^
help: did you mean `bass`?
```

```text
error[E044]: illegal routing cycle
  circle drums -> master
  circle master -> drums
```

```text
error[E071]: backend `supercollider` does not support effect `spectral_freeze`
  src/main.rite:88:12
```

Diagnostics must favor exact failure over silent approximation.

## 16. Command-Line Interface

Initial CLI:

```bash
malison init <name>
malison check <file>
malison render <file> --out <path>
malison ir <file>
malison events <file>
malison graph <file>
malison fmt <file>
malison scry <file>
```

### 16.1 `check`

Parses and validates without rendering.

```bash
malison check src/main.rite
```

### 16.2 `render`

Compiles and renders audio.

```bash
malison render src/main.rite --out renders/main.wav
```

Options:

```bash
--backend supercollider
--seed override-seed
--stems
--sample-rate 48000
--bit-depth 24
--dry-run
--force
```

### 16.3 `scry`

Inspects source, event expansion, routing, and automation in a human-readable form.

```bash
malison scry src/main.rite
```

This command should be useful for debugging musical causality.

## 17. Project Structure

Recommended project layout:

```text
wound-engine/
  malison.toml
  src/
    main.rite
    drums.rite
    bass.rite
  samples/
    kicks/
    machinery/
  renders/
  stems/
  build/
```

Manifest example:

```toml
[project]
name = "wound-engine"
version = "0.1.0"

[render]
sample_rate = 48000
bit_depth = 24
backend = "supercollider"

[paths]
samples = ["samples", "~/Malison/samples"]
```

## 18. Minimal Viable Product

The MVP should compile a single `.rite` file to a `.wav` file.

### 18.1 MVP features

Required:

* `language 0.1`
* `working`
* `tempo`
* `meter`
* `seed`
* sample daemons
* one built-in synth daemon
* string rhythm patterns
* note patterns
* ordered rites
* `invoke`
* implicit `master` output
* simple gain/pan/filter parameters
* deterministic pattern expansion
* deterministic event IDs
* source-mapped diagnostics
* SuperCollider non-realtime backend
* `check`, `render`, `events`

Deferred:

* package manager
* live coding
* VST hosting
* Ableton export
* circles and explicit routing
* wards
* effects
* sidechains
* pattern transforms
* probability and humanization
* aesthetic controls such as `tension`
* full synthesis graph language
* complex mix analysis
* semantic AI transforms
* microtonality
* GUI

### 18.2 MVP example target

This should render successfully:

```malison
language 0.1

working "First Working" {
  tempo 128
  meter 4/4
  seed "first"

  daemon kick = sample "samples/kick.wav" {
    gain -3
  }

  daemon bass = saw_sub {
    cutoff 300
    drive 0.3
  }

  spell kicks = pattern "x--- x--- x-x- x---"
  spell bassline = notes "F1 - F1 Gb1 | Ab1 - F1 Eb1"

  rite main bars 16 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "renders/first-working.wav"
}
```

### 18.3 Version `0.1` implementation contract

This section resolves the remaining choices needed for the first implementation. If it conflicts with earlier exploratory examples, this section wins for version `0.1`.

#### Lexical rules

Version `0.1` source is UTF-8 text.

```text
line comment       // until end of line
block comment      /* until matching */
identifier         [a-z_][a-z0-9_]*
number             -?[0-9]+(\.[0-9]+)?
integer            [0-9]+
fraction           [0-9]+ "/" [0-9]+
version            [0-9]+ "." [0-9]+
string             double-quoted UTF-8, with \" \\ \n \t escapes
pitch              [A-G](b|#)?[0-9]+
```

Keywords are reserved and cannot be used as identifiers in version `0.1`: `language`, `working`, `tempo`, `meter`, `seed`, `daemon`, `sample`, `saw_sub`, `spell`, `pattern`, `notes`, `rite`, `bars`, `invoke`, `with`, `every`, `evoke`, and `wav`.

Whitespace is insignificant except inside strings and pattern bodies.

#### Required declarations

A valid version `0.1` file must contain:

* exactly one `language 0.1` declaration
* exactly one `working`
* exactly one `tempo`
* exactly one `meter`
* exactly one `seed`
* at least one `rite`
* exactly one `evoke wav`

Duplicate declarations are errors unless this spec explicitly permits repetition. `daemon`, `spell`, and `rite` declarations may repeat, but their names must be unique within the working.

#### Pattern expansion

Pattern steps repeat from the start until the containing rite ends. Expansion truncates at the rite boundary and never schedules an event whose start beat is outside the rite.

For rhythm patterns:

```text
x -> trigger event
- -> rest
space -> ignored
```

For note patterns:

```text
pitch -> note event
-     -> rest
|     -> ignored
space -> separator
```

Any other pattern character is an error in version `0.1`.

#### Event defaults

All generated events include:

```json
{
  "id": "stable event id",
  "kind": "trigger or note",
  "time_beats": 0,
  "duration_beats": 0.25,
  "daemon": "daemon_name",
  "params": {},
  "source": {
    "file": "src/main.rite",
    "line": 1,
    "column": 1
  }
}
```

Sample trigger events use the invocation step duration as `duration_beats`; the backend may allow the sample tail to decay naturally. Synth note events use the invocation step duration as `duration_beats`.

Default parameters:

```text
gain      0 dB
pan       0
highpass  none
lowpass   none
cutoff    1200 hz for saw_sub
drive     0 for saw_sub
tune      0 semitones for sample
```

Per-invocation parameters override daemon defaults for the generated events. Daemon defaults override built-in defaults.

#### Sample resolution

Version `0.1` sample paths are interpreted relative to the project root, which is the directory containing `malison.toml` if present, otherwise the current working directory. `~`, glob patterns, sample-library identifiers, and remote URLs are errors in version `0.1`.

The compiler must fail during `check` if a referenced sample file does not exist.

#### IR schema

The `events` command emits deterministic JSON with this top-level shape:

```json
{
  "language": "0.1",
  "working": "First Working",
  "tempo_bpm": 128,
  "meter": [4, 4],
  "seed": "first",
  "duration_beats": 64,
  "daemons": [],
  "spells": [],
  "rites": [],
  "events": []
}
```

JSON object keys should be emitted in a stable order. Event arrays are sorted by `time_beats`, then source order, then `kind`, then `id`.

#### CLI behavior

Commands return exit code `0` on success and nonzero on error.

```text
malison check <file>          validates and prints diagnostics only
malison events <file>         validates and writes JSON events to stdout
malison render <file>         validates, generates backend code, and renders audio
```

Diagnostics are written to stderr. `events` writes no non-JSON text to stdout. `render` uses the `evoke wav` path unless `--out` is provided, in which case the CLI option wins.

By default, `render` refuses to overwrite an existing output file. `--force` allows overwrite.

#### Render defaults

Version `0.1` render defaults:

```text
backend       supercollider
sample rate   48000
bit depth     24
channels      2
tail          2.0 sec after the final event
```

The first backend should generate a SuperCollider source script and run it in non-realtime mode. Direct OSC score generation may replace this later without changing Malison semantics.

## 19. Non-Goals

Malison is not initially:

* a DAW replacement
* a general-purpose programming language
* a prompt-to-music generator
* a mastering suite
* a notation system for acoustic ensembles
* a visual modular synth
* a plugin standard

It may integrate with some of these later. The initial goal is narrower: compile executable electronic scores to audio.

## 20. Design Principles

### 20.1 Explicit beats implicit

The compiler should avoid guessing. If the source is ambiguous, reject it with a diagnostic.

### 20.2 Musical intent should survive backend changes

A Malison program should not depend on accidental SuperCollider behavior unless it explicitly uses a SuperCollider escape hatch.

### 20.3 Text is the source of truth

Rendered audio, stems, graphs, and backend files are build artifacts.

### 20.4 Determinism is a feature

Generative music must be reproducible. Variation should be controlled by source and seed, not accidental runtime state.

### 20.5 Dark aesthetics should be structural

Darkness should emerge from rhythm, timbre, harmony, pressure, space, degradation, and arrangement. The language should provide meaningful controls over those structures rather than merely applying dark labels.

## 21. Open Questions

1. Should `daemon` be public syntax, or should the less occult `instrument` be used in the first public release?
2. How should source maps represent events produced by transformed patterns in later profiles?
3. What is the smallest useful dark-electronic demo track that proves the concept?

## 22. Recommended Implementation Path

### Phase 0: Prototype

* Parse one `language 0.1` file.
* Expand patterns into events.
* Emit source-mapped JSON events.
* Generate a simple SuperCollider script.
* Render WAV through SuperCollider.
* Support one sample daemon and one synth daemon.

### Phase 1: Real frontend

* Formal grammar.
* Source spans.
* Diagnostics.
* Name resolution.
* Unit checking.

### Phase 2: Stable IR

* JSON IR output.
* Event expansion.
* Automation curves.
* Routing graph.
* Source maps.

### Phase 3: Musical usefulness

* Pattern transforms.
* Seeded randomness.
* Rites and arrangement.
* Circles and effects.
* Stems.

### Phase 4: Dark-electronic expressiveness

* Tension curves.
* Degradation controls.
* Industrial hit generators.
* Drone and swarm daemons.
* Spectral smear/freeze effects.
* Sidechain-native bass/drum workflows.

### Phase 5: Tooling

* Formatter.
* Language server.
* VS Code extension.
* Graph visualization.
* Audio preview cache.
* Semantic diff.

## 23. Summary

Malison is a compiler-oriented music system for writing dark electronic tracks as executable source code. It separates musical semantics from audio backend mechanics, preserves deterministic generativity, and treats rendered audio as a build artifact.

The project should begin with a narrow but complete path from `.rite` source to `.wav` render. Once that path works, the language can grow into the more interesting territory: tension, ritual structure, industrial timbre, degradation, controlled randomness, and source-level musical refactoring.

The first successful Malison demo should make the thesis obvious in one command:

```bash
malison render wound-engine.rite
```

A source file enters the system. A dark electronic track comes out.
