rust_collatz_solution
=====================

Collatz explorer with big‑integers and a tiny live visualizer. One command to try it.

<img width="500" height="531" alt="Visualizer" src="https://github.com/user-attachments/assets/05894ec5-fdf6-407f-ba98-85f0ee8044a4" />

Quick Start
-----------

- Easiest: run with the defaults (random + viz)
  - `cargo run --release --`
- Want more motion? draw more often
  - `cargo run --release -- --viz-interval 100 --viz-max-steps 3000`
- Sequential (no random), start from a value
  - `cargo run --release -- --no-random --start 100000000000000000000`

Tip: when using `cargo run`, put program flags after `--`.

What it does
------------

- Scans huge numbers (BigUint, up to 2^2000+)
- Detects loops/runaways with O(1) memory
- Writes to disk only when a finding occurs (`solution.txt`)
- Visualizer shows a current line and the number being tested
- Even with visualizer, it still runs many lines at full speed in the background

Handy flags
-----------

- `--random` / `--no-random` (default: random on)
- `--viz` / `--no-viz` (default: viz on)
- `--viz-interval <N>`: send a new seed to the GUI every N starts (default 1000)
- `--viz-max-steps <N>`: line window width (default 10_000)
- `--start <DECIMAL>` and `--count <N>` for sequential runs

Releases
--------

GitHub Actions builds downloadable archives per commit (named by short SHA) for:

- Linux x86_64 (GNU)
- Windows x86_64 (MSVC)
- Windows i686 (MSVC)

Download from Releases and run the executable. That’s it.
