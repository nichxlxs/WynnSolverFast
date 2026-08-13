//! CLI front-end for the enumeration kernel. The engine itself lives in
//! `sp_kernel::enumerate` so it can also be driven from WASM.
fn main() {
    sp_kernel::enumerate::cli_main();
}
