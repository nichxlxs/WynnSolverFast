//! Native entry point for the exact same validated API used by browser quick search.
//! Usage: quick_kernel_json ENUM.txt SCORE.json OPTIONS.json
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 3 {
        eprintln!("usage: quick_kernel_json ENUM.txt SCORE.json OPTIONS.json");
        std::process::exit(2);
    }
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_else(|e| {
        eprintln!("quick_kernel_json: {p}: {e}");
        std::process::exit(2);
    });
    let result = sp_kernel::enumerate::anytime::solve_json_with_progress(
        &read(&args[0]), &read(&args[1]), &read(&args[2]), None,
    );
    println!("{result}");
    if serde_json::from_str::<serde_json::Value>(&result)
        .ok().and_then(|v| v.get("error").cloned()).is_some() {
        std::process::exit(1);
    }
}
