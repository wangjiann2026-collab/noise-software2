// 3D terrain WGSL shader.
//
// Diffuse + ambient lighting with a directional light.
// The terrain is coloured by elevation. The noise heatmap is overlaid
// via a second draw call (heatmap.wgsl drawn as a transparent layer).

// ─── Uniforms ────────────────────────────────────────────────────────────────

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

// ─── Vertex stage ─────────────────────────────────────────────────────────────

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       world_pos:     vec3<f32>,
    @location(1)       normal:        vec3<f32>,
    @location(2)       elevation:     f32,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.world_pos     = in.position;
    out.normal        = normalize(in.normal);
    out.elevation     = in.position.z; // Z = height above datum (m)
    return out;
}

// ─── Elevation colour ─────────────────────────────────────────────────────────

fn elevation_to_color(z: f32) -> vec3<f32> {
    // Dark brown (z=0) → light beige (z=50 m).
    let t = clamp(z / 50.0, 0.0, 1.0);
    return mix(vec3<f32>(0.29, 0.21, 0.13), vec3<f32>(0.78, 0.72, 0.60), t);
}

// ─── Fragment stage ───────────────────────────────────────────────────────────

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir  = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let ambient    = 0.15;
    let n          = normalize(in.normal);
    let diffuse    = max(dot(n, light_dir), 0.0);
    let base       = elevation_to_color(in.elevation);
    let lit        = base * (diffuse + ambient);
    return vec4<f32>(clamp(lit, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
