// ======================================================================================
// BRDF integration LUT bake: pre-integrates the GGX BRDF into a 2D lookup.
// x = NdotV, y = roughness. Output .rg = (scale, bias) applied to F0 at runtime:
//   specular = prefiltered * (F0 * scale + bias)
// View- and environment-independent: baked once, reused everywhere. Pure maths, no
// inputs. (Split-sum, 5d.)
// ======================================================================================

const PI = 3.14159265359;

struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) uv: vec2f,
};

@vertex
fn vert_main(@builtin(vertex_index) vid: u32) -> VertexOutput {
    var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    let c = corners[vid];
    var out: VertexOutput;
    out.clip = vec4f(c, 0.0, 1.0);
    // uv in [0,1], Y flipped so texture row 0 (top) = roughness 0, matching how the LUT
    // is sampled at runtime with vec2(NdotV, roughness). This flip IS a correctness thing.
    out.uv = vec2f((c.x + 1.0) * 0.5, (1.0 - c.y) * 0.5);
    return out;
}

// Van der Corput radical inverse (bit-reversal of i, scaled to [0,1)).
fn radical_inverse_vdc(bits_in: u32) -> f32 {
    var bits = bits_in;
    bits = (bits << 16u) | (bits >> 16u);
    bits = ((bits & 0x55555555u) << 1u) | ((bits & 0xAAAAAAAAu) >> 1u);
    bits = ((bits & 0x33333333u) << 2u) | ((bits & 0xCCCCCCCCu) >> 2u);
    bits = ((bits & 0x0F0F0F0Fu) << 4u) | ((bits & 0xF0F0F0F0u) >> 4u);
    bits = ((bits & 0x00FF00FFu) << 8u) | ((bits & 0xFF00FF00u) >> 8u);
    return f32(bits) * 2.3283064365386963e-10; // 1 / 2^32
}

// Hammersley low-discrepancy point set.
fn hammersley(i: u32, n: u32) -> vec2f {
    return vec2f(f32(i) / f32(n), radical_inverse_vdc(i));
}

// Importance-sample the GGX half-vector around N.
fn importance_sample_ggx(xi: vec2f, n: vec3f, roughness: f32) -> vec3f {
    let a = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (a * a - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);

    let h = vec3f(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta); // tangent space

    var up = vec3f(0.0, 0.0, 1.0);
    if abs(n.z) >= 0.999 { up = vec3f(1.0, 0.0, 0.0); }
    let tangent = normalize(cross(up, n));
    let bitangent = cross(n, tangent);
    return normalize(tangent * h.x + bitangent * h.y + n * h.z);
}

// Smith geometry, IBL k remap = a^2/2. DELIBERATELY different from pbr.wgsl's direct
// (r+1)^2/8 - the split-sum LUT needs this variant.
fn geometry_schlick_ggx_ibl(n_dot_x: f32, roughness: f32) -> f32 {
    let k = (roughness * roughness) / 2.0;
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}
fn geometry_smith_ibl(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx_ibl(n_dot_v, roughness) * geometry_schlick_ggx_ibl(n_dot_l, roughness);
}

fn integrate_brdf(n_dot_v: f32, roughness: f32) -> vec2f {
    // View vector in a tangent frame where N = +Z.
    let v = vec3f(sqrt(1.0 - n_dot_v * n_dot_v), 0.0, n_dot_v);
    let n = vec3f(0.0, 0.0, 1.0);

    var a = 0.0; // scale
    var b = 0.0; // bias
    let SAMPLE_COUNT = 1024u;
    for (var i = 0u; i < SAMPLE_COUNT; i++) {
        let xi = hammersley(i, SAMPLE_COUNT);
        let h = importance_sample_ggx(xi, n, roughness);
        let l = normalize(2.0 * dot(v, h) * h - v);

        let n_dot_l = max(l.z, 0.0);
        let n_dot_h = max(h.z, 0.0);
        let v_dot_h = max(dot(v, h), 0.0);

        if n_dot_l > 0.0 {
            let g = geometry_smith_ibl(n_dot_v, n_dot_l, roughness);
            let g_vis = (g * v_dot_h) / (n_dot_h * n_dot_v);
            let fc = pow(1.0 - v_dot_h, 5.0);
            a += (1.0 - fc) * g_vis;
            b += fc * g_vis;
        }
    }
    return vec2f(a, b) / f32(SAMPLE_COUNT);
}

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec2f {
    let n_dot_v = max(in.uv.x, 1e-4); // at exactly 0 the view reconstruction degenerates
    let roughness = in.uv.y;
    return integrate_brdf(n_dot_v, roughness);
}