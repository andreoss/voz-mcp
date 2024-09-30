# voz

Local, offline text-to-speech in a single Rust binary. All synthesis runs on the
host; no cloud round-trips.

## Features

- One binary `voz`: CLI mode and an MCP server over stdio.
- Backends, discovered at startup:
  - Neural backend (default): CPU runtime over local weights, 24 kHz mono S16 WAV.
  - Fallback backend: VITS onnx engine as a subprocess, 22.05 kHz mono S16 WAV,
    one voice file per language, RTF ~0.7 against ~4 for neural.
  - Null adapter when neither is present: input is accepted, no speech produced.
- 10 languages: `ru en es de fr it pt zh ja ko` (ISO 639-1). The neural backend
  covers all 10. The fallback backend covers 9: there is no usable `ja` voice, so
  `ja` returns a per-language error while the fallback backend is active.
- `rate` (50..500) and `pitch` (0..99). The fallback backend honours both:
  rate maps to phoneme length, pitch shifts the signal with a length
  compensation that holds duration to within about ten percent. The neural
  backend has no length control, so it refuses both outright rather than
  accepting them and silently ignoring them.

## Requirements

- Rust toolchain (edition 2024).
- At least one backend provisioned under the data root, by default
  `$XDG_DATA_HOME/voz` (else `~/.local/share/voz`, else
  `/usr/local/share/voz`):
  - Neural: `qwentts/build/qwen-tts` plus a talker and a tokenizer `.gguf` under
    `gguf/`.
  - Fallback: the whole `piper/` directory — `bin/piper` with `bin/espeak-ng-data/`
    beside it, `lib64/`, and voices in `piper/voices/` (`<lang>_*.onnx` with a
    matching `.onnx.json`). The engine resolves its libraries through an rpath of
    `$ORIGIN/../lib64`, so copying only `bin/piper` and the voices fails with
    `error while loading shared libraries: libpiper.so`.

- Linux x86-64; both runtimes are CPU-only.

An existing tree elsewhere can be adopted by linking it at the data root:

```sh
ln -s /path/to/tree "${XDG_DATA_HOME:-$HOME/.local/share}/voz"
```

## Build

```sh
cargo build --release
```

The binary is `target/release/voz`.

## CLI usage

```sh
voz "hello world" --lang en --out hi.wav
voz "привет мир" --lang ru --rate 170 --pitch 60 --out hi_ru.wav
```

- `--lang` one of `ru en es de fr it pt zh ja ko` (default `en`).
- `--rate` 50..500, `--pitch` 0..99; both optional. Both need the fallback
  backend; the neural backend rejects either (exit 1).
- `--out` destination path; omit it and the file stays under `VOZ_OUT_DIR` and
  its path is printed.
- Exactly one positional text argument.

Exit 2 on argument errors, exit 1 when synthesis fails or no backend is
available.

## MCP server

```sh
voz mcp
```

JSON-RPC over stdio (`initialize`, `tools/list`, `tools/call`). Tools:

- `speak { text, lang, rate?, pitch? }` -> `{ path, lang }`.
- `readback {}` -> `{ items: [{ name, path, bytes }] }`, the files under
  `VOZ_OUT_DIR`.

Responses must be correlated by request id, never by arrival order.

Register the binary with any MCP client that launches stdio servers; the entry
names the command and the environment, e.g.

```json
{
  "type": "local",
  "command": ["/abs/path/to/voz", "mcp"],
  "environment": { "VOZ_OUT_DIR": "/abs/path/to/audio" },
  "enabled": true
}
```

Restart the client after editing its config; configs load once at startup.

## Environment

| Variable | Default | Purpose |
|----------|---------|---------|
| `VOZ_OUT_DIR` | `./audio` | Output directory for synthesized speech |
| `VOZ_BACKEND` | `auto` | Backend selection (see below) |
| `VOZ_NEURAL_ROOT` | `$XDG_DATA_HOME/voz` | Data root override; ignored if the path does not exist |
| `VOZ_NEURAL_BIN` | auto-discovered | Neural engine executable; weights are still resolved under the modelz root |
| `VOZ_PIPER_BIN` | auto-discovered | Fallback engine executable; voices are read from `../voices` next to it |
| `VOZ_TIMEOUT_SECS` | `600` | Synthesis budget in whole seconds, `1..86400` |

`VOZ_BACKEND` values:

- `auto` — neural if discovered, else fallback if discovered, else the null
  adapter.
- `neural` — force the neural backend; error and exit if it is not discovered.
- `fallback` — force the fallback backend; error and exit if it is not
  discovered.
- `null` — force the null adapter (accepts input, produces no speech).

An unrecognized value is rejected at startup with the expected list.

`VOZ_TIMEOUT_SECS` bounds one synthesis. If the engine has not finished within
the budget it is killed and the call fails rather than waiting forever; an
unusable value is rejected at startup with the accepted range. Raise it for
long inputs on the neural backend, which runs slower than real time.

```sh
VOZ_BACKEND=fallback voz "hello" --lang en --out hi.wav
```

## QA gate

```sh
bash scripts/qa.sh
```

Runs the whole gate: build, unit tests, serialized e2e, clippy, a coverage
floor, and live smokes against the real stack — neural stdio, CLI, forced
fallback, bounded synthesis, rate and pitch, and an offline run in a network
namespace. `SKIP_SMOKE=1` stops before the smoke stages.

## License

GPL-3.0-only; see `LICENSE`. The speech engines run as separate processes and
none of their code is linked in, so their own terms do not reach this binary.
