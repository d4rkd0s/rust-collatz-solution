# Rust Collatz Solution

A high-performance Rust implementation for searching potential counterexamples to the Collatz conjecture using arbitrary-precision arithmetic.

## Overview

This program searches for numbers that either:
- Enter a nontrivial cycle (not the known 1→4→2→1 loop)
- Create runaway sequences that exceed computational limits

The implementation uses Floyd's cycle-finding algorithm with O(1) memory complexity and supports arbitrary-precision integers via the `num-bigint` crate.

## Features

- **Arbitrary-precision arithmetic**: Handle extremely large numbers beyond standard integer limits
- **Memory-efficient cycle detection**: Uses Floyd's tortoise-and-hare algorithm
- **Resumable execution**: Automatically resume from the last processed number
- **Random sampling mode**: Test random numbers in configurable ranges
- **Progress tracking**: Real-time progress monitoring with configurable intervals
- **Solution detection**: Automatically saves any discovered counterexamples
- **Real-time visualization**: Optional graphical display of Collatz sequences

## Building

### Prerequisites
- Rust (latest stable version recommended)
- Cargo (comes with Rust)

### Build Commands

```bash
# Debug build
cargo build

# Optimized release build (recommended for actual searching)
cargo build --release

# Run tests
cargo test
```

## Running

### Basic Usage

```bash
# Run with default settings (starts at 2^68, runs indefinitely)
cargo run --release

# Run in random sampling mode
cargo run --release -- --random

# Start from a specific number
cargo run --release -- --start 1000000

# Process a limited number of values
cargo run --release -- --count 10000

# Start from specific number and process limited count
cargo run --release -- --start 500 --count 1000
```

### Command Line Options

| Option | Short | Description | Default |
|--------|-------|-------------|---------|
| `--start <N>` | `-s <N>` | Starting number for sequential search | 2^68 or resume point |
| `--count <N>` | `-n <N>` | Maximum numbers to process | Unlimited |
| `--random` | | Enable random sampling mode | Sequential mode |
| `--resume` | | Resume from last progress (default) | Enabled |
| `--no-resume` | | Start fresh, ignore previous progress | |
| `--output <FILE>` | `-o <FILE>` | Progress tracking file | `progress.txt` |
| `--progress <FILE>` | | Alias for `--output` | `progress.txt` |
| `--solution <FILE>` | | Solution output file | `solution.txt` |
| `--progress-interval <N>` | `-pi <N>` | Progress update frequency | 1000 |
| `--viz` | | Enable real-time visualization window | Disabled |
| `--viz-interval <N>` | | Visualization update frequency | 50000 |
| `--viz-max-steps <N>` | | Maximum steps to render in visualization | 10000 |

### Examples

```bash
# Quick test run starting from 1
cargo run --release -- --start 1 --count 100 --no-resume

# Random sampling for 1 million iterations
cargo run --release -- --random --count 1000000

# Resume previous session with custom progress file
cargo run --release -- --output my_progress.txt

# High-frequency progress updates
cargo run --release -- --progress-interval 100

# Start from a very large number
cargo run --release -- --start 123456789012345678901234567890

# Enable visualization for small numbers
cargo run --release -- --start 27 --count 10 --viz

# Visualization with custom update frequency
cargo run --release -- --random --viz --viz-interval 1000 --viz-max-steps 5000
```

## Output Files

### progress.txt
Contains the last processed number. The program automatically resumes from this point when restarted (unless `--no-resume` is specified).

### solution.txt
If a counterexample is found, this file will contain one of:
- `NONTRIVIAL_CYCLE_START <number>` - Found a cycle that doesn't include 1
- `RUNAWAY_STEPS_OVERFLOW_START <number>` - Found a sequence that exceeded step limits

## Algorithm Details

### Collatz Function
For any positive integer n:
- If n is even: n → n/2
- If n is odd: n → 3n + 1

### Cycle Detection
Uses Floyd's cycle-finding algorithm:
1. Initialize tortoise and hare pointers
2. Advance tortoise by 1 step, hare by 2 steps each iteration
3. When they meet, a cycle is detected
4. Determine if the cycle contains 1 (normal) or not (counterexample)

### Random Mode
When `--random` is specified:
- Samples numbers uniformly from the range [2^68, 2^2000 - 1]
- Uses a custom xorshift128+ PRNG for reproducible results
- Useful for statistical sampling of very large number spaces

### Visualization Mode
When `--viz` is specified:
- Opens a 500x500 pixel window showing real-time Collatz sequence plots
- X-axis represents steps in the sequence, Y-axis represents bit-length of values
- White lines trace the trajectory from start to 1 (or until max steps)
- Press ESC to close the visualization window
- Updates at configurable intervals to balance performance and visual feedback

## Performance Tips

1. **Always use `--release`**: Debug builds are significantly slower
2. **Adjust progress interval**: Lower values provide more frequent updates but slight performance overhead
3. **Use SSD storage**: Frequent progress writes benefit from fast disk I/O
4. **Consider random mode**: For very large starting points, random sampling may find counterexamples faster

## Implementation Notes

- Uses `num-bigint` for arbitrary-precision arithmetic
- Implements overflow detection to prevent infinite loops
- All file operations include explicit `sync_all()` calls for crash safety
- Progress is atomic - either fully written or not written at all

## License

This project is open source. Please check the repository for license details.