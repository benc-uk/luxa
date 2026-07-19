// ===== Data structures =====================================

const AMBIENT_COLOR: vec3f = vec3f(0.05);
const EXPOSURE: f32 = 1.5;

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
// Material values
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
    let albedo_rgba = get_albedo(in.tex_coord);
    let albedo = albedo_rgba.rgb;
    let mr = get_metallic_roughness(in.tex_coord);
    let metallic = mr.x;
    let roughness = mr.y;

    let N = normalize(in.normal);
    let V = normalize(camera.pos - in.world_pos);
    let NdotV = max(dot(N, V), 1e-4);
    let F0 = mix(vec3f(0.04), albedo, metallic);

    var light_accum: vec3f = vec3f(0.0, 0.0, 0.0);

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

    let ao = get_occlusion(in.tex_coord);
    let ambient = AMBIENT_COLOR * albedo * ao;

    var hdr = ambient + light_accum;
    hdr += get_emissive(in.tex_coord);
    let tone_mapped = tonemap_aces(hdr * EXPOSURE);
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