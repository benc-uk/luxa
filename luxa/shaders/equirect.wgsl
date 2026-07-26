// ======================================================================================
// Equirect bake: projects an equirectangular HDR into one cubemap face.
// Run once per face at load. A fullscreen triangle covers the face; each fragment
// unprojects its NDC through that face's inverse view-proj to a world direction, then
// maps the direction to equirectangular UVs. No lighting, no PBR.
// ======================================================================================

struct FaceUniform {
    inv_view_proj: mat4x4f,
};

@group(0) @binding(0) var<uniform> face: FaceUniform;
@group(1) @binding(0) var t_src: texture_2d<f32>;
@group(1) @binding(1) var s_src: sampler;

struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) ndc: vec2f,
};

@vertex
fn vert_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    // One oversized triangle covering the whole face.
    var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    let c = corners[vid];
    var out: VertexOutput;
    out.clip = vec4f(c, 1.0, 1.0); // z = w -> far plane (DX depth = 1.0)
    out.ndc = vec2f(c.x, -c.y);    // IMPORTANT: flip Y to match NDC convention
    return out;
}

const INV_ATAN = vec2f(0.1591, 0.3183); // (1/2PI, 1/PI)

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {
    // Eye is at the origin for the bake, so the unprojected far-plane point IS the direction.
    let far = face.inv_view_proj * vec4f(in.ndc, 1.0, 1.0);
    let dir = normalize(far.xyz / far.w);

    var uv = vec2f(atan2(dir.z, dir.x), asin(dir.y)) * INV_ATAN + 0.5;
    uv.y = 1.0 - uv.y; // image row 0 is the top; flip to match

    return vec4f(textureSampleLevel(t_src, s_src, uv, 0.0).rgb, 1.0);
}
