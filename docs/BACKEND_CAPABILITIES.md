# Backend Capabilities

Malison currently supports two render backends:

* `rust`
* `supercollider`

The machine-readable capability table is available with:

```bash
malison capabilities
```

Both backends currently support:

* `sample`
* `samplekit`
* `saw_sub`
* `drone`
* `noise_burst`
* `swarm`
* `metal_hit`

Supported sample features:

* mono WAV
* stereo WAV
* `start_seconds`
* `end_seconds`
* explicit sample normalization with `normalize on`
* deterministic sample selection from a `samplekit` directory

Supported pattern features:

* rhythm patterns
* note patterns
* Euclidean rhythms
* accents and ghosts
* deterministic transforms
* seeded stochastic transforms

Known unsupported backend features:

* audio bus routing
* effect processors
* parameter bindings
