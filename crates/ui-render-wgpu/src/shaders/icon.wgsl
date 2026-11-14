@group(1) @binding(0) var mask_texture: texture_2d<f32>;
@group(1) @binding(1) var mask_sampler: sampler;

struct Instance {
    @location(0) rect: vec4<f32>,   // x, y, width, height (logical px)
    @location(1) uv: vec4<f32>,     // u0, v0, u1, v1
    @location(2) color: vec4<f32>,  // sRGB, straight alpha
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    let corner = strip_corner(vertex_index);
    var out: VertexOutput;
    out.position = to_clip(instance.rect.xy + corner * instance.rect.zw);
    out.uv = mix(instance.uv.xy, instance.uv.zw, corner);
    out.color = instance.color;
    return out;
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let coverage = textureSample(mask_texture, mask_sampler, in.uv).r;
    return output_color(vec4<f32>(in.color.rgb, in.color.a * coverage));
}
