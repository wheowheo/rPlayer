struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    return out;
}

// --- RGBA path ---
@group(0) @binding(0) var t_video: texture_2d<f32>;
@group(0) @binding(1) var s_video: sampler;

@fragment
fn fs_rgba(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_video, s_video, in.tex_coords);
}

// --- YUV420P path (3 planes: Y, U, V as R8 textures) ---
@group(0) @binding(0) var t_y: texture_2d<f32>;
@group(0) @binding(1) var t_u: texture_2d<f32>;
@group(0) @binding(2) var t_v: texture_2d<f32>;
@group(0) @binding(3) var s_yuv: sampler;

@fragment
fn fs_yuv(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(t_y, s_yuv, in.tex_coords).r;
    let u = textureSample(t_u, s_yuv, in.tex_coords).r;
    let v = textureSample(t_v, s_yuv, in.tex_coords).r;

    // BT.601 YUV to RGB
    let r = y + 1.402 * (v - 0.5);
    let g = y - 0.344136 * (u - 0.5) - 0.714136 * (v - 0.5);
    let b = y + 1.772 * (u - 0.5);

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}

// --- NV12 path (2 planes: Y as R8, UV interleaved as RG8) ---
@group(0) @binding(0) var t_nv12_y: texture_2d<f32>;
@group(0) @binding(1) var t_nv12_uv: texture_2d<f32>;
@group(0) @binding(2) var s_nv12: sampler;

@fragment
fn fs_nv12(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(t_nv12_y, s_nv12, in.tex_coords).r;
    let uv = textureSample(t_nv12_uv, s_nv12, in.tex_coords).rg;
    let u = uv.r;
    let v = uv.g;

    let r = y + 1.402 * (v - 0.5);
    let g = y - 0.344136 * (u - 0.5) - 0.714136 * (v - 0.5);
    let b = y + 1.772 * (u - 0.5);

    return vec4<f32>(clamp(r, 0.0, 1.0), clamp(g, 0.0, 1.0), clamp(b, 0.0, 1.0), 1.0);
}
