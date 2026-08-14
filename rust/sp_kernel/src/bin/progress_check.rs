//! Exercises the live-progress sink natively, so the browser path's counters
//! can be checked without a browser.
//!
//! Usage: progress_check <enum fixture> <score fixture>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let enum_fx = std::fs::read_to_string(&a[1]).expect("enum fixture");
    let score_fx = if a.len() > 2 {
        std::fs::read_to_string(&a[2]).expect("score fixture")
    } else { String::new() };

    let mut n = 0usize;
    let mut last = String::new();
    let mut sink = |p: sp_kernel::enumerate::ProgressSnapshot| {
        n += 1;
        let j = sp_kernel::enumerate::progress_json(&p);
        if n <= 3 { println!("progress[{n}]: {}", &j[..j.len().min(220)]); }
        last = j;
        None
    };
    let out = sp_kernel::enumerate::solve_json_with_progress(
        &enum_fx, &score_fx, 0.0, Some(&mut sink));
    println!("emissions: {n}");
    println!("final progress: {}", &last[..last.len().min(220)]);
    println!("result:   {}", &out[..out.len().min(220)]);
}
