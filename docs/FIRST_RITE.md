# Tutorial: First Rite

Create a project:

```bash
mkdir my-working
cd my-working
mkdir samples renders
```

Create `malison.toml`:

```toml
[project]
name = "my-working"

[render]
backend = "rust"
sample_rate = 48000
bit_depth = 24

[paths]
samples = "samples"
renders = "renders"
build = "build"
```

Put a mono or stereo WAV file at `samples/kick.wav`.

Create `main.rite`:

```text
language 0.1

working "My Working" {
  tempo 126
  meter 4/4
  seed "first"

  daemon kick = sample "kick.wav" { gain -4 }
  daemon bass = saw_sub { cutoff 300 drive 0.25 gain -10 }

  spell kicks = pattern "X--- x--- x-g- x---"
  spell bassline = notes "F1 - F1 Gb1"

  rite main bars 4 {
    invoke kick with kicks every 1/16
    invoke bass with bassline every 1/8
  }

  evoke wav "first.wav"
}
```

Render it:

```bash
malison render main.rite --force
```

The output is `renders/first.wav`.
