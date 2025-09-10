rust_collatz_solution
=====================

A high‑performance Collatz explorer with big‑integer support and a lightweight visualizer.

- Arbitrary precision via `num-bigint::BigUint` (explores up to 2^2000 and beyond)
- O(1) memory cycle detection (Floyd’s algorithm)
- Random‑shot mode by default across [2^68, 2^2000−1]
- Minimal disk writes: only writes `solution.txt` when a finding occurs
- Optional 500×500 GUI (minifb) animates trajectories and shows the current seed

Quick Start
-----------

- Run with defaults (random scanning + viz):
  - `cargo run --release --`
- More frequent visualization updates:
  - `cargo run --release -- --viz-interval 100 --viz-max-steps 3000`
- Sequential (no random), starting at a value:
  - `cargo run --release -- --no-random --start 100000000000000000000`

Note: When using `cargo run`, pass program flags after `--` so Cargo doesn’t parse them.

CLI Flags
---------

- `--start <DECIMAL>`: starting value (decimal string); used when `--no-random`.
- `--count <N>`: stop after testing N starts (for experiments/testing).
- `--solution <PATH>`: path for the single‑line solution file (default `solution.txt`).
- `--random` / `--no-random` (default `--random`): random shots vs sequential scan.
- `--viz` / `--no-viz` (default `--viz`): enable/disable the visualizer.
- `--viz-interval <N>`: send a new seed to the GUI every N starts (default 1000).
- `--viz-max-steps <N>`: sliding window width for the animated line (default 10,000).

Output
------

- `solution.txt`: written and fsynced on first finding, then the app exits.
  - Formats: `NONTRIVIAL_CYCLE_START <n>` or `RUNAWAY_STEPS_OVERFLOW_START <n>`.

Releases
--------

This repo includes a GitHub Actions workflow that builds and publishes archives per commit (tagged by short SHA) for:

- Linux x86_64 (GNU)
- Windows x86_64 (MSVC)
- Windows i686 (MSVC)

Each archive includes the compiled executable. See the Releases tab for downloads.
