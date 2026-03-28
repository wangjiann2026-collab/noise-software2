// Noise heatmap WGSL shader.
//
// Vertex shader receives per-vertex world XY position and noise level (dBA).
// Fragment shader maps noise level to colour using the WHO standard scale,
// and optionally draws iso-contour lines.

// ─── Uniforms ────────────────────────────────────────────────────────────────

struct HeatmapUniform {
    min_db: f32,
    max_db: f32,
    alpha:  f32,
    _pad:   f32,
};

@group(0) @binding(0)
var<uniform> hm: HeatmapUniform;

// ─── Vertex stage ─────────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position:    vec2<f32>,
    @location(1) noise_level: f32,
    @location(2) _pad:        f32,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) noise_level: f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    // Position already in clip space [-1,+1] for the orthographic 2D view.
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.noise_level   = in.noise_level;
    return out;
}

// ─── Colour scale (WHO / EEA) ─────────────────────────────────────────────────

// Piecewise linear colour scale with 8 stops (35–75 dBA).
fn noise_to_color(level: f32, min_db: f32, max_db: f32, alpha: f32) -> vec4<f32> {
    // Clamp to [0, 1] across the displayed range.
    let t = clamp((level - min_db) / max(max_db - min_db, 1.0), 0.0, 1.0);

    // 5-stop piecewise linear: blue→cyan→green→yellow→orange→red.
    var r: f32;
    var g: f32;
    var b: f32;

    if t < 0.2 {
        let s = t / 0.2;
        r = 0.0;
        g = mix(0.45, 0.72, s);
        b = mix(0.21, 0.34, s);
    } else if t < 0.4 {
        let s = (t - 0.2) / 0.2;
        r = mix(0.0,  0.66, s);
        g = mix(0.72, 0.88, s);
        b = mix(0.34, 0.22, s);
    } else if t < 0.6 {
        let s = (t - 0.4) / 0.2;
        r = mix(0.66, 1.0,  s);
        g = mix(0.88, 0.94, s);
        b = mix(0.22, 0.0,  s);
    } else if t < 0.8 {
        let s = (t - 0.6) / 0.2;
        r = mix(1.0,  1.0, s);
        g = mix(0.94, 0.39, s);
        b = 0.0;
    } else {
        let s = (t - 0.8) / 0.2;
        r = mix(1.0,  0.47, s);
        g = mix(0.39, 0.0,  s);
        b = 0.0;
    }

    return vec4<f32>(r, g, b, alpha);
}

// ─── Fragment stage ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // Discard no-data cells (represented as very large negative values).
    if in.noise_level < -900.0 {
        discard;
    }
    return noise_to_color(in.noise_level, hm.min_db, hm.max_db, hm.alpha);
}
