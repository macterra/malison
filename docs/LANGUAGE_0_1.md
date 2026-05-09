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

* `circle name -> parent { ward limiter ceiling -1 }`
* `daemon name = sample "path.wav" { ... }`
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

Supported rite statements:

* `invoke daemon`
* `invoke daemon with spell every 1/16`
* `banish daemon`
* `raise tension 0.2 -> 0.8`
* `lower degradation 0.6 -> 0.1 curve stepped`

`banish` emits a lifecycle event at the start of its rite and truncates any already-running continuous event for the named daemon. Automation curves may be `linear`, `exponential`, or `stepped`; exponential curves require positive endpoints.

Common daemon parameters:

* Samples: `gain`, `pan`, `tune`, `start`, `end`, `normalize on|off`
* `saw_sub`: `gain`, `pan`, `cutoff`, `drive`, `attack`, `decay`, `sustain`, `release`, `detune`, `sub`, `resonance`

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
