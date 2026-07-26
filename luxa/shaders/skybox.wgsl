// ======================================================================================
// Skybox: draws the environment cubemap behind the scene. No lighting, no PBR.
// ======================================================================================

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
    return vec4f(textureSample(t_env, s_env, dir).rgb, 1.0);
}
