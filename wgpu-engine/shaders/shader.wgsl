// ===== Data structures =====================================

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) tex_coord: vec2f,
    @location(2) normal: vec3f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coord: vec2f,
    @location(1) normal: vec3f,
    @location(2) world_pos: vec3f,
};

struct ModelUniform {
    model: mat4x4f,
    normal_matrix: mat4x4f,
};

struct CameraUniform {
    view_proj: mat4x4f,
};

struct FrameUniform {
    time: f32,
};

struct Material {
    base_color: vec4f,
    specular_color: vec4f,
    shininess: f32,
};
 
struct Light {
    position: vec3f,
    intensity: f32,
    color: vec3f,
    _pad: f32,
};

struct Lights {
    count: u32,
    lights: array<Light, 16>,
};

// ===== Uniform bindings ====================================

// Frame group: camera and frame uniforms
@group(0) @binding(0)
var<uniform> camera: CameraUniform;
@group(0) @binding(1)
var<uniform> time: f32;

// Material group: material and texture bindings
@group(1) @binding(0)
var<uniform> material: Material;
@group(1) @binding(1)
var t_diffuse: texture_2d<f32>;
@group(1) @binding(2)
var s_diffuse: sampler;

// Model group: model uniform
@group(2) @binding(0)
var<uniform> model: ModelUniform;

@group(3) @binding(0)
var<uniform> lights: Lights;

// ===== Vertex shader ==========================================

@vertex
fn vert_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = camera.view_proj * model.model * vec4(in.position, 1.0);
    out.tex_coord = in.tex_coord;
    out.normal = (model.normal_matrix * vec4(in.normal, 0.0)).xyz;
    out.world_pos = (model.model * vec4(in.position, 1.0)).xyz;

    return out;
}

// ==== Fragment shader =========================================

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {

    var light_accum: vec3f = vec3f(0.0, 0.0, 0.0);

    for (var i = 0u; i < lights.count; i = i + 1u) {
        let light = lights.lights[i];
        let light_dir = normalize(light.position - in.world_pos);
        let normal = normalize(in.normal);
        let diffuse = max(dot(normal, light_dir), 0.0);
        light_accum = light_accum + diffuse * light.color * light.intensity;
    }

    let tex_color = textureSample(t_diffuse, s_diffuse, in.tex_coord);
    let color = material.base_color * tex_color * vec4f(light_accum, 1.0);
    return color;
}