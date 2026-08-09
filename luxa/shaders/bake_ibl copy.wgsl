// ======================================================================================
// IBL bake passes. One standard WGSL module holds the shared fullscreen, cubemap and GGX
// helpers, with a separate fragment entry point for each baked resource.
// ======================================================================================

const PI = 3.14159265359;
const INV_ATAN = vec2f(0.1591, 0.3183); // (1/2PI, 1/PI)

struct FaceUniform {
    inv_view_proj: mat4x4f,
    roughness: f32,
};

struct VertexOutput {
    @builtin(position) clip: vec4f,
    @location(0) ndc: vec2f,
    @location(1) uv: vec2f,
};

// ===== Uniform bindings ====================================

@group(0) @binding(0) var<uniform> face: FaceUniform;

// Cubemap source used by the irradiance, environment-mip and prefilter passes.
@group(1) @binding(0) var t_env: texture_cube<f32>;
@group(1) @binding(1) var s_env: sampler;

// 2D source used only by the equirectangular-to-cubemap pass. It lives in a separate
// group because WGSL resource bindings have a fixed texture dimension.
@group(2) @binding(0) var t_equirect: texture_2d<f32>;
@group(2) @binding(1) var s_equirect: sampler;

// ===== Entry points for various bake passes ====================================

@vertex
fn vert_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var corners = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    let corner = corners[vertex_index];

    var out: VertexOutput;
    out.clip = vec4f(corner, 1.0, 1.0);
    out.ndc = vec2f(corner.x, -corner.y);
    out.uv = vec2f((corner.x + 1.0) * 0.5, (1.0 - corner.y) * 0.5);
    return out;
}

@fragment
fn frag_equirect(in: VertexOutput) -> @location(0) vec4f {
    let direction = face_direction(in.ndc);
    var uv = vec2f(atan2(direction.z, direction.x), asin(direction.y)) * INV_ATAN + 0.5;
    uv.y = 1.0 - uv.y;
    return vec4f(textureSampleLevel(t_equirect, s_equirect, uv, 0.0).rgb, 1.0);
}

@fragment
fn frag_irradiance(in: VertexOutput) -> @location(0) vec4f {
    let normal = face_direction(in.ndc);

    var up = vec3f(0.0, 1.0, 0.0);
    let right = normalize(cross(up, normal));
    up = normalize(cross(normal, right));

    var irradiance = vec3f(0.0);
    let sample_delta = 0.025;
    var samples = 0.0;
    for (var phi = 0.0; phi < 2.0 * PI; phi += sample_delta) {
        for (var theta = 0.0; theta < 0.5 * PI; theta += sample_delta) {
            let tangent = vec3f(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
            let world = tangent.x * right + tangent.y * up + tangent.z * normal;
            let radiance = textureSampleLevel(t_env, s_env, world, 0.0).rgb;
            irradiance += radiance * cos(theta) * sin(theta);
            samples += 1.0;
        }
    }
    irradiance = PI * irradiance / samples;
    return vec4f(irradiance, 1.0);
}

@fragment
fn frag_brdf_lut(in: VertexOutput) -> @location(0) vec2f {
    let n_dot_v = max(in.uv.x, 1e-4);
    return integrate_brdf(n_dot_v, in.uv.y);
}

@fragment
fn frag_env_mips(in: VertexOutput) -> @location(0) vec4f {
    let direction = face_direction(in.ndc);
    return vec4f(textureSampleLevel(t_env, s_env, direction, 0.0).rgb, 1.0);
}

@fragment
fn frag_prefilter(in: VertexOutput) -> @location(0) vec4f {
    let normal = face_direction(in.ndc);
    let view = normal;

    let SAMPLE_COUNT = 1024u;
    let env_size = f32(textureDimensions(t_env, 0).x);
    let alpha = face.roughness * face.roughness;

    var prefiltered = vec3f(0.0);
    var total_weight = 0.0;
    for (var index = 0u; index < SAMPLE_COUNT; index++) {
        let xi = hammersley(index, SAMPLE_COUNT);
        let half_vector = importance_sample_ggx(xi, normal, face.roughness);
        let light = normalize(2.0 * dot(view, half_vector) * half_vector - view);

        let n_dot_l = dot(normal, light);
        if n_dot_l > 0.0 {
            let n_dot_h = max(dot(normal, half_vector), 0.0);
            let pdf = distribution_ggx(n_dot_h, alpha) / 4.0;
            var lod = compute_lod(pdf, env_size, f32(SAMPLE_COUNT));
            if face.roughness == 0.0 {
                lod = 0.0;
            }

            let radiance = textureSampleLevel(t_env, s_env, light, lod).rgb;
            prefiltered += radiance * n_dot_l;
            total_weight += n_dot_l;
        }
    }
    prefiltered /= max(total_weight, 1e-4);
    return vec4f(prefiltered, 1.0);
}

// ===== Helper & shared functions ===================================

// Unproject a far-plane point back to world space, then form the view ray.
fn face_direction(ndc: vec2f) -> vec3f {
    let far = face.inv_view_proj * vec4f(ndc, 1.0, 1.0);
    return normalize(far.xyz / far.w);
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

// Compute a 2D Hammersley point in [0,1]^2 for the given sample index and total sample count.
fn hammersley(index: u32, count: u32) -> vec2f {
    return vec2f(f32(index) / f32(count), radical_inverse_vdc(index));
}

// Importance sample a GGX half-vector for the given 2D Hammersley point, normal and roughness.
fn importance_sample_ggx(xi: vec2f, normal: vec3f, roughness: f32) -> vec3f {
    let alpha = roughness * roughness;
    let phi = 2.0 * PI * xi.x;
    let cos_theta = sqrt((1.0 - xi.y) / (1.0 + (alpha * alpha - 1.0) * xi.y));
    let sin_theta = sqrt(1.0 - cos_theta * cos_theta);
    let half_vector = vec3f(cos(phi) * sin_theta, sin(phi) * sin_theta, cos_theta);

    var up = vec3f(0.0, 0.0, 1.0);
    if abs(normal.z) >= 0.999 {
        up = vec3f(1.0, 0.0, 0.0);
    }
    let tangent = normalize(cross(up, normal));
    let bitangent = cross(normal, tangent);
    return normalize(tangent * half_vector.x + bitangent * half_vector.y + normal * half_vector.z);
}

// GGX geometry term for IBL. This is a simplified version of the geometry_smith() function
fn geometry_schlick_ggx_ibl(n_dot_x: f32, roughness: f32) -> f32 {
    let k = (roughness * roughness) / 2.0;
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}

// Smith geometry term for IBL. This is a simplified version of the geometry_smith() function
fn geometry_smith_ibl(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx_ibl(n_dot_v, roughness) * geometry_schlick_ggx_ibl(n_dot_l, roughness);
}

// Integrate the GGX BRDF over the hemisphere for the given NdotV and roughness, returning a scale and bias factor.
fn integrate_brdf(n_dot_v: f32, roughness: f32) -> vec2f {
    let view = vec3f(sqrt(1.0 - n_dot_v * n_dot_v), 0.0, n_dot_v);
    let normal = vec3f(0.0, 0.0, 1.0);

    var scale = 0.0;
    var bias = 0.0;
    let SAMPLE_COUNT = 1024u;
    for (var index = 0u; index < SAMPLE_COUNT; index++) {
        let xi = hammersley(index, SAMPLE_COUNT);
        let half_vector = importance_sample_ggx(xi, normal, roughness);
        let light = normalize(2.0 * dot(view, half_vector) * half_vector - view);

        let n_dot_l = max(light.z, 0.0);
        let n_dot_h = max(half_vector.z, 0.0);
        let v_dot_h = max(dot(view, half_vector), 0.0);

        if n_dot_l > 0.0 {
            let geometry = geometry_smith_ibl(n_dot_v, n_dot_l, roughness);
            let geometry_visibility = (geometry * v_dot_h) / (n_dot_h * n_dot_v);
            let fresnel = pow(1.0 - v_dot_h, 5.0);
            scale += (1.0 - fresnel) * geometry_visibility;
            bias += fresnel * geometry_visibility;
        }
    }
    return vec2f(scale, bias) / f32(SAMPLE_COUNT);
}

// GGX normal distribution. `alpha` is perceptual roughness squared.
fn distribution_ggx(n_dot_h: f32, alpha: f32) -> f32 {
    let a = n_dot_h * alpha;
    let k = alpha / (1.0 - n_dot_h * n_dot_h + a * a);
    return k * k / PI;
}

// Select an environment source mip matching the solid angle represented by one sample.
fn compute_lod(pdf: f32, env_size: f32, sample_count: f32) -> f32 {
    return 0.5 * log2(6.0 * env_size * env_size / (sample_count * max(pdf, 1e-6)));
}