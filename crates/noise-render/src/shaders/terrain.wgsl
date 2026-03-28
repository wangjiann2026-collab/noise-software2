// 3D terrain WGSL shader — stub for Phase 5.

struct CameraUniform {
    view_proj: mat4x4<f32>,
};

@group(0) @binding(0)
var<uniform> camera: CameraUniform;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal:   vec3<f32>,
    @location(2) uv:       vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0)       normal:        vec3<f32>,
    @location(1)       world_pos:     vec3<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal        = in.normal;
    out.world_pos     = in.position;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let light_dir = normalize(vec3<f32>(0.5, 1.0, 0.3));
    let diffuse = max(dot(normalize(in.normal), light_dir), 0.1);
    let color = vec3<f32>(0.55, 0.47, 0.35) * diffuse;
    return vec4<f32>(color, 1.0);
}
