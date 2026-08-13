//! GPU vs CPU capability benchmark for the batch-shaped ceiling work
//! (feature `gpu`).
//!
//! Answers the go/no-go question for a GPU offload: on THIS machine, how
//! much faster is the GPU at the arithmetic shape our ceiling/bound
//! evaluations use, and what end-to-end speedup would that actually buy?
//!
//! The projection needs the offloadable share of runtime, which
//! SCORE_TRACE reports (`bound evals ... in Xs` plus the gate phase) and
//! bench.py prints as "gpu-offloadable". Pass it with --share.
//!
//! Usage:
//!   cargo run --release --features gpu --bin gpu_bench -- [--share 0.29]
//!
//! Both sides run the SAME dependent multiply-add chain, so the ratio is a
//! fair proxy for the real kernel; absolute numbers are synthetic.

use std::time::Instant;

const LANES: u32 = 6; // damage elements
const STEPS: u32 = 64; // boost/multiplier applications per lane
const INPUTS_PER_EVAL: usize = 16;

fn cpu_eval(inputs: &[f64], mults: &[f64], adds: &[f64]) -> f64 {
    let mut acc = 0.0f64;
    for lane in 0..LANES as usize {
        let mut v = inputs[lane % INPUTS_PER_EVAL];
        for s in 0..STEPS as usize {
            v = v * mults[s] + adds[s];
        }
        acc += v;
    }
    acc
}

fn cpu_bench(n: usize, threads: usize, mults: &[f64], adds: &[f64]) -> f64 {
    let per = n / threads.max(1);
    let t0 = Instant::now();
    std::thread::scope(|scope| {
        for t in 0..threads {
            scope.spawn(move || {
                let mut inputs = [0.0f64; INPUTS_PER_EVAL];
                for (i, v) in inputs.iter_mut().enumerate() { *v = 1.0 + i as f64 * 0.01; }
                let mut sink = 0.0f64;
                for i in 0..per {
                    inputs[i % INPUTS_PER_EVAL] += 1e-12;
                    sink += cpu_eval(&inputs, mults, adds);
                }
                std::hint::black_box(sink);
                let _ = t;
            });
        }
    });
    let secs = t0.elapsed().as_secs_f64();
    (per * threads) as f64 / secs
}

fn shader_src(ty: &str, lit: &str) -> String {
    format!(r#"
@group(0) @binding(0) var<storage, read> params: array<{ty}>;
@group(0) @binding(1) var<storage, read_write> out: array<{ty}>;

@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) gid: vec3<u32>) {{
    let i = gid.x;
    if (i >= arrayLength(&out)) {{ return; }}
    var acc: {ty} = {lit};
    let base = i % 16u;
    for (var lane: u32 = 0u; lane < {lanes}u; lane = lane + 1u) {{
        var v: {ty} = params[(base + lane) % 16u];
        for (var s: u32 = 0u; s < {steps}u; s = s + 1u) {{
            v = v * params[16u + s % 16u] + params[32u + s % 16u];
        }}
        acc = acc + v;
    }}
    out[i] = acc;
}}
"#, ty = ty, lit = lit, lanes = LANES, steps = STEPS)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let share: f64 = args.windows(2)
        .find(|w| w[0] == "--share")
        .and_then(|w| w[1].parse().ok())
        .unwrap_or(0.29);

    let probe = sp_kernel::gpu::probe();
    println!("gpu_bench: tier = {:?}", probe.tier);
    let Some(idx) = probe.chosen else {
        println!("gpu_bench: no usable GPU — nothing to compare (CPU-only machine)");
        std::process::exit(1);
    };
    let info = &probe.adapters[idx];
    println!("gpu_bench: device = {} ({}, fp64 shaders: {})",
             info.name, info.backend, if info.shader_f64 { "yes" } else { "no" });

    // Constants shared by both sides.
    let mults: Vec<f64> = (0..STEPS as usize).map(|i| 1.0 + (i as f64 % 7.0) * 1e-3).collect();
    let adds: Vec<f64> = (0..STEPS as usize).map(|i| (i as f64 % 5.0) * 1e-2).collect();

    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("\ngpu_bench: CPU baseline ({} cores)", cores);
    let cpu1 = cpu_bench(200_000, 1, &mults, &adds);
    let cpun = cpu_bench(200_000 * cores, cores, &mults, &adds);
    println!("  1 thread : {:>12.0} evals/s", cpu1);
    println!("  {} threads: {:>12.0} evals/s", cores, cpun);

    // ── GPU ──
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(), ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        compatible_surface: None,
        force_fallback_adapter: false,
    }));
    let Some(adapter) = adapter else {
        println!("gpu_bench: adapter request failed — CPU-only");
        std::process::exit(1);
    };
    let want_f64 = adapter.features().contains(wgpu::Features::SHADER_F64);
    let features = if want_f64 { wgpu::Features::SHADER_F64 } else { wgpu::Features::empty() };
    let dev = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("bench"), required_features: features,
        required_limits: wgpu::Limits::default(),
        memory_hints: Default::default(),
    }, None));
    let (device, queue) = match dev {
        Ok(d) => d,
        Err(e) => { println!("gpu_bench: device request failed: {e} — CPU-only"); std::process::exit(1); }
    };

    for (ty, lit, precision) in [("f64", "0.0lf", "fp64 (bit-exact eligible)"),
                                 ("f32", "0.0", "fp32 (prescreen tier)")] {
        if ty == "f64" && !want_f64 {
            println!("\ngpu_bench: fp64 shaders unsupported on this device — skipping exact tier");
            continue;
        }
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(ty),
            source: wgpu::ShaderSource::Wgsl(shader_src(ty, lit).into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(ty), layout: None, module: &module,
            entry_point: "main", compilation_options: Default::default(),
            cache: None,
        });
        if let Some(err) = pollster::block_on(device.pop_error_scope()) {
            println!("\ngpu_bench: {ty} shader/pipeline rejected: {err}");
            continue;
        }

        println!("\ngpu_bench: GPU {precision}");
        for &n in &[16_384usize, 262_144, 1_048_576] {
            let elem = if ty == "f64" { 8 } else { 4 };
            let params_bytes: Vec<u8> = if ty == "f64" {
                (0..48).flat_map(|i| (1.0f64 + i as f64 * 1e-3).to_le_bytes()).collect()
            } else {
                (0..48).flat_map(|i| (1.0f32 + i as f32 * 1e-3).to_le_bytes()).collect()
            };
            let params = device.create_buffer(&wgpu::BufferDescriptor {
                label: None, size: params_bytes.len() as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&params, 0, &params_bytes);
            let out_size = (n * elem) as u64;
            let out = device.create_buffer(&wgpu::BufferDescriptor {
                label: None, size: out_size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let staging = device.create_buffer(&wgpu::BufferDescriptor {
                label: None, size: out_size,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: None, layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry { binding: 0, resource: params.as_entire_binding() },
                    wgpu::BindGroupEntry { binding: 1, resource: out.as_entire_binding() },
                ],
            });

            // Warm-up, then timed batches including readback (the real cost).
            let mut run = || {
                let mut enc = device.create_command_encoder(&Default::default());
                {
                    let mut pass = enc.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&pipeline);
                    pass.set_bind_group(0, &bind, &[]);
                    pass.dispatch_workgroups((n as u32 + 63) / 64, 1, 1);
                }
                enc.copy_buffer_to_buffer(&out, 0, &staging, 0, out_size);
                queue.submit(Some(enc.finish()));
                let slice = staging.slice(..);
                let (tx, rx) = std::sync::mpsc::channel();
                slice.map_async(wgpu::MapMode::Read, move |r| { let _ = tx.send(r); });
                device.poll(wgpu::Maintain::Wait);
                let _ = rx.recv();
                staging.unmap();
            };
            run();
            let iters = 5;
            let t0 = Instant::now();
            for _ in 0..iters { run(); }
            let secs = t0.elapsed().as_secs_f64();
            let rate = (n * iters) as f64 / secs;
            let vs_cpu = rate / cpun;
            println!("  batch {:>9} : {:>12.0} evals/s  ({:.1}x all-core CPU)", n, rate, vs_cpu);
            if n == 1_048_576 {
                let end_to_end = 1.0 / ((1.0 - share) + share / vs_cpu.max(1e-9));
                println!("  → projected end-to-end at {:.0}% offloadable work: {:.2}x",
                         share * 100.0, end_to_end);
                println!("    (ceiling if the GPU were infinitely fast: {:.2}x)",
                         1.0 / (1.0 - share));
            }
        }
    }
}
