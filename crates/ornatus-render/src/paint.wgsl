// Paint layer passes: stamp dabs into a coverage texture, composite that
// coverage onto the paint texture, and copy one texture into another.
// Textures use the texture coordinate convention: (0, 0) is the top left.

// ---- dabs -----------------------------------------------------------------

struct DabGlobals {
    // Texture width and height in texels.
    size: vec4<f32>,
}

@group(0) @binding(0) var<uniform> dab_globals: DabGlobals;

struct DabInput {
    @location(0) center: vec2<f32>,
    @location(1) radius: f32,
    @location(2) hardness: f32,
}

struct DabOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) hardness: f32,
    @location(2) texel_radius: f32,
}

@vertex
fn vs_dab(@builtin(vertex_index) index: u32, dab: DabInput) -> DabOutput {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let corner = corners[index];
    let uv = dab.center + corner * dab.radius;
    var out: DabOutput;
    out.clip = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.local = corner;
    out.hardness = dab.hardness;
    out.texel_radius = dab.radius * dab_globals.size.x;
    return out;
}

@fragment
fn fs_dab(input: DabOutput) -> @location(0) vec4<f32> {
    let d = length(input.local);
    // Keep at least about a texel of softness so hard edges are not jagged.
    let texel = 1.0 / max(input.texel_radius, 1.0);
    let inner = clamp(min(input.hardness, 1.0 - 1.5 * texel), 0.0, 0.99);
    let coverage = 1.0 - smoothstep(inner, 1.0, d);
    return vec4<f32>(coverage, 0.0, 0.0, 0.0);
}

// ---- composite ------------------------------------------------------------

struct StrokeParams {
    // rgb: color (sRGB, 0 to 1). a: opacity.
    color: vec4<f32>,
}

@group(0) @binding(0) var scratch: texture_2d<f32>;
@group(0) @binding(1) var<uniform> stroke: StrokeParams;

@vertex
fn vs_full(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    return vec4<f32>(x, y, 0.0, 1.0);
}

fn coverage_at(position: vec4<f32>) -> f32 {
    return textureLoad(scratch, vec2<i32>(position.xy), 0).r * stroke.color.a;
}

// Premultiplied output, blended "over" the destination.
@fragment
fn fs_paint(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let a = coverage_at(position);
    return vec4<f32>(stroke.color.rgb * a, a);
}

// Only the alpha matters: the blend state scales the destination by 1 - a.
@fragment
fn fs_erase(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, coverage_at(position));
}

// ---- copy -----------------------------------------------------------------

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;

struct BlitOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex
fn vs_blit(@builtin(vertex_index) index: u32) -> BlitOutput {
    let x = f32((index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(index & 2u) * 2.0 - 1.0;
    var out: BlitOutput;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_blit(input: BlitOutput) -> @location(0) vec4<f32> {
    return textureSample(source, source_sampler, input.uv);
}
