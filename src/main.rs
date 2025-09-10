use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

/// Compute the next Collatz value, returning None on u128 overflow
fn collatz_next(n: u128) -> Option<u128> {
    if n % 2 == 0 {
        Some(n / 2)
    } else {
        n.checked_mul(3)?.checked_add(1)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Outcome {
    ReachesOne,          // enters the known 1-4-2 loop
    NontrivialCycle,     // enters a cycle that does not include 1
    Overflow,            // arithmetic overflow while iterating
    StepsOverflow,       // exceeded u64::MAX steps while detecting
}

/// Use Floyd's cycle-finding algorithm with O(1) memory to classify the orbit.
fn detect_outcome(start: u128) -> Outcome {
    // Advance one/two steps with overflow checks
    let mut step_count: u64 = 0;

    let mut tortoise = match collatz_next(start) {
        Some(v) => v,
        None => return Outcome::Overflow,
    };
    let mut hare = match collatz_next(tortoise).and_then(collatz_next) {
        Some(v) => v,
        None => return Outcome::Overflow,
    };

    loop {
        if tortoise == hare { break; }

        tortoise = match collatz_next(tortoise) { Some(v) => v, None => return Outcome::Overflow };
        // hare moves two steps
        hare = match collatz_next(hare).and_then(collatz_next) { Some(v) => v, None => return Outcome::Overflow };

        step_count = step_count.wrapping_add(1);
        if step_count == u64::MAX { return Outcome::StepsOverflow; }
    }

    // We have a cycle; determine whether it contains 1 (i.e., 1-4-2 loop)
    let meet = tortoise;
    let mut x = meet;
    loop {
        if x == 1 { return Outcome::ReachesOne; }
        x = match collatz_next(x) { Some(v) => v, None => return Outcome::Overflow };
        if x == meet { break; }
    }
    Outcome::NontrivialCycle
}

fn read_last_start(path: &str) -> Option<u128> {
    let f = File::open(path).ok()?;
    let reader = BufReader::new(f);
    let mut last: Option<u128> = None;
    for line in reader.lines() {
        if let Ok(l) = line {
            let t = l.trim();
            if t.is_empty() { continue; }
            if let Ok(v) = t.parse::<u128>() { last = Some(v); }
        }
    }
    last
}

fn parse_args() -> (Option<u128>, Option<u64>, bool, String, String, u64) {
    let mut start: Option<u128> = None;
    let mut count: Option<u64> = None;
    let mut resume = true;
    let mut output = String::from("progress.txt");
    let mut solution = String::from("solution.txt");
    let mut progress_interval: u64 = 1000;

    let mut args = env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--start" | "-s" => {
                if let Some(v) = args.next() { start = v.parse::<u128>().ok(); }
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
            other => {
                // Fallback positional handling: first number => start, second => count
                if let Ok(v) = other.parse::<u128>() {
                    if start.is_none() { start = Some(v); continue; }
                }
                if let Ok(v) = other.parse::<u64>() {
                    if count.is_none() { count = Some(v); continue; }
                }
            }
        }
    }

    (start, count, resume, output, solution, progress_interval)
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let (start_arg, count_arg, resume, output, solution, progress_interval_arg) = parse_args();

    // Determine start number, possibly resuming from last written line
    // Default start is 2^68 when not resuming and not provided explicitly.
    let default_start: u128 = 1u128 << 68; // 2^68
    let start = if let Some(s) = start_arg {
        s
    } else if resume {
        match read_last_start(&output) {
            Some(last) => last.saturating_add(1),
            None => default_start,
        }
    } else {
        default_start
    };

    let count = count_arg; // None => run indefinitely
    let progress_interval = progress_interval_arg.max(1);

    let progress_path = Path::new(&output);
    let solution_path = Path::new(&solution);

    eprintln!("Starting at {}{} -> recording progress in {}", start,
        if resume { " (resume)" } else { "" }, progress_path.display());

    // Ensure the progress file exists and reflects the starting point.
    write_progress_number(progress_path, start)?;

    let mut processed: u64 = 0;
    loop {
        let current = start + processed as u128;
        let outcome = detect_outcome(current);

        // Update progress occasionally (single-line file)
        if processed % progress_interval == 0 {
            write_progress_number(progress_path, current)?;
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
                write_progress_number(progress_path, current)?;
                break;
            }
            Outcome::Overflow | Outcome::StepsOverflow => {
                let kind = if matches!(outcome, Outcome::Overflow) { "RUNAWAY_OVERFLOW_START" } else { "RUNAWAY_STEPS_OVERFLOW_START" };
                eprintln!("Detected runaway ({}). Start: {}", kind, current);
                write_solution(solution_path, &format!("{} {}", kind, current))?;
                write_progress_number(progress_path, current)?;
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

fn write_progress_number(path: &Path, value: u128) -> std::io::Result<()> {
    // Truncate and write a single line with the current start
    let mut f = OpenOptions::new().create(true).write(true).truncate(true).open(path)?;
    writeln!(f, "{}", value)?;
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

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {}", e);
        std::process::exit(1);
    }
}
