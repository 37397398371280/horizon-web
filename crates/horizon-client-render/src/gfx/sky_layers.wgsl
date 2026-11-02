// Mods' sky layers (sky_layers.rs): a textured cube around the camera, OptiFine's custom sky.
struct Layer {
    // The camera-relative view projection times the layer's rotation.
    clip_from_layer: mat4x4<f32>,
    // The blend's colour for the layer's strength (SkyBlend::color).
    color: vec4<f32>,
}
@group(0) @binding(0) var<uniform> layer: Layer;
@group(0) @binding(1) var faces: texture_2d<f32>;
@group(0) @binding(2) var faces_sampler: sampler;

struct Out {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@vertex fn vs_sky_layer(@location(0) position: vec3<f32>, @location(1) uv: vec2<f32>) -> Out {
    var out: Out;
    let clip = layer.clip_from_layer * vec4(position, 1.0);
    // On the far plane (reversed depth: 0), so the view distance never cuts the cube.
    out.position = vec4(clip.xy, 0.0, clip.w);
    out.uv = uv;
    return out;
}

@fragment fn fs_sky_layer(in: Out) -> @location(0) vec4<f32> {
    return textureSample(faces, faces_sampler, in.uv) * layer.color;
}
