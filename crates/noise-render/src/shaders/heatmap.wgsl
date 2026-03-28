// Noise heatmap WGSL shader — stub for Phase 5.
// Maps noise level (dBA) to color using a custom color scale.

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) noise_level: f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) noise_level: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.noise_level = in.noise_level;
    return out;
}

// Color scale: blue (35 dBA) → green (50 dBA) → yellow (60 dBA) → red (75+ dBA)
fn noise_to_color(level: f32) -> vec4<f32> {
    let t = clamp((level - 35.0) / 40.0, 0.0, 1.0);
    let r = smoothstep(0.5, 1.0, t);
    let g = 1.0 - abs(2.0 * t - 1.0);
    let b = smoothstep(0.5, 0.0, t);
    return vec4<f32>(r, g, b, 0.8);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    return noise_to_color(in.noise_level);
}
