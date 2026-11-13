// Physically based shading lit by a procedural studio environment: a soft
// gradient room with three soft boxes (key, fill, rim). The environment is
// evaluated analytically, so no HDR file is shipped. Back faces are lit from
// their own side so thin quads look right from both sides.

struct Camera {
    view_proj: mat4x4<f32>,
    // xyz: eye position. w: 1 when the target needs manual sRGB encoding.
    eye: vec4<f32>,
}

struct Material {
    base_color: vec4<f32>,
    // x: metallic, y: roughness, z: selection highlight, 0 to 1.
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> material: Material;

// Freehand paint on the selected surface (see paint.wgsl). Premultiplied sRGB.
struct StrokePreview {
    // rgb: color (sRGB), a: opacity.
    color: vec4<f32>,
    // x: 1 when erasing, y: 1 while a stroke is being drawn.
    flags: vec4<f32>,
}

@group(2) @binding(0) var paint_texture: texture_2d<f32>;
@group(2) @binding(1) var stroke_coverage: texture_2d<f32>;
@group(2) @binding(2) var paint_sampler: sampler;
@group(2) @binding(3) var<uniform> preview: StrokePreview;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip = camera.view_proj * vec4<f32>(input.position, 1.0);
    out.world = input.position;
    out.normal = input.normal;
    out.uv = input.uv;
    return out;
}

// A soft box: a lobe around `axis`. Higher roughness widens and dims it.
fn soft_box(d: vec3<f32>, axis: vec3<f32>, color: vec3<f32>, roughness: f32) -> vec3<f32> {
    let sharpness = mix(80.0, 2.5, roughness);
    let energy = mix(1.0, 0.18, roughness);
    return color * energy * exp(sharpness * (dot(d, normalize(axis)) - 1.0));
}

// The studio, seen from direction `d` through a blur set by `roughness`.
fn studio(d: vec3<f32>, roughness: f32) -> vec3<f32> {
    let t = clamp(d.y * 0.5 + 0.5, 0.0, 1.0);
    var c = mix(vec3<f32>(0.035, 0.035, 0.04), vec3<f32>(0.30, 0.32, 0.37), smoothstep(0.1, 1.0, t));
    c += soft_box(d, vec3<f32>(0.6, 0.8, 0.5), vec3<f32>(5.0, 4.8, 4.5), roughness);
    c += soft_box(d, vec3<f32>(-0.9, 0.3, 0.4), vec3<f32>(1.4, 1.7, 2.4), roughness);
    c += soft_box(d, vec3<f32>(0.1, 0.5, -1.0), vec3<f32>(2.5, 2.5, 3.0), roughness);
    return c;
}

// Analytic split-sum BRDF fit (Karis), so no lookup texture is needed.
fn env_brdf(f0: vec3<f32>, roughness: f32, n_dot_v: f32) -> vec3<f32> {
    let c0 = vec4<f32>(-1.0, -0.0275, -0.572, 0.022);
    let c1 = vec4<f32>(1.0, 0.0425, 1.04, -0.04);
    let r = roughness * c0 + c1;
    let a004 = min(r.x * r.x, exp2(-9.28 * n_dot_v)) * r.x + r.y;
    let ab = vec2<f32>(-1.04, 1.04) * a004 + r.zw;
    return f0 * ab.x + vec3<f32>(ab.y);
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return pow(c, vec3<f32>(2.2));
}

// The paint at a point, including the stroke that is still being drawn.
fn paint_at(uv: vec2<f32>) -> vec4<f32> {
    var paint = textureSample(paint_texture, paint_sampler, uv);
    let coverage = textureSample(stroke_coverage, paint_sampler, uv).r * preview.color.a * preview.flags.y;
    if preview.flags.x > 0.5 {
        paint = paint * (1.0 - coverage);
    } else {
        paint = vec4<f32>(preview.color.rgb * coverage, coverage) + paint * (1.0 - coverage);
    }
    return paint;
}

fn tonemap(c: vec3<f32>) -> vec3<f32> {
    // ACES filmic approximation (Narkowicz).
    let x = c * 0.7;
    return clamp((x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(input.normal);
    if !front {
        n = -n;
    }
    let v = normalize(camera.eye.xyz - input.world);
    let n_dot_v = max(dot(n, v), 1e-3);
    // Texture reads must happen in uniform control flow, before any branches.
    let paint = paint_at(input.uv);
    var albedo = material.base_color.rgb;
    if paint.a > 1e-4 {
        albedo = mix(albedo, srgb_to_linear(paint.rgb / paint.a), paint.a);
    }
    let metallic = clamp(material.params.x, 0.0, 1.0);
    let roughness = clamp(material.params.y, 0.04, 1.0);

    let f0 = mix(vec3<f32>(0.04), albedo, metallic);
    let diffuse_color = albedo * (1.0 - metallic);
    let irradiance = studio(n, 1.0);
    let reflected = reflect(-v, n);
    let prefiltered = studio(reflected, roughness);
    let color = diffuse_color * irradiance + prefiltered * env_brdf(f0, roughness, n_dot_v);

    var out_color = tonemap(color);
    // Selected surface: tint towards the accent color.
    out_color = mix(out_color, vec3<f32>(0.47, 0.55, 1.0), 0.32 * material.params.z);
    if camera.eye.w > 0.5 {
        out_color = pow(out_color, vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(out_color, material.base_color.a);
}

// Problem areas found by the UV audit, drawn on top of the surface.
@fragment
fn fs_flag() -> @location(0) vec4<f32> {
    return vec4<f32>(1.0, 0.18, 0.22, 0.6);
}
