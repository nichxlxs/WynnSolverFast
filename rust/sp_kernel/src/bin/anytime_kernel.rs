//! Bounded native heuristic search. See enumerate::anytime for guarantees.
fn main() {
    if let Err(e) = sp_kernel::enumerate::anytime::cli_main() {
        eprintln!("anytime_kernel: {e}");
        std::process::exit(2);
    }
}
