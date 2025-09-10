use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use num_bigint::BigUint;
use num_traits::{One, Zero};
use num_integer::Integer;

/// Compute the next Collatz value for arbitrary-precision integers
fn collatz_next(n: &BigUint) -> BigUint {
    if n.is_even() {
        n >> 1
    } else {
        n * BigUint::from(3u32) + BigUint::from(1u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    ReachesOne,          // enters the known 1-4-2 loop
    NontrivialCycle,     // enters a cycle that does not include 1
    StepsOverflow,       // exceeded u64::MAX steps while detecting
}

/// Use Floyd's cycle-finding algorithm with O(1) memory to classify the orbit.
fn detect_outcome(start: &BigUint) -> Outcome {
    // Advance one/two steps with overflow checks
    let mut step_count: u64 = 0;

    let mut tortoise = collatz_next(start);
    let mut hare = collatz_next(&collatz_next(&tortoise));

    loop {
        if tortoise == hare { break; }

        tortoise = collatz_next(&tortoise);
        // hare moves two steps
        hare = collatz_next(&collatz_next(&hare));

        step_count = step_count.wrapping_add(1);
        if step_count == u64::MAX { return Outcome::StepsOverflow; }
    }

    // We have a cycle; determine whether it contains 1 (i.e., 1-4-2 loop)
    let meet = tortoise;
    let mut x = meet.clone();
    loop {
        if x == BigUint::one() { return Outcome::ReachesOne; }
        x = collatz_next(&x);
        if x == meet { break; }
    }
    Outcome::NontrivialCycle
}

fn read_last_start(path: &str) -> Option<BigUint> {
    let f = File::open(path).ok()?;
    let reader = BufReader::new(f);
    let mut last: Option<BigUint> = None;
    for line in reader.lines() {
        if let Ok(l) = line {
            let t = l.trim();
            if t.is_empty() { continue; }
            if let Ok(v) = t.parse::<BigUint>() { last = Some(v); }
        }
    }
    last
}

fn parse_args() -> (Option<BigUint>, Option<u64>, bool, String, String, u64, bool) {
    let mut start: Option<BigUint> = None;
    let mut count: Option<u64> = None;
    let mut resume = true;
    let mut output = String::from("progress.txt");
    let mut solution = String::from("solution.txt");
    let mut progress_interval: u64 = 1000;
    let mut random = false;

    let mut args = env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" | "-s" => {
                if let Some(v) = args.next() { start = v.parse::<BigUint>().ok(); }
            }
            "--count" | "-n" => {
                if let Some(v) = args.next() { count = v.parse::<u64>().ok(); }
            }
            "--resume" => resume = true,
            "--no-resume" => resume = false,
            "--output" | "-o" | "--progress" => {
                if let Some(v) = args.next() { output = v; }
            }
            "--solution" => {
                if let Some(v) = args.next() { solution = v; }
            }
            "--progress-interval" | "-pi" => {
                if let Some(v) = args.next() { if let Ok(n) = v.parse::<u64>() { progress_interval = n; } }
            }
            "--random" => {
                random = true;
            }
            other => {
                // Fallback positional handling: first number => start, second => count
                if let Ok(v) = other.parse::<BigUint>() {
                    if start.is_none() { start = Some(v); continue; }
                }
                if let Ok(v) = other.parse::<u64>() {
                    if count.is_none() { count = Some(v); continue; }
                }
            }
        }
    }

    (start, count, resume, output, solution, progress_interval, random)
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let (start_arg, count_arg, resume, output, solution, progress_interval_arg, random) = parse_args();

    // Determine start number, possibly resuming from last written line
    // Default start is 2^68 when not resuming and not provided explicitly.
    let default_start: BigUint = BigUint::one() << 68; // 2^68
    let start: BigUint = if let Some(s) = start_arg {
        s
    } else if resume {
        match read_last_start(&output) {
            Some(last) => last + BigUint::one(),
            None => default_start,
        }
    } else {
        default_start
    };

    let count = count_arg; // None => run indefinitely
    let progress_interval = progress_interval_arg.max(1);

    let progress_path = Path::new(&output);
    let solution_path = Path::new(&solution);

    if random {
        eprintln!(
            "Random mode: sampling starts in [2^68, 2^2000-1]; progress in {}",
            progress_path.display()
        );
    } else {
        eprintln!("Starting at {}{} -> recording progress in {}", start,
            if resume { " (resume)" } else { "" }, progress_path.display());
    }

    // Ensure the progress file exists and reflects the starting point.
    write_progress_number(progress_path, &start)?;

    let mut processed: u64 = 0;

    // Minimal PRNG (xorshift128+)
    let mut rng = Rng::seeded();

    // Random range [2^68, 2^2000 - 1]
    let rand_low: BigUint = BigUint::one() << 68;
    let rand_high_inclusive: BigUint = (BigUint::one() << 2000) - BigUint::one();

    loop {
        let current: BigUint = if random {
            rng.gen_range_biguint(&rand_low, &rand_high_inclusive)
        } else {
            &start + &BigUint::from(processed)
        };
        let outcome = detect_outcome(&current);

        // Update progress occasionally (single-line file)
        if processed % progress_interval == 0 {
            write_progress_number(progress_path, &current)?;
        }

        if processed % 10000 == 0 {
            eprintln!("Processed {} starts (up to {})", processed, current);
        }

        match outcome {
            Outcome::ReachesOne => {
                // Keep scanning
            }
            Outcome::NontrivialCycle => {
                eprintln!("Found nontrivial loop starting from {}.", current);
                write_solution(solution_path, &format!("NONTRIVIAL_CYCLE_START {}", current))?;
                // Also update progress to this current number
                write_progress_number(progress_path, &current)?;
                break;
            }
            Outcome::StepsOverflow => {
                let kind = "RUNAWAY_STEPS_OVERFLOW_START";
                eprintln!("Detected runaway ({}). Start: {}", kind, current);
                write_solution(solution_path, &format!("{} {}", kind, current))?;
                write_progress_number(progress_path, &current)?;
                break;
            }
        }

        processed = processed.saturating_add(1);
        if let Some(limit) = count {
            if processed >= limit { break; }
        }
    }
    Ok(())
}

fn write_progress_number(path: &Path, value: &BigUint) -> std::io::Result<()> {
    // Truncate and write a single line with the current start
    let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
    writeln!(f, "{}", value.to_str_radix(10))?;
    f.flush()?;
    // Ensure durability so we don't lose our place on crashes
    f.sync_all()?;
    Ok(())
}

fn write_solution(path: &Path, line: &str) -> std::io::Result<()> {
    // Overwrite solution.txt with a single line describing the finding
    let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
    writeln!(f, "{}", line)?;
    f.flush()?;
    // Strong durability guarantee: never miss a found solution
    f.sync_all()?;
    Ok(())
}

// Simple xorshift128+ RNG for environments without external crates
struct Rng { s0: u64, s1: u64 }

impl Rng {
    fn seeded() -> Self {
        // Seed from current time; mix to avoid zeros
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
        let nanos = now.as_nanos() as u128;
        // Split into two 64-bit seeds and scramble
        let mut s0 = (nanos as u64).wrapping_mul(0x9E3779B97F4A7C15);
        let mut s1 = ((nanos >> 64) as u64).wrapping_mul(0xD1B54A32D192ED03);
        if s0 == 0 && s1 == 0 { s1 = 1; }
        Rng { s0, s1 }
    }

    fn next_u64(&mut self) -> u64 {
        // xorshift128+
        let mut s1 = self.s0;
        let s0 = self.s1;
        self.s0 = s0;
        s1 ^= s1 << 23;
        let s1_new = s1 ^ s0 ^ (s1 >> 18) ^ (s0 >> 5);
        self.s1 = s1_new;
        self.s1.wrapping_add(s0)
    }

    fn next_u128(&mut self) -> u128 {
        let hi = self.next_u64() as u128;
        let lo = self.next_u64() as u128;
        (hi << 64) | lo
    }

    fn gen_range_biguint(&mut self, low: &BigUint, high_inclusive: &BigUint) -> BigUint {
        use std::cmp::Ordering;
        if low >= high_inclusive { return low.clone(); }
        let one = BigUint::one();
        let span = high_inclusive - low + &one; // inclusive span
        // Precompute span byte length
        let span_bytes = span.to_bytes_be();
        let len = span_bytes.len().max(1);
        loop {
            // Generate len random bytes
            let mut buf = vec![0u8; len];
            let mut i = 0usize;
            while i < len {
                let r = self.next_u64();
                let mut chunk = r.to_be_bytes();
                let take = usize::min(8, len - i);
                buf[i..i+take].copy_from_slice(&chunk[..take]);
                i += take;
            }
            let v = BigUint::from_bytes_be(&buf);
            match v.cmp(&span) {
                Ordering::Less => return low + v,
                _ => continue, // reject and retry
            }
        }
    }
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
