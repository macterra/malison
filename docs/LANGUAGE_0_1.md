# Language Reference 0.1

Malison source files use the `.rite` extension.

The supported top-level form is:

```text
language 0.1

working "Name" {
  tempo 128
  meter 4/4
  seed "seed"
  ...
}
```

Supported declarations:

* `circle name -> parent { effect saturator amount 0.2 ward limiter ceiling -1 }`
* `daemon name = sample "path.wav" { ... }`
* `daemon name = samplekit "folder" { ... }`
* `daemon name = saw_sub { ... }`
* `daemon name = drone { ... }`
* `daemon name = noise_burst { ... }`
* `daemon name = swarm { ... }`
* `daemon name = metal_hit { ... }`
* `spell name = pattern "x---"`
* `spell name = notes "F1 - Gb1 -"`
* `spell name = euclid(5, 16).rotate(1)`
* `rite name bars 4 { ... }`
* `rite name at bar 5 bars 8 { ... }`
* `rite name at 1:30 bars 4 layer { ... }`
* `evoke wav "renders/out.wav"`

Inside a `working` block, `include "relative/path.rite"` expands another `.rite` fragment before parsing. Included fragments may contain declarations or rite blocks without their own `language` or `working` wrapper.

Sample paths are resolved first relative to the project root, then through `[paths].samples`, then each folder in `[paths].sample_libraries`.

Supported rite statements:

* `invoke daemon`
* `invoke daemon with spell every 1/16`
* `banish daemon`
* `raise tension 0.2 -> 0.8`
* `lower degradation 0.6 -> 0.1 curve stepped`
* `bind bass.cutoff to tension 180 -> 1200 curve exponential`

`banish` emits a lifecycle event at the start of its rite and truncates any already-running continuous event for the named daemon. Automation curves may be `linear`, `exponential`, or `stepped`; exponential curves require positive endpoints.
Bindings map normalized control streams to numeric daemon parameters. For discrete events, the compiler writes the lowered parameter value into matching event parameters.

Common daemon parameters:

* Samples and sample kits: `gain`, `pan`, `tune`, `start`, `end`, `normalize on|off`
* `saw_sub`: `gain`, `pan`, `cutoff`, `drive`, `attack`, `decay`, `sustain`, `release`, `detune`, `sub`, `resonance`

Supported hard wards are `ward limiter ceiling`, `ward loudness max`, and `ward gain max`. Limiter wards are also applied as simple output limiters in the Rust backend.

Supported pattern transforms:

* `.rotate(steps)`
* `.reverse()`
* `.repeat(count)`
* `.every(interval)`
* `.degrade(amount)`
* `.humanize(amount)`
* `.mutate(probability)`
* `.velocity(rand(min, max))`

Supported rhythm pattern characters:

* `x`: normal event
* `X`: accented event
* `g`: ghost event
* `-`: rest

Supported note tokens are pitch names such as `F1`, `Gb1`, or `C2`, plus `-` for rests.
