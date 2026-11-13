// Studio-lit model shader. Analytic lights for now; image-based lighting
// comes later. Back faces are lit from their own side so thin quads look
// right from both sides.

struct Camera {
    view_proj: mat4x4<f32>,
    // xyz: eye position. w: 1 when the target needs manual sRGB encoding.
    eye: vec4<f32>,
}

struct Material {
    base_color: vec4<f32>,
    // x: metallic, y: roughness.
    params: vec4<f32>,
}

@group(0) @binding(0) var<uniform> camera: Camera;
@group(1) @binding(0) var<uniform> material: Material;

struct VertexInput {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) uv: vec2<f32>,
}

struct VertexOutput {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
}

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip = camera.view_proj * vec4<f32>(input.position, 1.0);
    out.world = input.position;
    out.normal = input.normal;
    return out;
}

fn light(n: vec3<f32>, v: vec3<f32>, dir: vec3<f32>, color: vec3<f32>, albedo: vec3<f32>, metallic: f32, roughness: f32) -> vec3<f32> {
    let l = normalize(dir);
    let h = normalize(l + v);
    let diffuse = max(dot(n, l), 0.0) * albedo * (1.0 - metallic);
    let shininess = mix(8.0, 128.0, 1.0 - roughness);
    let spec_color = mix(vec3<f32>(0.04), albedo, metallic);
    let specular = pow(max(dot(n, h), 0.0), shininess) * spec_color * (1.0 - roughness * 0.7);
    return (diffuse + specular) * color;
}

@fragment
fn fs_main(input: VertexOutput, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(input.normal);
    if !front {
        n = -n;
    }
    let v = normalize(camera.eye.xyz - input.world);
    let albedo = material.base_color.rgb;
    let metallic = material.params.x;
    let roughness = material.params.y;

    let sky = mix(vec3<f32>(0.10, 0.10, 0.12), vec3<f32>(0.32, 0.34, 0.40), n.y * 0.5 + 0.5);
    var color = sky * albedo * (1.0 - metallic * 0.6);
    color += light(n, v, vec3<f32>(0.5, 0.9, 0.6), vec3<f32>(1.0, 0.97, 0.92) * 1.6, albedo, metallic, roughness);
    color += light(n, v, vec3<f32>(-0.8, 0.3, 0.4), vec3<f32>(0.55, 0.65, 0.9) * 0.7, albedo, metallic, roughness);
    color += light(n, v, vec3<f32>(0.0, 0.4, -1.0), vec3<f32>(0.9, 0.9, 1.0) * 0.8, albedo, metallic, roughness);

    var out_color = color;
    if camera.eye.w > 0.5 {
        out_color = pow(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), vec3<f32>(1.0 / 2.2));
    }
    return vec4<f32>(out_color, material.base_color.a);
}
