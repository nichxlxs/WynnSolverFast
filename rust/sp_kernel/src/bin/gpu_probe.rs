//! GPU detection report for the optional GPU runner (feature `gpu`).
//!
//! Usage: cargo run --release --features gpu --bin gpu_probe
//! Exit code: 0 = a usable GPU tier was found, 1 = CPU-only.

fn main() {
    let probe = sp_kernel::gpu::probe();
    if probe.adapters.is_empty() {
        println!("gpu_probe: no adapters found — CPU-only mode");
        std::process::exit(1);
    }
    for (i, a) in probe.adapters.iter().enumerate() {
        let marker = if probe.chosen == Some(i) { " <== selected" } else { "" };
        println!(
            "gpu_probe: [{}] {} | backend {} | {} | fp64 shaders: {}{}",
            i, a.name, a.backend, a.device_type,
            if a.shader_f64 { "yes" } else { "no" }, marker,
        );
    }
    let tier = match probe.tier {
        sp_kernel::gpu::GpuTier::ExactF64 =>
            "exact-f64 (bit-exact batched ceiling evals eligible)",
        sp_kernel::gpu::GpuTier::PrescreenF32 =>
            "prescreen-f32 (margin-widened GPU prescreen + exact CPU re-check)",
        sp_kernel::gpu::GpuTier::CpuOnly => "cpu-only",
    };
    println!("gpu_probe: tier = {}", tier);
    if probe.tier == sp_kernel::gpu::GpuTier::CpuOnly {
        std::process::exit(1);
    }
}
