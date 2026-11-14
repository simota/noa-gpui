// One draw call per run of instances that share a texture.

@group(1) @binding(0) var image_texture: texture_2d<f32>;
@group(1) @binding(1) var image_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,  // x, y, width, height (logical px)
    @location(1) uv: vec4<f32>,    // u0, v0, u1, v1
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    let corner = strip_corner(vertex_index);
    var out: VertexOutput;
    out.position = to_clip(instance.rect.xy + corner * instance.rect.zw);
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    // The texture's format already matches the target's encoding.
    return textureSample(image_texture, image_sampler, in.uv);
}
