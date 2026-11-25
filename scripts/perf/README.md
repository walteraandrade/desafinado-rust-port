# Performance runs

Measures the app the way a person uses it: a release build with the `perf-probe` feature writes GPUI's profiler journal (input, draws and their layout, prepaint and paint phases, time per view, slow task polls on every thread) and the app's own `perf_span!` timers to `DESAFINADO_PERF_OUT` as JSON lines. A small XTEST driver ([`tools/xdrive`](../../tools/xdrive)) types, scrolls and presses keys in the window under Xvfb, with a fresh user data folder and the fake AI, so no run touches your settings or costs money.

```sh
cargo build --release -p desafinado --features perf-probe
cargo build --release --manifest-path tools/xdrive/Cargo.toml
Xvfb :97 -screen 0 1600x1000x24 &
scripts/perf/matrix.sh                 # every scenario; PREFIX=name labels the runs
scripts/perf/run.sh mine/type-large ~/.cache/desafinado-perf/docs/large.md scripts/perf/scenarios/type.txt
python3 scripts/perf/analyze.py ~/.cache/desafinado-perf/runs/mine/type-large
```

`gen_docs.py` writes the documents (2 KB to 2 MB, and one 55 KB paragraph); the same seed gives the same files. A scenario file is one driver command per line: `focus`, `move X Y`, `click X Y`, `type GAP_MS TEXT`, `key COMBO [REPEAT] [GAP_MS]`, `scroll up|down N GAP_MS`, `resize W H`, `sleep MS`.

Xvfb draws in software, so the time from a key to the frame on screen is longer than on a desktop with a GPU. Compare main-thread work (handlers, layout, prepaint, paint, task polls) and background CPU between builds; do not read the absolute key-to-frame latency as what a person sees. Runs are serial: each needs its own display (`PERF_DISPLAY`) and folder (`PERF_DIR`) to run beside another. Span totals overlap where one timed function calls another (`workspace.changed` holds `pane.on_edits`); `render` spans time building the elements, not their layout or paint, which the view timings cover. A `WARNING: the journal lost N events` line means the run is incomplete. `analyze.py` counts only what happens after the driver starts; startup work is in the run's `first_frame_ms` and in `perf.jsonl`.
