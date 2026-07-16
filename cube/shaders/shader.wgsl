// ===== Data structures =====================================

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) tex_coord: vec2f,
    @location(2) color: vec3f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coord: vec2f,
    @location(1) color: vec3f,
};

struct CameraUniform {
    view_proj: mat4x4f,
};

// ===== Vertex shader ==========================================

@group(1) @binding(0)
var<uniform> camera: CameraUniform;

@vertex
fn vert_main(model: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = camera.view_proj * vec4(model.position, 1.0);
    out.tex_coord = model.tex_coord;
    out.color = model.color;

    return out;
}

// ==== Fragment shader =========================================

@group(0) @binding(0)
var t_diffuse: texture_2d<f32>;
@group(0) @binding(1)
var s_diffuse: sampler;

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {
    return textureSample(t_diffuse, s_diffuse, in.tex_coord) * vec4(in.color, 1.0);
}