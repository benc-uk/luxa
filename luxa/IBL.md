# IBL Implementation Plan

Image-based lighting for the engine. This is **Phase 5** of the wider PBR plan
(see `PBR.md`); it is split into its own document because it is large and only
worth starting once the analytic lighting from Phases 1 to 4 is solid.

IBL replaces the flat `AMBIENT_COLOR` term with light gathered from an environment
map, so surfaces pick up colour and specular reflections from their surroundings.
This is the phase that makes metals look like metal.

## The approach: split-sum

We use the **split-sum approximation** (Karis / UE4), the same one every real-time
PBR engine uses. The full environment lighting integral is too expensive per
fragment, so it is pre-baked once at load into three resources:

- **Irradiance cubemap** - diffuse ambient. A heavily blurred, cosine-convolved
  version of the environment. Sampled by surface normal `N`.
- **Prefiltered environment cubemap** - specular ambient. The environment convolved
  with the GGX lobe at increasing roughness, one roughness level per mip. Sampled by
  reflection vector `R` at a mip chosen from roughness.
- **BRDF integration LUT** - a 2D lookup (`NdotV` x roughness) holding the scale and
  bias to apply to `F0`. View-independent, so it can be baked once and reused for any
  environment.

At runtime the ambient becomes:

```
F        = fresnel_schlick_roughness(NdotV, F0, roughness)
kd       = (1 - F) * (1 - metallic)
diffuse  = irradiance(N) * albedo
brdf     = textureSample(brdf_lut, vec2(NdotV, roughness)).rg
specular = prefiltered(R, roughness) * (F0 * brdf.x + brdf.y)
ambient  = (kd * diffuse + specular) * ao
```

All three bakes are one-off render passes done when an environment is loaded, not per
frame. Group them behind an engine call like `engine.set_environment(hdr_bytes)` so
consumers never see the cubemap machinery. A good home is a new `src/engine/ibl.rs`.

## Data plumbing shared by every sub-phase

- **Cubemap support in `src/models/texture.rs`.** A cubemap is a texture with
  `depth_or_array_layers = 6`. You need two kinds of view: a `TextureViewDimension::Cube`
  view for _sampling_, and six single-layer 2D views (`base_array_layer = i`,
  `array_layer_count = 1`) as _render targets_, one per face.
- **Pixel format.** Use `Rgba16Float` for every cubemap and the HDR source. On
  WebGPU `Rgba16Float` is filterable by default; `Rgba32Float` is **not** filterable
  without an optional feature, and we target wasm, so avoid it. Half float has ample
  range for environment radiance.
- **Face orientation.** Baking each face is a render pass with a 90 degree FOV
  perspective and a view matrix looking down +X, -X, +Y, -Y, +Z, -Z. Keep the six
  view matrices in one table and reuse them across 5a to 5c. Watch the +Y / -Y up
  vectors, this is the usual place cubemaps come out flipped.
- **New bind group.** Add IBL as `@group(4)`: irradiance cube + sampler, prefiltered
  cube + sampler, BRDF LUT + sampler. Groups 0 to 3 (frame, material, model, lights)
  stay as they are. Bind it once per frame alongside the frame group.

## 5a. Equirectangular HDR to cubemap

- Load an `.hdr` equirectangular with the `image` crate (it decodes Radiance HDR to
  `Rgb32F`); upload it as a 2D `Rgba16Float` texture.
- Render into the six faces of a colour cubemap (say 512x512) by drawing a unit cube
  and, per fragment, mapping the world-space direction to equirectangular UVs:

```wgsl
const INV_ATAN = vec2f(0.1591, 0.3183); // (1/2PI, 1/PI)

fn sample_spherical_map(dir: vec3f) -> vec2f {
    let v = normalize(dir);
    var uv = vec2f(atan2(v.z, v.x), asin(v.y));
    uv *= INV_ATAN;
    uv += 0.5;
    return uv;
}
```

The vertex shader for each face transforms the cube positions by that face's
`proj * view`; the fragment interpolates the local position as the sample direction.

**Test:** sample the resulting cubemap directly as the background (skybox) behind the
scene. It should look like the original HDR with no seams and the horizon level.

## 5b. Diffuse irradiance cubemap

- Convolve the environment cubemap into a small irradiance cubemap (32x32 is plenty,
  it is very low frequency). For each output direction `N`, integrate incoming
  radiance over the hemisphere, cosine weighted, in a fragment pass per face:

```wgsl
// N is the interpolated cube direction for this fragment.
var irradiance = vec3f(0.0);
var up    = vec3f(0.0, 1.0, 0.0);
let right = normalize(cross(up, N));
up        = normalize(cross(N, right));

let sample_delta = 0.025;
var samples = 0.0;
for (var phi = 0.0; phi < 2.0 * PI; phi += sample_delta) {
    for (var theta = 0.0; theta < 0.5 * PI; theta += sample_delta) {
        // spherical -> tangent -> world
        let tangent = vec3f(sin(theta) * cos(phi), sin(theta) * sin(phi), cos(theta));
        let world   = tangent.x * right + tangent.y * up + tangent.z * N;
        irradiance += textureSample(env_cube, env_sampler, world).rgb
                    * cos(theta) * sin(theta);
        samples += 1.0;
    }
}
irradiance = PI * irradiance / samples;
```

**Test:** sample the irradiance map by `N` as the _only_ ambient term (drop the
prefilter/LUT for now). A rough dielectric should take on the overall colour of the
environment, brighter where the environment is bright.

## 5c. Prefiltered specular cubemap

- Create a cubemap with a mip chain (128x128 base, ~5 mips). Each mip corresponds to
  a roughness in `[0, 1]` (`roughness = mip / (mip_count - 1)`). For each face and mip,
  importance-sample the GGX lobe around `R = V = N` (the split-sum assumption) and
  average, weighting by `NdotL`:

```wgsl
let roughness = push.roughness; // per-mip, passed in
let N = normalize(local_pos);
let R = N;
let V = R;

var prefiltered = vec3f(0.0);
var total_weight = 0.0;
let SAMPLE_COUNT = 1024u;
for (var i = 0u; i < SAMPLE_COUNT; i++) {
    let xi = hammersley(i, SAMPLE_COUNT);
    let H  = importance_sample_ggx(xi, N, roughness);
    let L  = normalize(2.0 * dot(V, H) * H - V);
    let n_dot_l = max(dot(N, L), 0.0);
    if (n_dot_l > 0.0) {
        prefiltered  += textureSampleLevel(env_cube, env_sampler, L, 0.0).rgb * n_dot_l;
        total_weight += n_dot_l;
    }
}
prefiltered /= total_weight;
```

`hammersley` (low-discrepancy sequence) and `importance_sample_ggx` are the standard
helpers; put them in `pbr.wgsl` since 5c and 5d both use them. Sample the source
environment with an explicit mip (`textureSampleLevel(..., 0.0)`) to avoid the
uniformity issue you already hit with `textureSample` under loops.

**Test:** sample the prefiltered map at a fixed mip as a debug output. Low mips (mirror)
should show a sharp reflection of the environment; high mips a smeared blur.

## 5d. BRDF integration LUT

- A one-off 2D `Rg16Float` texture (512x512), `x = NdotV`, `y = roughness`. Full-screen
  pass, no environment input. Each texel integrates the GGX BRDF to a scale (`.r`) and
  bias (`.g`) for `F0`:

```wgsl
// integrate_brdf(n_dot_v, roughness) -> vec2f  (scale, bias)
// same Hammersley + GGX importance sampling as 5c, with the
// geometry_smith IBL k = a^2 / 2 remap (NOT the direct-light (r+1)^2/8).
```

Note the **k remap** here differs from Phase 1: IBL uses `k = roughness^2 / 2`, direct
lighting uses `(roughness + 1)^2 / 8`. Keep two geometry helpers, or parameterise `k`.

**Test:** render the LUT to screen. It is a recognisable image: bright in the lower
left, a smooth green/red gradient. Compare against the reference LUT from any PBR
tutorial.

## 5e. Wire the split-sum term into the shader

- Add the Fresnel-with-roughness variant (the ambient term needs roughness folded into
  `F` so smooth surfaces still get an edge highlight):

```wgsl
fn fresnel_schlick_roughness(cos_theta: f32, f0: vec3f, roughness: f32) -> vec3f {
    let f90 = max(vec3f(1.0 - roughness), f0);
    return f0 + (f90 - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}
```

- In `frag_main`, replace `let ambient = AMBIENT_COLOR * albedo * ao;` with the
  split-sum block from the top of this section. Reflection vector `R = reflect(-V, N)`;
  choose the prefilter mip with `roughness * (PREFILTER_MIP_COUNT - 1)` via
  `textureSampleLevel`.
- Keep `AMBIENT_COLOR` as a fallback for when no environment is set, so models still
  render before `set_environment` is called.

**Test:** the Phase 1 metallic/roughness sphere grid, now lit only by IBL (kill the
analytic lights). Smooth metals should mirror the environment; rough metals show a
blurred tint; dielectrics pick up soft diffuse colour. Orbiting moves the reflections
correctly. Then re-enable the analytic lights, the two should sum cleanly.

## Suggested order within Phase 5

**5a -> 5b -> 5d -> 5c -> 5e.** The LUT (5d) is self-contained and easy to verify, so
bake it before the fiddlier prefilter. 5e lights up only once all three inputs exist.
