/// Print system and platform information.
pub fn run() -> anyhow::Result<()> {
    println!("Noise Platform — System Information");
    println!("  Version     : {}", env!("CARGO_PKG_VERSION"));
    println!("  Platform    : {}", std::env::consts::OS);
    println!("  Arch        : {}", std::env::consts::ARCH);
    println!("  CPU threads : {}", num_cpus_count());
    println!("  Rust edition: 2021");
    println!();
    println!("  Acoustic Engine");
    println!("    Max reflection order : {}", noise_core::engine::ray_tracer::MAX_REFLECTION_ORDER);
    println!("    Propagation models   : ISO 9613-2, CNOSSOS-EU");
    println!();
    println!("  Graphics");
    println!("    Rendering API        : wgpu (Vulkan / Metal / DX12 / WebGPU)");
    println!("    Shader language      : WGSL");
    Ok(())
}

fn num_cpus_count() -> usize {
    // Use rayon's detected thread count as a proxy.
    rayon::current_num_threads()
}
