// ======================================================================================
// Irradiance bake: convolves the environment cube into a diffuse irradiance cube.
// Run once per face at load. For each output direction N (this fragment's world dir),
// integrate incoming radiance over the hemisphere around N, cosine-weighted. The result
// is the diffuse ambient a surface with normal N receives. Very low frequency, so the
// target cube is tiny (32x32). No PBR here, just the convolution.
// ======================================================================================

struct FaceUniform {
    inv_view_proj: mat4x4f,
};

@group(0) @binding(0) var<uniform> face: FaceUniform;
@group(1) @binding(0) var t_env: texture_cube<f32>;
@group(1) @binding(1) var s_env: sampler;

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
    out.ndc = vec2f(c.x, -c.y);    // IMPORTANT: flip Y to match NDC convention (same as equirect bake)
    return out;
}

const PI = 3.14159265359;
const MAX_RADIANCE = 100.0; // clamp to avoid fireflies from HDR spikes

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {
    // Eye is at the origin for the bake, so the unprojected far-plane point IS the
    // output direction N we are computing irradiance for.
    let far = face.inv_view_proj * vec4f(in.ndc, 1.0, 1.0);
    let N = normalize(far.xyz / far.w);

    // Build a tangent basis around N so we can walk the hemisphere.
    var up = vec3f(0.0, 1.0, 0.0);
    let right = normalize(cross(up, N));
    up = normalize(cross(N, right));

    // Riemann-sum the cosine-weighted hemisphere integral.
    var irradiance = vec3f(0.0);
    let sample_delta = 0.025;
    var samples = 0.0;
    for (var phi = 0.0; phi < 2.0 * PI; phi += sample_delta) {
        for (var theta = 0.0; theta < 0.5 * PI; theta += sample_delta) {
            // spherical (tangent space) -> world
            let tangent = vec3f(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
            let world = tangent.x * right + tangent.y * up + tangent.z * N;
            // textureSampleLevel (explicit LOD): sampling in a loop needs uniform LOD or
            // WGSL rejects it. cos(theta) = Lambert weight, sin(theta) = solid-angle scale.
            let radiance = min(textureSampleLevel(t_env, s_env, world, 0.0).rgb, vec3f(MAX_RADIANCE));
            irradiance += radiance * cos(theta) * sin(theta);
            samples += 1.0;
        }
    }
    irradiance = PI * irradiance / samples;

    return vec4f(irradiance, 1.0);
}