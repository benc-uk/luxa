// ======================================================================================
// Skybox: draws the environment cubemap behind the scene. No lighting, no PBR.
// ======================================================================================
const EXPOSURE: f32 = 1.0;

struct CameraUniform {
    view_proj: mat4x4f,
    inv_view_proj: mat4x4f,
    pos: vec3f,
};

@group(0) @binding(0) var<uniform> camera: CameraUniform;
@group(1) @binding(0) var t_env: texture_cube<f32>;
@group(1) @binding(1) var s_env: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) ndc: vec2f,
};

@vertex
fn vert_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    // One oversized triangle covering the whole screen.
    var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    let c = corners[vid];
    var out: VertexOutput;
    out.clip = vec4f(c, 1.0, 1.0); // z = w -> far plane (DX depth = 1.0)
    out.ndc = c;
    return out;
}

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {
    // Unproject a far-plane point back to world space, then form the view ray.
    let far = camera.inv_view_proj * vec4f(in.ndc, 1.0, 1.0);
    let world = far.xyz / far.w;
    let dir = normalize(world - camera.pos);

    // Pin to mip 0: the env cube carries a mip chain
    let env_sample = textureSampleLevel(t_env, s_env, dir, 0.0);

    let tone_mapped = tonemap_aces(env_sample.rgb * EXPOSURE);
    return vec4f(tone_mapped, 1.0);
}

// ACES filmic approximation (Narkowicz). Input & output linear.
fn tonemap_aces(x: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3f(0.0), vec3f(1.0));
}
