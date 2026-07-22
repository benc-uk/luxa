# PBR Implementation Plan

A phased plan to bring physically based rendering (metallic-roughness workflow, glTF
style) to the engine. Ordered so each phase is independently testable and always
leaves something rendering. No throwaway work: each phase builds on the last.

## Current state

The bindings are ~90% in place already:

- `Material` struct, uniform, and all five glTF textures (base colour,
  metallic-roughness, normal, occlusion, emissive) are wired up in
  `src/models/material.rs`.
- The light array is bound (`@group(3)`).
- The fragment shader in `shaders/shader.wgsl` currently does only simple Lambert
  diffuse and samples base colour only.

The gaps are almost entirely in the fragment shader, plus two data-plumbing items
(camera position, vertex tangents).

## Phase 0 - Groundwork (data plumbing)

### 0a. Add camera world position to the camera uniform

- Extend `CameraUniform` in `src/engine.rs`: add `camera_pos: [f32; 4]` (vec3 + pad,
  kept 16-byte aligned).
- Fill it in the render pass (`src/engine/render.rs`) from the active camera node's
  world translation.
- Update the WGSL `CameraUniform` struct in `shaders/shader.wgsl` to match.
- **Test:** output `camera_pos` as a debug colour, confirm it changes as you orbit.

### 0b. Confirm light data is usable

- Check the `Light` struct carries enough: position, colour, intensity. Decide now
  whether to add a `light_type` (0 = point, 1 = directional) and a `range`. Even a
  single hard-coded directional "sun" is fine to start.
- **Test:** existing Lambert loop still runs.

## Phase 1 - Core BRDF with vertex normals (the big visual win)

Rewrite the fragment shader in `shaders/shader.wgsl` to a proper Cook-Torrance
metallic-roughness model, using the interpolated vertex normal only (no normal maps
yet). This is the phase that turns flat Lambert shading into recognisable PBR.

### 1.1 The model

We evaluate the reflectance equation as a sum over the analytic lights. For each
light the outgoing radiance is:

```
Lo = (kd * albedo / PI + spec) * radiance * NdotL
```

where `spec` is the Cook-Torrance specular BRDF:

```
spec = (D * G * F) / (4 * NdotV * NdotL + eps)
```

- `D` - normal distribution function (GGX / Trowbridge-Reitz): how many microfacets
  are aligned to the half vector `H`. Dominates the highlight shape.
- `G` - geometry / masking-shadowing (Smith with Schlick-GGX): microfacets occluding
  each other at grazing angles.
- `F` - Fresnel (Schlick): how reflectivity rises toward grazing angles.

Energy conservation ties diffuse and specular together: `F` is also the specular
reflectance weight, so the diffuse weight is `kd = (1 - F) * (1 - metallic)`. Metals
have no diffuse term, hence the `(1 - metallic)` factor.

### 1.2 Inputs to gather per fragment

1. **Albedo:** `base_color_factor * textureSample(base_color).rgb`. The texture is
   sRGB so sampling already linearises it. Keep alpha separately for Phase 3.
2. **Metallic / roughness:** `textureSample(metallic_roughness)` with glTF packing:
   `.b` = metallic, `.g` = roughness. Multiply by `metallic_factor` /
   `roughness_factor`. Clamp roughness to a small floor (e.g. `max(rough, 0.045)`) to
   avoid a zero-area highlight that sparkles.
3. **N, V:** `N = normalize(in.normal)` (vertex normal for now), `V = normalize(
camera_pos - in.world_pos)` (needs Phase 0a). Precompute `NdotV = max(dot(N, V),
1e-4)`.
4. **F0:** `mix(vec3(0.04), albedo, metallic)`. 0.04 is the standard dielectric base
   reflectance; metals take their F0 from the albedo.

### 1.3 WGSL helper functions

Add these as free functions in the shader. All operate on scalars/`vec3f`:

```wgsl
const PI: f32 = 3.14159265359;

// GGX / Trowbridge-Reitz normal distribution.
fn distribution_ggx(n_dot_h: f32, roughness: f32) -> f32 {
    let a  = roughness * roughness;      // perceptual -> linear roughness
    let a2 = a * a;
    let d  = n_dot_h * n_dot_h * (a2 - 1.0) + 1.0;
    return a2 / (PI * d * d);
}

// Smith geometry term using Schlick-GGX, direct-lighting k remap.
fn geometry_schlick_ggx(n_dot_x: f32, roughness: f32) -> f32 {
    let r = roughness + 1.0;
    let k = (r * r) / 8.0;               // direct lighting variant
    return n_dot_x / (n_dot_x * (1.0 - k) + k);
}

fn geometry_smith(n_dot_v: f32, n_dot_l: f32, roughness: f32) -> f32 {
    return geometry_schlick_ggx(n_dot_v, roughness)
         * geometry_schlick_ggx(n_dot_l, roughness);
}

// Fresnel-Schlick.
fn fresnel_schlick(cos_theta: f32, f0: vec3f) -> vec3f {
    return f0 + (vec3f(1.0) - f0) * pow(clamp(1.0 - cos_theta, 0.0, 1.0), 5.0);
}
```

Note the two roughness conventions: `D` and `G` use the squared "linear" roughness
`a = roughness^2` internally, and `G`'s `k` uses the direct-lighting remap
`(roughness + 1)^2 / 8`. Keep these separate from the IBL variant (`k = a^2 / 2`)
you will add in Phase 5.

### 1.4 Per-light loop

For each light `i` in `0..lights.count`:

```wgsl
let L = normalize(light.position - in.world_pos);   // point light
let H = normalize(V + L);
let n_dot_l = max(dot(N, L), 0.0);

// Skip fully back-facing fragments early.
if (n_dot_l <= 0.0) { continue; }

// Attenuation (point light, inverse-square). Directional: attenuation = 1.
let dist        = length(light.position - in.world_pos);
let attenuation = 1.0 / (dist * dist);
let radiance    = light.color * light.intensity * attenuation;

let n_dot_h = max(dot(N, H), 0.0);
let n_dot_v = max(dot(N, V), 1e-4);

let D = distribution_ggx(n_dot_h, roughness);
let G = geometry_smith(n_dot_v, n_dot_l, roughness);
let F = fresnel_schlick(max(dot(H, V), 0.0), f0);

let spec = (D * G * F) / (4.0 * n_dot_v * n_dot_l + 1e-4);

let kd = (vec3f(1.0) - F) * (1.0 - metallic);
Lo += (kd * albedo / PI + spec) * radiance * n_dot_l;
```

### 1.5 Ambient and final colour

With no IBL yet, add a flat ambient so shadowed areas are not pure black:

```wgsl
let ambient = ambient_color * albedo;    // e.g. ambient_color = vec3(0.03)
var color   = ambient + Lo;
```

Return `vec4f(color, base_alpha)`. Do **not** tonemap or gamma-correct here yet;
that is Phase 2. If you want to sanity check before Phase 2, a temporary
`color = color / (color + 1.0)` Reinhard keeps things in range.

### 1.6 Practical notes

- **Directional vs point:** gate `L` and `attenuation` on a light type. For a first
  pass a single hard-coded directional sun (`L = -normalize(light.direction)`,
  `attenuation = 1`) is the easiest thing to reason about.
- **Epsilons:** the `+ 1e-4` in the specular denominator and the `NdotV` floor avoid
  divide-by-zero fireflies at grazing angles.
- **Intensity units:** inverse-square falloff makes point lights dim fast; expect to
  push `intensity` well above 1 (tens to hundreds) to get a visible result.

**Test:** render a 5x5 grid of spheres, metallic 0->1 across one axis and roughness
0->1 across the other. Dielectric rough spheres should look chalky, metallic smooth
spheres should have a tight bright highlight and near-black diffuse. Orbiting the
camera should move highlights correctly (validates `V` / Phase 0a).

## Phase 2 - Tonemap and colour correctness

The per-light loop produces open-ended **HDR** radiance in **linear** space. Two
separate steps turn that into correct pixels: a **tonemap** (compress HDR into
`[0,1]`) and a **gamma/sRGB encode** (linear -> display). Keep them distinct - they
are different operations and it is easy to accidentally do the second one twice.

### Surface is sRGB (our case) - confirmed

Our surface view uses `add_srgb_suffix()` (see `src/engine/render.rs` and the pipeline
format in `src/engine.rs`), so the GPU applies the sRGB OETF automatically on write.

Consequences:

- **Do the tonemap in the shader.** Operate on the linear HDR colour, output linear
  `[0,1]`.
- **Do NOT add a manual `pow(color, 1.0/2.2)`.** The sRGB surface already encodes it;
  a manual gamma on top double-encodes and washes the image out.
- Everything upstream (albedo from sRGB textures, lighting) is already linear, so no
  extra decode is needed either.

### 2.1 Tonemap operator

Add a helper to `shaders/pbr.wgsl`. ACES (Narkowicz fit) is recommended for filmic
highlight rolloff; Reinhard is the simple fallback.

```wgsl
// ACES filmic approximation (Narkowicz). Input & output linear.
fn tonemap_aces(x: vec3f) -> vec3f {
    let a = 2.51;
    let b = 0.03;
    let c = 2.43;
    let d = 0.59;
    let e = 0.14;
    return clamp((x * (a * x + b)) / (x * (c * x + d) + e), vec3f(0.0), vec3f(1.0));
}

// Reinhard fallback.
fn tonemap_reinhard(x: vec3f) -> vec3f {
    return x / (x + vec3f(1.0));
}
```

Reinhard is cheaper but desaturates/greys bright highlights; ACES keeps saturation
and rolls off more pleasingly. Both drop into the same place.

### 2.2 Apply in the fragment shader

In `shaders/shader.wgsl` `frag_main`, keep an explicit HDR accumulator so Phase 3
emissive can be added before tonemapping later:

```wgsl
var hdr = ambient + light_accum;
// (Phase 3: hdr += emissive; goes here, before the tonemap.)
hdr *= EXPOSURE;                 // optional single exposure knob, e.g. 1.0
let mapped = tonemap_aces(hdr);
return vec4f(mapped, albedo_rgba.a);
```

### 2.3 Exposure (optional but recommended)

Add a constant `EXPOSURE` (or, later, a camera/frame uniform) multiplied into `hdr`
before the tonemap. Because point lights use inverse-square falloff, intensities are
large and vary a lot; one exposure scalar lets you rebalance the whole scene without
re-tuning every light or swapping tonemappers.

### If the surface were NOT sRGB (reference only)

Not our situation, but for completeness: with a linear/UNORM surface you would tonemap
**and then** manually encode, e.g. `pow(mapped, vec3(1.0/2.2))`, before returning.
Doing this on our sRGB surface is the classic double-gamma bug.

**Test:** bright highlights on smooth metals roll off to colour instead of clipping to
flat white; midtones look right. A/B by temporarily setting `mapped = hdr` to see the
untonemapped clipping for comparison.

## Phase 3 - Remaining material maps

1. **Occlusion:** sample `.r`, apply to the ambient term only, scaled by
   `occlusion_strength`.
2. **Emissive:** sample, multiply by `emissive_factor`, add after lighting (before
   tonemap).
3. **Alpha:** implement `Mask` mode using `alpha_cutoff` (discard); wire `Blend` later
   with pipeline blend state.

**Test:** an emissive material glows; an occlusion map darkens crevices in ambient
only.

## Phase 4 - Normal mapping (the tangent detour)

Deliberately last because it needs mesh and parser changes.

1. Add `tangent: [f32; 4]` to `Vertex` in `src/models/mesh_vert.rs` and its vertex
   attribute array.
2. Read `TANGENT` from glTF in `src/parser.rs`; **generate tangents** when absent
   (per-triangle from positions + UVs, averaged and orthonormalised).
3. Pass tangent through the vertex shader, build a TBN matrix, sample `t_normal`,
   apply `normal_scale`.
4. Update primitive builders (`src/models/builder.rs`) to emit tangents.

**Test:** a flat quad with a normal map shows correct lit relief as the light moves.

## Phase 5 (optional, later) - IBL

Image-based lighting: replace the flat `AMBIENT_COLOR` with light gathered from an
environment map (split-sum approximation, glTF style). This phase is large enough to
live in its own document, see `IBL.md`. Only start it once the analytic lighting from
Phases 1 to 4 is solid.

## Recommended coding order

**0a -> 0b -> 1 -> 2 -> 3 -> 4 -> 5.**

Phases 1 and 2 together give "real PBR" on screen. Phase 4 is the only one that
touches vertex layout, parser, and builders at once, so isolating it prevents churn
while the BRDF is still being tuned.
