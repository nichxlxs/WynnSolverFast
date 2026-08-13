//! Optional GPU probing (cargo feature `gpu`).
//!
//! The solver never REQUIRES a GPU: this module only detects what is
//! available and reports which acceleration tier the hardware can support,
//! so the runner can pick a mode automatically:
//!
//! - **exact-f64**: discrete adapter exposing SHADER_F64 (e.g. an RTX
//!   3060 Ti via Vulkan). IEEE fp64 add/mul on GPU are bit-identical to
//!   the CPU when evaluation order is preserved and FMA contraction is
//!   disabled, so batched ceiling/gate evaluations could prune with the
//!   same admissibility contract as the CPU path.
//! - **prescreen-f32**: any adapter without fp64 (typical integrated
//!   graphics). f32 batched ceilings can still prune SOUNDLY by widening
//!   the cutoff margin to cover the worst-case f32 accumulation error
//!   (prune only when `ceiling_f32 * (1 + EPS) < cutoff`), with survivors
//!   re-checked exactly on the CPU — results stay bit-exact on ANY GPU.
//! - **cpu-only**: no adapter, or a software rasterizer.
//!
//! See GPU_PLAN.md for the staged integration design.

#[derive(Debug)]
pub struct GpuAdapterInfo {
    pub name: String,
    pub backend: String,
    pub device_type: String,
    pub discrete: bool,
    pub shader_f64: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GpuTier {
    ExactF64,
    PrescreenF32,
    CpuOnly,
}

pub struct GpuProbe {
    pub adapters: Vec<GpuAdapterInfo>,
    pub tier: GpuTier,
    /// Index into `adapters` of the chosen device, when any.
    pub chosen: Option<usize>,
}

/// Enumerate adapters and choose the best acceleration tier. Never fails:
/// a machine with no usable GPU reports `CpuOnly`.
pub fn probe() -> GpuProbe {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let mut adapters = Vec::new();
    for adapter in instance.enumerate_adapters(wgpu::Backends::all()) {
        let info = adapter.get_info();
        // Software rasterizers (llvmpipe etc.) are slower than the CPU path.
        let device_type = format!("{:?}", info.device_type);
        let discrete = matches!(info.device_type, wgpu::DeviceType::DiscreteGpu);
        let real_gpu = matches!(
            info.device_type,
            wgpu::DeviceType::DiscreteGpu | wgpu::DeviceType::IntegratedGpu
        );
        let shader_f64 = real_gpu && adapter.features().contains(wgpu::Features::SHADER_F64);
        adapters.push(GpuAdapterInfo {
            name: info.name,
            backend: format!("{:?}", info.backend),
            device_type,
            discrete,
            shader_f64,
        });
    }
    // Preference: fp64 beats fp32, discrete beats integrated; software
    // rasterizers and virtual devices never qualify.
    let rank = |a: &GpuAdapterInfo| -> i32 {
        let real = a.device_type.contains("DiscreteGpu") || a.device_type.contains("IntegratedGpu");
        if !real { return -1; }
        (if a.shader_f64 { 2 } else { 0 }) + (if a.discrete { 1 } else { 0 })
    };
    let chosen = adapters.iter().enumerate()
        .filter(|(_, a)| rank(a) >= 0)
        .max_by_key(|(_, a)| rank(a))
        .map(|(i, _)| i);
    let tier = match chosen {
        Some(i) if adapters[i].shader_f64 => GpuTier::ExactF64,
        Some(_) => GpuTier::PrescreenF32,
        None => GpuTier::CpuOnly,
    };
    GpuProbe { adapters, tier, chosen }
}
