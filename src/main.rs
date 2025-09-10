use std::env;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH, Duration, Instant};
use std::collections::VecDeque;
use std::sync::mpsc::{self, SyncSender, Receiver};
use std::thread;

use num_bigint::BigUint;
use num_traits::One;
use num_integer::Integer;
use minifb::{Window, WindowOptions, Key};

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

// Messages from compute thread to visualization thread
enum VizMsg {
    Draw(BigUint),
    Stats { processed: u64, sps: f64 },
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
    for l in reader.lines().map_while(Result::ok) {
        let t = l.trim();
        if t.is_empty() { continue; }
        if let Ok(v) = t.parse::<BigUint>() { last = Some(v); }
    }
    last
}

#[allow(clippy::type_complexity)]
fn parse_args() -> (Option<BigUint>, Option<u64>, bool, String, String, u64, bool, bool, u64, u64) {
    let mut start: Option<BigUint> = None;
    let mut count: Option<u64> = None;
    let mut resume = true;
    let mut output = String::from("progress.txt");
    let mut solution = String::from("solution.txt");
    let mut progress_interval: u64 = 1000;
    let mut random = false; // default OFF
    let mut viz = true;    // default ON
    let mut viz_interval: u64 = 1_000; // draw often by default
    let mut viz_max_steps: u64 = 10_000; // limit steps when rendering

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
            "--no-random" => {
                random = false;
            }
            "--viz" => {
                viz = true;
            }
            "--no-viz" => {
                viz = false;
            }
            "--viz-interval" => {
                if let Some(v) = args.next() { if let Ok(n) = v.parse::<u64>() { viz_interval = n; } }
            }
            "--viz-max-steps" => {
                if let Some(v) = args.next() { if let Ok(n) = v.parse::<u64>() { viz_max_steps = n.max(100); } }
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

    (start, count, resume, output, solution, progress_interval, random, viz, viz_interval, viz_max_steps)
}

fn real_main() -> Result<(), Box<dyn std::error::Error>> {
    let (start_arg, count_arg, resume, output, solution, progress_interval_arg, random, viz, viz_interval_arg, viz_max_steps) = parse_args();

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
    let viz_interval = viz_interval_arg.max(1);

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

    // Ensure the progress file exists and reflects the starting point (sequential mode only).
    if !random {
        write_progress_number(progress_path, &start)?;
    }

    let mut processed: u64 = 0;

    // Minimal PRNG (xorshift128+)
    let mut rng = Rng::seeded();

    // Random range [2^68, 2^2000 - 1]
    let rand_low: BigUint = BigUint::one() << 68;
    let rand_high_inclusive: BigUint = (BigUint::one() << 2000) - BigUint::one();

    // Optional visualization thread/channel
    let viz_sender: Option<SyncSender<VizMsg>> = if viz {
        let (tx, rx) = mpsc::sync_channel::<VizMsg>(4);
        thread::spawn(move || run_viz(rx, viz_max_steps as usize));
        Some(tx)
    } else { None };

    let mut last_stat = Instant::now();
    let mut last_count: u64 = 0;

    loop {
        let current: BigUint = if random {
            rng.gen_range_biguint(&rand_low, &rand_high_inclusive)
        } else {
            &start + &BigUint::from(processed)
        };
        let outcome = detect_outcome(&current);

        // Update progress occasionally (single-line file), only in sequential mode
        if !random && processed % progress_interval == 0 {
            write_progress_number(progress_path, &current)?;
        }

        // Send trajectory data at configured cadence
        if let Some(ref tx) = viz_sender {
            if processed % viz_interval == 0 {
                let _ = tx.try_send(VizMsg::Draw(current.clone()));
            }
        }

        if processed % 10000 == 0 {
            eprintln!("Processed {processed} starts (up to {current})");
        }

        match outcome {
            Outcome::ReachesOne => {
                // Keep scanning
            }
            Outcome::NontrivialCycle => {
                eprintln!("Found nontrivial loop starting from {current}.");
                write_solution(solution_path, &format!("NONTRIVIAL_CYCLE_START {current}"))?;
                // Also update progress to this current number (sequential mode only)
                if !random { write_progress_number(progress_path, &current)?; }
                break;
            }
            Outcome::StepsOverflow => {
                let kind = "RUNAWAY_STEPS_OVERFLOW_START";
                eprintln!("Detected runaway ({kind}). Start: {current}");
                write_solution(solution_path, &format!("{kind} {current}"))?;
                if !random { write_progress_number(progress_path, &current)?; }
                break;
            }
        }

        processed = processed.saturating_add(1);
        // Send stats periodically (~500ms)
        if let Some(ref tx) = viz_sender {
            let now = Instant::now();
            let elapsed = now.duration_since(last_stat);
            if elapsed >= Duration::from_millis(500) {
                let delta = processed.saturating_sub(last_count) as f64;
                let secs = elapsed.as_secs_f64().max(1e-9);
                let sps = delta / secs;
                let _ = tx.try_send(VizMsg::Stats { processed, sps });
                last_stat = now;
                last_count = processed;
            }
        }
        if let Some(limit) = count {
            if processed >= limit { 
                eprintln!("Finished processing {processed} numbers. Keeping visualization open...");
                // Keep sending the last computed trajectory to keep viz alive
                if let Some(ref tx) = viz_sender {
                    let _ = tx.try_send(VizMsg::Draw(current.clone()));
                }
                break; 
            }
        }
    }
    
    // If visualization is enabled, wait for user to close the window
    if viz_sender.is_some() {
        eprintln!("Computation complete. Close the visualization window or press Ctrl+C to exit.");
        // Keep the main thread alive so the visualization thread continues running
        loop {
            thread::sleep(Duration::from_millis(1000));
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
    writeln!(f, "{line}")?;
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
        let nanos: u128 = now.as_nanos();
        // Split into two 64-bit seeds and scramble
        let s0 = (nanos as u64).wrapping_mul(0x9E3779B97F4A7C15);
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

    // removed unused next_u128()

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
                let chunk = r.to_be_bytes();
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

// ---------- Visualization (minifb) ----------
const VIZ_W: usize = 500;
const VIZ_H: usize = 500;

fn run_viz(rx: Receiver<VizMsg>, max_steps: usize) {
    let mut window = match Window::new(
        "Collatz Visualizer",
        VIZ_W,
        VIZ_H,
        WindowOptions {
            resize: false,
            scale: minifb::Scale::X1,
            ..WindowOptions::default()
        },
    ) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("viz error: {e}");
            return;
        }
    };

    let mut buffer = vec![0u32; VIZ_W * VIZ_H];
    // Streaming animation state
    let mut current_n: Option<BigUint> = None;
    let mut bits_window: VecDeque<usize> = VecDeque::with_capacity(max_steps.max(1));
    let max_points = max_steps.max(1);
    let steps_per_tick: usize = (max_points / 60).clamp(1, 2000);
    let one = BigUint::one();
    // Local RNG for fallback samples to keep animation moving
    let mut vrng = Rng::seeded();
    let rand_low: BigUint = BigUint::one() << 68;
    let rand_high_inclusive: BigUint = (BigUint::one() << 2000) - BigUint::one();
    
    // Initial clear
    clear_buffer(&mut buffer, 0xFFFFFFFF);
    draw_grid(&mut buffer, 50, 0xFFE0E0E0);
    draw_axes(&mut buffer, 10, 0xFF000000);
    window.set_title("Collatz Visualizer - waiting for samples...");
    let _ = window.update_with_buffer(&buffer, VIZ_W, VIZ_H);

    // no need to track last_draw now that we redraw only on new data

    while window.is_open() && !window.is_key_down(Key::Escape) {
        let mut should_redraw = false;
        
        // Check for new messages
        let mut had_new_draw = false;
        while let Ok(msg) = rx.try_recv() {
            match msg {
                VizMsg::Draw(start) => {
                    // Begin animating this trajectory from scratch
                    current_n = Some(start);
                    bits_window.clear();
                    should_redraw = true;
                    had_new_draw = true;
                }
                VizMsg::Stats { processed, sps } => {
                    window.set_title(&format!("Collatz Visualizer  |  processed={processed}  |  {sps:.1} samples/s"));
                }
            }
        }

        // Incrementally extend trajectory for animation
        if let Some(ref mut n) = current_n {
            for _ in 0..steps_per_tick {
                // Record current magnitude
                bits_window.push_back(bit_len_biguint(n).max(1));
                if bits_window.len() > max_points { bits_window.pop_front(); }
                // Advance
                if *n == one { break; }
                *n = collatz_next(n);
            }
            // If we reached 1 and didn't receive a new start, pick a fallback sample
            if *n == one && !had_new_draw {
                *n = vrng.gen_range_biguint(&rand_low, &rand_high_inclusive);
                bits_window.clear();
            }
            should_redraw = true;
        }

        // Only redraw when we have new data
        if should_redraw && bits_window.len() >= 2 {
            clear_buffer(&mut buffer, 0xFFFFFFFF);
            draw_grid(&mut buffer, 50, 0xFFE0E0E0);
            draw_axes(&mut buffer, 10, 0xFF000000);
            
            // Draw the visible window
            let pad = 10usize;
            let w = VIZ_W - 2*pad;
            let h = VIZ_H - 2*pad;
            let len = bits_window.len();
            let max_bits = *bits_window.iter().max().unwrap_or(&1);
            let mut prev = point_xy(0, bits_window[0], len, max_bits, w, h, pad);
            for (i, bits) in bits_window.iter().enumerate().skip(1) {
                let curr = point_xy(i, *bits, len, max_bits, w, h, pad);
                draw_line(prev.0 as i32, prev.1 as i32, curr.0 as i32, curr.1 as i32, 0xFF000000, &mut buffer);
                prev = curr;
            }
            
            let _ = window.update_with_buffer(&buffer, VIZ_W, VIZ_H);
        } else {
            window.update();
        }
        
        thread::sleep(Duration::from_millis(10));
    }
}

fn clear_buffer(buf: &mut [u32], color: u32) {
    for px in buf.iter_mut() { *px = color; }
}

// streaming visualization no longer uses a precomputed trajectory function

fn point_xy(i: usize, bits: usize, len: usize, max_bits: usize, w: usize, h: usize, pad: usize) -> (usize, usize) {
    let x = pad + (i.saturating_mul(w.saturating_sub(1))) / (len.saturating_sub(1).max(1));
    // y: top is 0; map higher bits to lower y (higher on screen)
    let y = pad + (h.saturating_sub(1)).saturating_sub((bits.saturating_mul(h.saturating_sub(1))) / max_bits.max(1));
    (x.min(VIZ_W.saturating_sub(1)), y.min(VIZ_H.saturating_sub(1)))
}

fn draw_line(x0: i32, y0: i32, x1: i32, y1: i32, color: u32, buffer: &mut [u32]) {
    let mut x0 = x0; let mut y0 = y0;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;
    loop {
        plot(x0, y0, color, buffer);
        if x0 == x1 && y0 == y1 { break; }
        let e2 = 2 * err;
        if e2 >= dy { err += dy; x0 += sx; }
        if e2 <= dx { err += dx; y0 += sy; }
    }
}

fn plot(x: i32, y: i32, color: u32, buffer: &mut [u32]) {
    if x < 0 || y < 0 { return; }
    let x = x as usize; let y = y as usize;
    if x >= VIZ_W || y >= VIZ_H { return; }
    buffer[y * VIZ_W + x] = color;
}

fn bit_len_biguint(n: &BigUint) -> usize {
    let bytes = n.to_bytes_be();
    if bytes.is_empty() { return 0; }
    let leading = bytes[0].leading_zeros() as usize;
    let bit_len = bytes.len().saturating_mul(8).saturating_sub(leading);
    bit_len.min(5000)
}

fn draw_grid(buf: &mut [u32], spacing: usize, color: u32) {
    for x in (0..VIZ_W).step_by(spacing.max(1)) {
        for y in 0..VIZ_H { buf[y * VIZ_W + x] = color; }
    }
    for y in (0..VIZ_H).step_by(spacing.max(1)) {
        for x in 0..VIZ_W { buf[y * VIZ_W + x] = color; }
    }
}

fn draw_axes(buf: &mut [u32], pad: usize, color: u32) {
    let x0 = pad; let x1 = VIZ_W - pad;
    let y0 = pad; let y1 = VIZ_H - pad;
    for x in x0..=x1 { buf[y1 * VIZ_W + x] = color; }
    for y in y0..=y1 { buf[y * VIZ_W + x0] = color; }
}

fn main() {
    if let Err(e) = real_main() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
