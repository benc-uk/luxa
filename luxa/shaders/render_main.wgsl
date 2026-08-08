// ======================================================================================
// Main core rendering shader for Luxa. 
// This is a PBR shader that supports glTF 2.0 materials, IBL, and multiple lights.
// ======================================================================================

const EXPOSURE: f32 = 1.0;
const PI: f32 = 3.14159265359;

struct VertexInput {
    @location(0) position: vec3f,
    @location(1) tex_coord: vec2f,
    @location(2) normal: vec3f,
    @location(3) tangent: vec4f,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4f,
    @location(0) tex_coord: vec2f,
    @location(1) normal: vec3f,
    @location(2) world_pos: vec3f,
    @location(3) tangent: vec4f,
};

struct ModelUniform {
    model: mat4x4f,
    normal_matrix: mat4x4f,
};

struct CameraUniform {
    view_proj: mat4x4f,
    inv_view_proj: mat4x4f,
    pos: vec3f,
};

struct FrameUniform {
    time: f32,
};

struct Material {
    base_color_factor: vec4f,
    emissive_factor: vec3f,
    metallic_factor: f32,
    roughness_factor: f32,
    normal_scale: f32,
    occlusion_strength: f32,
    alpha_cutoff: f32,
    alpha_mode: u32, // 0 = opaque, 1 = mask, 2 = blend
};
 
struct Light {
    position: vec3f,
    intensity: f32,
    color: vec3f,
    _pad: f32,
};

struct Lights {
    ambient_color: vec3f,
    ambient_intensity: f32,
    count: u32,
    ibl_enabled: u32,
    _padding: vec2u,
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

// Base colour, sRGB
@group(1) @binding(1)
var t_base_color: texture_2d<f32>;
@group(1) @binding(2)
var s_base_color: sampler;

// Metallic-roughness, linear
@group(1) @binding(3)
var t_metallic_roughness: texture_2d<f32>;
@group(1) @binding(4)
var s_metallic_roughness: sampler;

// Normal, linear
@group(1) @binding(5)
var t_normal: texture_2d<f32>;
@group(1) @binding(6)
var s_normal: sampler;

// Occlusion, linear
@group(1) @binding(7)
var t_occlusion: texture_2d<f32>;
@group(1) @binding(8)
var s_occlusion: sampler;

// Emissive, sRGB
@group(1) @binding(9)
var t_emissive: texture_2d<f32>;
@group(1) @binding(10)
var s_emissive: sampler;

// Model group: model uniform
@group(2) @binding(0)
var<uniform> model: ModelUniform;

@group(3) @binding(0)
var<uniform> lights: Lights;

// IBL group: irradiance, prefilter, and BRDF LUT
@group(3) @binding(1)
var t_irradiance: texture_cube<f32>;
@group(3) @binding(2)
var s_irradiance: sampler;
@group(3) @binding(3)
var t_prefilter: texture_cube<f32>;
@group(3) @binding(4)
var s_prefilter: sampler;
@group(3) @binding(5)
var t_brdf_lut: texture_2d<f32>;
@group(3) @binding(6)
var s_brdf_lut: sampler;

// ===== Vertex shader ==========================================

@vertex
fn vert_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;

    out.clip_position = camera.view_proj * model.model * vec4(in.position, 1.0); // Clip space position based on model and camera matrices.
    out.tex_coord = in.tex_coord;                                   // Simple pass-through of texture coordinates.
    out.normal = (model.normal_matrix * vec4f(in.normal, 0.0)).xyz; // Transform normal to world space using the inverse transpose of the model matrix (normal matrix).
    out.world_pos = (model.model * vec4f(in.position, 1.0)).xyz;    // Transform position to world space using the model matrix.
    let t = (model.normal_matrix * vec4f(in.tangent.xyz, 0.0)).xyz; // Transform tangent to world space using the normal matrix.
    out.tangent = vec4f(t, in.tangent.w);                           // Preserve the handedness of the tangent vector (w component) while transforming the xyz components.

    return out;
}

// ==== Fragment shader =========================================

@fragment
fn frag_main(in: VertexOutput) -> @location(0) vec4f {
    // Start by sampling the material albedo texture kinda the base color
    let albedo_rgba = get_albedo(in.tex_coord);
    let albedo = albedo_rgba.rgb;

    // Handle alpha modes mask, discard fragments below the alpha cutoff threshold.
    if material.alpha_mode == 1u && albedo_rgba.a < material.alpha_cutoff {
        discard;
    }

    // PBR starts here: get metallic and roughness values from the packed texture.
    let mr = get_metallic_roughness(in.tex_coord);
    let metallic = mr.x;
    let roughness = mr.y;

    // Some vectors needed for all lighting calcs
    let N = get_normal(in);
    let V = normalize(camera.pos - in.world_pos);
    let NdotV = max(dot(N, V), 1e-4);
    let F0 = mix(vec3f(0.04), albedo, metallic);

    var light_accum: vec3f = vec3f(0.0, 0.0, 0.0);

    // Handle classic point lights in the scene
    for (var i = 0u; i < lights.count; i = i + 1u) {
        let light = lights.lights[i];
        let L = normalize(light.position - in.world_pos);   // point light
        let H = normalize(V + L);
        let NdotL = max(dot(N, L), 0.0);

        if NdotL <= 0.0 { continue; }

        // Attenuation (point light, inverse-square). Directional: attenuation = 1.
        let dist = length(light.position - in.world_pos);
        let attenuation = 1.0 / (dist * dist);
        let radiance = light.color * light.intensity * attenuation;

        let NdotH = max(dot(N, H), 0.0);

        let D = distribution_ggx(NdotH, roughness);
        let G = geometry_smith(NdotV, NdotL, roughness);
        let F = fresnel_schlick(max(dot(H, V), 0.0), F0);

        let spec = (D * G * F) / (4.0 * NdotV * NdotL + 1e-4);
        let kd = (vec3f(1.0) - F) * (1.0 - metallic);
        let lo = (kd * albedo / PI + spec) * radiance * NdotL;
        light_accum = light_accum + lo;
    }

    // Ambient occlusion ---
    let ao = get_occlusion(in.tex_coord);

    var ambient = albedo
        * lights.ambient_color
        * lights.ambient_intensity
        * ao;

    if lights.ibl_enabled != 0u {
        // --- Split-sum IBL ambient (replaces flat AMBIENT_COLOR) ---
        let F_amb = fresnel_schlick_roughness(NdotV, F0, roughness);
        let kd = (vec3f(1.0) - F_amb) * (1.0 - metallic);

        // Diffuse: irradiance in the surface-normal direction, tinted by albedo.
        let irradiance = textureSample(t_irradiance, s_irradiance, N).rgb;
        let diffuse = irradiance * albedo;

        // Specular: prefiltered radiance along the reflection vector, at a mip chosen by
        // roughness, combined with the pre-integrated BRDF (scale, bias) from the LUT.
        let R = reflect(-V, N);
        let max_prefilter_mip = f32(textureNumLevels(t_prefilter) - 1u);
        let prefiltered = textureSampleLevel(t_prefilter, s_prefilter, R, roughness * max_prefilter_mip).rgb;
        let brdf = textureSample(t_brdf_lut, s_brdf_lut, vec2f(NdotV, roughness)).rg;
        let specular = prefiltered * (F0 * brdf.x + brdf.y);
        ambient = (kd * diffuse + specular) * ao;
    }

    var light_final = ambient + light_accum;
    light_final = light_final + get_emissive(in.tex_coord);

    let tone_mapped = tonemap_aces(light_final * EXPOSURE);

    return vec4f(tone_mapped, albedo_rgba.a);
}

// ===== Material texture sampling ==============================

// Returns the base colour with the base colour factor applied. The base colour texture is sRGB, so the sample is already linear here.
fn get_albedo(uv: vec2f) -> vec4f {
    // base colour texture is sRGB, so the sample is already linear here
    return material.base_color_factor * textureSample(t_base_color, s_base_color, uv);
}

// Returns (metallic, roughness) with the glTF channel packing applied.
fn get_metallic_roughness(uv: vec2f) -> vec2f {
    let mr = textureSample(t_metallic_roughness, s_metallic_roughness, uv);
    let metallic = mr.b * material.metallic_factor;
    let roughness = max(mr.g * material.roughness_factor, 0.045); // floor to kill sparkle
    return vec2f(metallic, roughness);
}

fn get_occlusion(uv: vec2f) -> f32 {
    let ao = textureSample(t_occlusion, s_occlusion, uv).r;
    // strength 0 = no occlusion, 1 = full. Lerp from white so partial strength fades it out.
    return mix(1.0, ao, material.occlusion_strength);
}

fn get_emissive(uv: vec2f) -> vec3f {
    // emissive texture is sRGB, so the sample is already linear here
    return material.emissive_factor * textureSample(t_emissive, s_emissive, uv).rgb;
}

// ===== Other helpers =========================================

fn get_normal(in: VertexOutput) -> vec3f {
    let N = normalize(in.normal);

    // Sample first
    var n = textureSample(t_normal, s_normal, in.tex_coord).xyz * 2.0 - 1.0;

    // Degenerate or missing tangent: fall back to the geometric normal.
    if dot(in.tangent.xyz, in.tangent.xyz) < 1e-8 { return N; }

    // Gram-Schmidt: re-orthogonalise T against N (interpolation skews it slightly).
    let T = normalize(in.tangent.xyz - dot(in.tangent.xyz, N) * N);
    // Bitangent from the cross product; tangent.w carries glTF handedness (+/-1).
    let B = cross(N, T) * in.tangent.w;

    let tbn = mat3x3f(T, B, N);

    // normal_scale only affects the tangent-space XY, never Z.
    n = vec3f(n.xy * material.normal_scale, n.z);

    return normalize(tbn * n);
}

// ===== PBR math helpers =========================================

// GGX / Trowbridge-Reitz normal distribution.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a = roughness * roughness;      // perceptual -> linear roughness
    let a2 = a * a;
    let d = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// Smith geometry term using Schlick-GGX, direct-lighting k remap.
fn geometry_schlick_ggx(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;               // direct lighting variant
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness) * geometry_schlick_ggx(n_dot_l, roughness);
}

// Fresnel-Schlick.
fn fresnel_schlick(cos_theta: f32, f0: vec3f) -> vec3f {
    return f0 + (vec3f(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
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

fn fresnel_schlick_roughness(cos_theta: f32, f0: vec3f, roughness: f32) -> vec3f {
    let f90 = max(vec3f(1.0 - roughness), f0);
    return f0 + (f90 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}