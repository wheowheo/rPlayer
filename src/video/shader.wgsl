struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coords: vec2<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
};

// Color conversion parameters (set per-frame from CPU)
struct ColorParams {
    // YUV→RGB matrix (row-major 3x3, padded to 3x vec4)
    row0: vec4<f32>,  // [m00, m01, m02, 0]
    row1: vec4<f32>,  // [m10, m11, m12, 0]
    row2: vec4<f32>,  // [m20, m21, m22, 0]
    // Range offset: Y offset, UV offset, Y scale, UV scale
    range: vec4<f32>, // [y_off, uv_off, y_scale, uv_scale]
};

@vertex
fn vs_main(in: VertexInput) -> VertexOutput {
    var out: VertexOutput;
    out.clip_position = vec4<f32>(in.position, 0.0, 1.0);
    out.tex_coords = in.tex_coords;
    return out;
}

// Convert YUV to linear RGB using the provided color matrix and range
fn yuv_to_rgb(y_raw: f32, u_raw: f32, v_raw: f32, cp: ColorParams) -> vec3<f32> {
    // Apply range scaling: limited range → normalize to 0..1
    let y = (y_raw - cp.range.x) * cp.range.z;
    let u = (u_raw - cp.range.y) * cp.range.w;
    let v = (v_raw - cp.range.y) * cp.range.w;

    let yuv = vec3<f32>(y, u, v);
    let r = dot(cp.row0.xyz, yuv);
    let g = dot(cp.row1.xyz, yuv);
    let b = dot(cp.row2.xyz, yuv);
    return clamp(vec3<f32>(r, g, b), vec3<f32>(0.0), vec3<f32>(1.0));
}

// --- RGBA path ---
@group(0) @binding(0) var t_video: texture_2d<f32>;
@group(0) @binding(1) var s_video: sampler;

@fragment
fn fs_rgba(in: VertexOutput) -> @location(0) vec4<f32> {
    return textureSample(t_video, s_video, in.tex_coords);
}

// --- YUV420P path ---
@group(0) @binding(0) var t_y: texture_2d<f32>;
@group(0) @binding(1) var t_u: texture_2d<f32>;
@group(0) @binding(2) var t_v: texture_2d<f32>;
@group(0) @binding(3) var s_yuv: sampler;
@group(0) @binding(4) var<uniform> color_yuv: ColorParams;

@fragment
fn fs_yuv(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(t_y, s_yuv, in.tex_coords).r;
    let u = textureSample(t_u, s_yuv, in.tex_coords).r;
    let v = textureSample(t_v, s_yuv, in.tex_coords).r;
    let rgb = yuv_to_rgb(y, u, v, color_yuv);
    return vec4<f32>(rgb, 1.0);
}

// --- NV12 path ---
@group(0) @binding(0) var t_nv12_y: texture_2d<f32>;
@group(0) @binding(1) var t_nv12_uv: texture_2d<f32>;
@group(0) @binding(2) var s_nv12: sampler;
@group(0) @binding(3) var<uniform> color_nv12: ColorParams;

@fragment
fn fs_nv12(in: VertexOutput) -> @location(0) vec4<f32> {
    let y = textureSample(t_nv12_y, s_nv12, in.tex_coords).r;
    let uv = textureSample(t_nv12_uv, s_nv12, in.tex_coords).rg;
    let rgb = yuv_to_rgb(y, uv.r, uv.g, color_nv12);
    return vec4<f32>(rgb, 1.0);
}
