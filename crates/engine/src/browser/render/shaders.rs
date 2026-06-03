//! WGSL shader source strings.
//!
//! BLUR: 2-pass gaussian blur (separable). RECT: solid/text/gradient/shadow
//! multi-mode shader. TRANSFORM: 4x4 matrix s perspective. COMPOSE: filter
//! result -> swap chain s color matrix.

/// Separable Gaussian blur shader - 2 pass (horizontal + vertical).
/// Sample 9 tapu s gauss vahami.
pub(super) const BLUR_SHADER: &str = r#"
struct BlurParams {
    /// direction.x = 1 horizontal, .y = 1 vertical
    direction: vec2<f32>,
    /// blur radius in pixels
    radius: f32,
    /// texel size 1/width or 1/height
    texel: f32,
};
@group(0) @binding(0) var<uniform> params: BlurParams;
@group(0) @binding(1) var src_tex: texture_2d<f32>;
@group(0) @binding(2) var src_smp: sampler;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    // Fullscreen triangle
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOut;
    out.clip = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // 9-tap Gaussian (sigma ~ radius/3). Unrolled - WGSL nepovoluje dynamic
    // indexing var/let array v Naga validation (jen const-indexable v uniform/storage).
    let w0: f32 = 0.227027;
    let w1: f32 = 0.1945946;
    let w2: f32 = 0.1216216;
    let w3: f32 = 0.054054;
    let w4: f32 = 0.016216;
    let step = params.direction * params.texel * params.radius * 0.3;
    var color = textureSample(src_tex, src_smp, in.uv) * w0;
    let off1 = step * 1.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off1) * w1;
    color = color + textureSample(src_tex, src_smp, in.uv - off1) * w1;
    let off2 = step * 2.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off2) * w2;
    color = color + textureSample(src_tex, src_smp, in.uv - off2) * w2;
    let off3 = step * 3.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off3) * w3;
    color = color + textureSample(src_tex, src_smp, in.uv - off3) * w3;
    let off4 = step * 4.0;
    color = color + textureSample(src_tex, src_smp, in.uv + off4) * w4;
    color = color + textureSample(src_tex, src_smp, in.uv - off4) * w4;
    return color;
}
"#;

/// 3D Transform compose shader. Renderuje 4-vertex quad transformovany 4x4
/// matici (vc perspective) v px space, sample z offscreen_tex pres uv region.
pub(super) const TRANSFORM_SHADER: &str = r#"
struct TransformParams {
    /// 4x4 row-major matrix (CSS transform incl. parent perspective).
    /// V WGSL ulozeno jako 4 vec4 (po radkach).
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
    row3: vec4<f32>,
    /// (cx, cy, hw, hh) - center bbox v px + half-size.
    center: vec4<f32>,
    /// (viewport_w, viewport_h, _, _)
    viewport: vec4<f32>,
    /// (u0, v0, u1, v1) - region z offscreen RT k samplovani.
    uv_box: vec4<f32>,
};
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> tp: TransformParams;

struct VOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VOut {
    // 6 vrcholu pro dva trianlges (0,1,2 + 0,2,3)
    var corners = array<vec2<f32>, 4>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 1.0, -1.0),
        vec2<f32>( 1.0,  1.0),
        vec2<f32>(-1.0,  1.0),
    );
    var uvs = array<vec2<f32>, 4>(
        vec2<f32>(tp.uv_box.x, tp.uv_box.y),
        vec2<f32>(tp.uv_box.z, tp.uv_box.y),
        vec2<f32>(tp.uv_box.z, tp.uv_box.w),
        vec2<f32>(tp.uv_box.x, tp.uv_box.w),
    );
    var ix = array<u32, 6>(0u, 1u, 2u, 0u, 2u, 3u);
    let i = ix[idx];
    let c = corners[i];
    // Local px space (centered)
    let lx = c.x * tp.center.z;
    let ly = c.y * tp.center.w;
    let p = vec4<f32>(lx, ly, 0.0, 1.0);
    // Apply matrix (row-major: dot s row vec4)
    let tx = dot(tp.row0, p);
    let ty = dot(tp.row1, p);
    let tz = dot(tp.row2, p);
    let tw = dot(tp.row3, p);
    // Perspective divide
    let inv_w = 1.0 / max(tw, 0.0001);
    let px = tx * inv_w + tp.center.x;
    let py = ty * inv_w + tp.center.y;
    let nx = (px / tp.viewport.x) * 2.0 - 1.0;
    let ny = 1.0 - (py / tp.viewport.y) * 2.0;
    // wgpu NDC z range = [0, 1]. Pri 3D rotaci tz muze byt v sirokem rozsahu
    // (rotateY(45) -> tz ∈ [-hw, +hw]) -> mimo [0,1] = fragment clipped =
    // pulka rotace nezobrazena. Fix: pevny nz = 0.5 (vsechno na same depth -
    // ne real 3D, ale spravne 2D-style render rotovaneho elementu).
    let nz = 0.5;
    var out: VOut;
    out.clip = vec4<f32>(nx, ny, nz, 1.0);
    out.uv = uvs[i];
    return out;
}

@fragment
fn fs_main(in: VOut) -> @location(0) vec4<f32> {
    return textureSample(src_tex, src_smp, in.uv);
}
"#;

/// Compose shader - samples offscreen_tex (mid-blur result) a kresli do swap chain.
/// Aplikuje 4x5 color matrix (vetsina filter operaci) - identity = passthrough.
/// Advanced mix-blend-mode shader pres per-pixel dst sample.
/// 2 textures (src + dst snapshot) + uniform blend_mode_id.
/// Implementuje vsech 16 modes per CSS Compositing-1 spec.
/// Inspired by Skia `src/effects/SkBlendMode.cpp`.
pub(super) const ADVANCED_BLEND_SHADER: &str = r#"
struct BlendParams {
    blend_mode: u32,
    opacity: f32,
    _pad0: f32,
    _pad1: f32,
};
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var dst_tex: texture_2d<f32>;
@group(0) @binding(3) var dst_smp: sampler;
@group(0) @binding(4) var<uniform> params: BlendParams;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var pos = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>( 3.0, -1.0),
        vec2<f32>(-1.0,  3.0),
    );
    var uv = array<vec2<f32>, 3>(
        vec2<f32>(0.0, 1.0),
        vec2<f32>(2.0, 1.0),
        vec2<f32>(0.0, -1.0),
    );
    var out: VertexOut;
    out.clip = vec4<f32>(pos[idx], 0.0, 1.0);
    out.uv = uv[idx];
    return out;
}

fn blend_each(mode: u32, s: f32, d: f32) -> f32 {
    if (mode == 1u) { return s * d; } // Multiply
    if (mode == 2u) { return 1.0 - (1.0 - s) * (1.0 - d); } // Screen
    if (mode == 3u) { // Overlay
        if (d < 0.5) { return 2.0 * s * d; }
        return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
    }
    if (mode == 4u) { return min(s, d); } // Darken
    if (mode == 5u) { return max(s, d); } // Lighten
    if (mode == 6u) { // ColorDodge
        if (s >= 1.0) { return 1.0; }
        return min(d / max(1.0 - s, 0.0001), 1.0);
    }
    if (mode == 7u) { // ColorBurn
        if (s <= 0.0) { return 0.0; }
        return 1.0 - min((1.0 - d) / max(s, 0.0001), 1.0);
    }
    if (mode == 8u) { // HardLight
        if (s < 0.5) { return 2.0 * s * d; }
        return 1.0 - 2.0 * (1.0 - s) * (1.0 - d);
    }
    if (mode == 9u) { // SoftLight
        if (s < 0.5) { return d - (1.0 - 2.0 * s) * d * (1.0 - d); }
        var g: f32 = sqrt(d);
        if (d <= 0.25) { g = ((16.0 * d - 12.0) * d + 4.0) * d; }
        return d + (2.0 * s - 1.0) * (g - d);
    }
    if (mode == 10u) { return abs(s - d); } // Difference
    if (mode == 11u) { return s + d - 2.0 * s * d; } // Exclusion
    if (mode == 16u) { return min(s + d, 1.0); } // PlusLighter
    return s; // Normal (or unsupported = src wins)
}

fn lum(c: vec3<f32>) -> f32 {
    return 0.3 * c.r + 0.59 * c.g + 0.11 * c.b;
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    let d = l - lum(c);
    return clamp(c + vec3<f32>(d), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    // Simplified: scale color s preserved hue.
    let cur = sat(c);
    if (cur < 0.0001) { return vec3<f32>(0.0); }
    let mn = min(c.r, min(c.g, c.b));
    return (c - vec3<f32>(mn)) * (s / cur);
}

fn blend_hsl(mode: u32, src: vec3<f32>, dst: vec3<f32>) -> vec3<f32> {
    if (mode == 12u) { return set_lum(set_sat(src, sat(dst)), lum(dst)); } // Hue
    if (mode == 13u) { return set_lum(set_sat(dst, sat(src)), lum(dst)); } // Saturation
    if (mode == 14u) { return set_lum(src, lum(dst)); } // Color
    if (mode == 15u) { return set_lum(dst, lum(src)); } // Luminosity
    return src;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let src = textureSample(src_tex, src_smp, in.uv);
    let dst = textureSample(dst_tex, dst_smp, in.uv);
    let mode = params.blend_mode;
    var blended: vec3<f32>;
    if (mode >= 12u && mode <= 15u) {
        blended = blend_hsl(mode, src.rgb, dst.rgb);
    } else {
        blended = vec3<f32>(
            blend_each(mode, src.r, dst.r),
            blend_each(mode, src.g, dst.g),
            blend_each(mode, src.b, dst.b),
        );
    }
    // Porter-Duff over s blended color: out = src.a*blended + (1-src.a)*dst
    let sa = src.a * params.opacity;
    let out_rgb = sa * blended + (1.0 - sa) * dst.rgb;
    let out_a = sa + dst.a * (1.0 - sa);
    return vec4<f32>(out_rgb, out_a);
}
"#;

pub(super) const COMPOSE_SHADER: &str = r#"
struct ComposeParams {
    row0: vec4<f32>,
    row1: vec4<f32>,
    row2: vec4<f32>,
    row3: vec4<f32>,
    offset: vec4<f32>,
    dst_box: vec4<f32>,
    src_uv: vec4<f32>,
};
@group(0) @binding(0) var src_tex: texture_2d<f32>;
@group(0) @binding(1) var src_smp: sampler;
@group(0) @binding(2) var<uniform> params: ComposeParams;

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOut {
    var base = array<vec2<f32>, 6>(
        vec2<f32>(0.0, 0.0), vec2<f32>(1.0, 0.0), vec2<f32>(0.0, 1.0),
        vec2<f32>(1.0, 0.0), vec2<f32>(1.0, 1.0), vec2<f32>(0.0, 1.0),
    );
    let b = base[idx];
    let pos_x = mix(params.dst_box.x, params.dst_box.z, b.x);
    let pos_y = mix(params.dst_box.w, params.dst_box.y, b.y);
    let uv_x = mix(params.src_uv.x, params.src_uv.z, b.x);
    let uv_y = mix(params.src_uv.y, params.src_uv.w, b.y);
    var out: VertexOut;
    out.clip = vec4<f32>(pos_x, pos_y, 0.0, 1.0);
    out.uv = vec2<f32>(uv_x, uv_y);
    return out;
}

// Detekuje identity color matrix - skip gamma roundtrip pres LAYER compose path
// (per-layer/per-tile composite je VZDY identity matrix). Gamma roundtrip
// (linear -> sRGB -> srgb_to_linear) na pow() fp precision ztracel saturaci -
// barvy renderovaly jako stupne sedi.
//
// Pro CSS filter matrices (sepia/hue-rotate/saturate) - jine code path
// (compose_offscreen) prochazi pres gamma. Tady jen identity = direct passthrough.
fn is_identity(p: ComposeParams) -> bool {
    let eps = 0.0001;
    return abs(p.row0.x - 1.0) < eps && abs(p.row0.y) < eps && abs(p.row0.z) < eps
        && abs(p.row1.x) < eps && abs(p.row1.y - 1.0) < eps && abs(p.row1.z) < eps
        && abs(p.row2.x) < eps && abs(p.row2.y) < eps && abs(p.row2.z - 1.0) < eps
        && abs(p.offset.x) < eps && abs(p.offset.y) < eps && abs(p.offset.z) < eps;
}

fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.0031308);
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(1.0/2.4)) * 1.055 - 0.055;
    return select(hi, lo, c < cutoff);
}
fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.04045);
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c < cutoff);
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let src_linear = textureSample(src_tex, src_smp, in.uv);
    // Fast path: identity matrix (layer compose) - skip gamma roundtrip.
    if (is_identity(params)) {
        let a = params.row3.w * src_linear.a + params.offset.w;
        return vec4<f32>(src_linear.rgb * params.row3.w, a);
    }
    // CSS filter path: gamma to sRGB, apply matrix in sRGB space, gamma back.
    let src_srgb_rgb = linear_to_srgb(src_linear.rgb);
    let src = vec4<f32>(src_srgb_rgb, src_linear.a);
    let r = dot(params.row0, src) + params.offset.x;
    let g = dot(params.row1, src) + params.offset.y;
    let b = dot(params.row2, src) + params.offset.z;
    let a = dot(params.row3, src) + params.offset.w;
    let out_linear_rgb = srgb_to_linear(vec3<f32>(r, g, b));
    return vec4<f32>(out_linear_rgb, a);
}
"#;

pub(super) const RECT_SHADER: &str = r#"
struct Uniforms {
    /// (logical_w, logical_h, zoom, _pad). vp je v logical px (window/zoom).
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_smp: sampler;
@group(0) @binding(3) var image_tex: texture_2d<f32>;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mode: f32,
    @location(4) local: vec2<f32>,
    @location(5) half_size: vec2<f32>,
    @location(6) radius: f32,
    @location(7) color2: vec4<f32>,
    @location(8) blur: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) mode: f32,
    @location(3) local: vec2<f32>,
    @location(4) half_size: vec2<f32>,
    @location(5) radius: f32,
    @location(6) color2: vec4<f32>,
    @location(7) blur: f32,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let x = (in.pos.x / u.viewport.x) * 2.0 - 1.0;
    let y = 1.0 - (in.pos.y / u.viewport.y) * 2.0;
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    out.mode = in.mode;
    out.local = in.local;
    out.half_size = in.half_size;
    out.radius = in.radius;
    out.color2 = in.color2;
    out.blur = in.blur;
    return out;
}

/// Signed distance to rounded rectangle.
fn sdf_rounded_box(p: vec2<f32>, half_size: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - half_size + vec2<f32>(r, r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0, 0.0))) - r;
}

// Gradient mixing v sRGB space. CSS spec: linear-gradient interpolation v sRGB
// (legacy) nebo oklab (modern default). Linear-space mix dava prilis svetly mid
// (red->blue mid je purple bright misto darkish purple per Chrome).
fn lin_to_srgb_v(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.0031308);
    let lo = c * 12.92;
    let hi = pow(c, vec3<f32>(1.0/2.4)) * 1.055 - 0.055;
    return select(hi, lo, c < cutoff);
}
fn srgb_to_lin_v(c: vec3<f32>) -> vec3<f32> {
    let cutoff = vec3<f32>(0.04045);
    let lo = c / 12.92;
    let hi = pow((c + 0.055) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c < cutoff);
}
fn mix_srgb(a: vec4<f32>, b: vec4<f32>, t: f32) -> vec4<f32> {
    let a_srgb = lin_to_srgb_v(a.rgb);
    let b_srgb = lin_to_srgb_v(b.rgb);
    let mixed = mix(a_srgb, b_srgb, t);
    return vec4<f32>(srgb_to_lin_v(mixed), mix(a.a, b.a, t));
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    // Mode 1: text - sample atlas (grayscale alpha)
    if (in.mode > 0.5 && in.mode < 1.5) {
        let alpha = textureSample(atlas_tex, atlas_smp, in.uv).r;
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 9: LCD subpixel atlas storage (3x horizontal). Bez dual-source
    // blendu proper LCD nelze - per-channel modulace v standard alpha blendu
    // dela barevny artifacts. Pouzivame avg ze 3 sub-pixelu = grayscale
    // approximation s o trochu lepsi AA nez fontdue 1x raster.
    if (in.mode > 8.5 && in.mode < 9.5) {
        // Gamma-correct subpixel coverage:
        // 1) sample 3 sousedni subpixely R/G/B
        // 2) prevedeme do linear (^2.2 aproximace srgb)
        // 3) avg v linear space (gamma-correct blending)
        // 4) zpet do srgb pres ^(1/2.2)
        // Bez dual-source = grayscale ale spravne tonove. Bez gamma corr
        // glyfy vychazely o 8-12% tmavsi nez ocekavane (sub-linear blending).
        let dims = textureDimensions(atlas_tex);
        let texel_w = 1.0 / f32(dims.x);
        let r_a = textureSample(atlas_tex, atlas_smp, in.uv - vec2<f32>(texel_w, 0.0)).r;
        let g_a = textureSample(atlas_tex, atlas_smp, in.uv).r;
        let b_a = textureSample(atlas_tex, atlas_smp, in.uv + vec2<f32>(texel_w, 0.0)).r;
        let r_lin = pow(r_a, 2.2);
        let g_lin = pow(g_a, 2.2);
        let b_lin = pow(b_a, 2.2);
        let avg_lin = (r_lin + g_lin + b_lin) * (1.0 / 3.0);
        let avg = pow(avg_lin, 1.0 / 2.2);
        return vec4<f32>(in.color.rgb, in.color.a * avg);
    }
    // Mode 2: linear gradient - lerp color->color2 podle uv.x (pre-rotated)
    if (in.mode > 1.5 && in.mode < 2.5) {
        let t = clamp(in.uv.x, 0.0, 1.0);
        var rgba = mix_srgb(in.color, in.color2, t);
        if (in.radius > 0.5) {
            let d = sdf_rounded_box(in.local, in.half_size, in.radius);
            let aa_range = 1.0 / max(u.viewport.z, 0.0001);
            let aa = 1.0 - smoothstep(-aa_range, aa_range, d);
            rgba = vec4<f32>(rgba.rgb, rgba.a * aa);
        }
        return rgba;
    }
    // Mode 3: shadow with blur (Gaussian-like fade)
    if (in.mode > 2.5 && in.mode < 3.5) {
        let blur = max(in.blur, 1.0);
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        let alpha = 1.0 - smoothstep(-blur, blur, d);
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 6: radial gradient - CSS ellipse farthest-corner (default).
    // half_size = (rx, ry) ellipse radii (= box half-w, half-h). local =
    // pos relativni k gradient centru. Normalize local by axes -> ellipse
    // distance (= circle pres square box, ellipse pres rect). Drive d =
    // length(local) circular = ne-aspect-aware = "moc kulaty pres rect box".
    if (in.mode > 5.5 && in.mode < 6.5) {
        let nx = in.local.x / max(in.half_size.x, 0.001);
        let ny = in.local.y / max(in.half_size.y, 0.001);
        let d = length(vec2<f32>(nx, ny));
        let t = clamp(d, 0.0, 1.0);
        return mix_srgb(in.color, in.color2, t);
    }
    // Mode 7: conic gradient - CSS spec: start at TOP (12 o'clock), rotate
    // clockwise. atan2(p.x, -p.y) -> 0 at top, +π/2 at right, π at bottom.
    // Drive atan2(p.y, p.x) = math angle z +X axis = 0 at right = yellow vpravo
    // shift na bottom.
    if (in.mode > 6.5 && in.mode < 7.5) {
        let p = in.local - in.half_size;
        var ang = atan2(p.x, -p.y) - in.blur;
        let two_pi = 6.28318530718;
        ang = ang - floor(ang / two_pi) * two_pi;
        let t = clamp(ang / two_pi, 0.0, 1.0);
        return mix_srgb(in.color, in.color2, t);
    }
    // Mode 5: inset shadow - kresli uvnitr boxu, fade smerem dovnitr od okraju
    if (in.mode > 4.5 && in.mode < 5.5) {
        let blur = max(in.blur, 1.0);
        // Offset shift sample center: pri offset (ox, oy) shadow se posune v opacnem smeru
        let p = in.local - vec2<f32>(in.color2.x, in.color2.y);
        let d = sdf_rounded_box(p, in.half_size, in.radius);
        // Pozitivni d = vne boxu = nezobrazi (mimo cliping kvadr)
        // Negativni d = uvnitr -> alpha podle vzdalenosti od okraje
        let alpha = smoothstep(-blur, blur, d);
        // Clip mimo box
        let outer = sdf_rounded_box(in.local, in.half_size, in.radius);
        let aa_range = 1.0 / max(u.viewport.z, 0.0001);
        let clip = 1.0 - smoothstep(-aa_range, aa_range, outer);
        return vec4<f32>(in.color.rgb, in.color.a * alpha * clip);
    }
    // Mode 8: blurred solid - smoothstep s blur radius na okrajich
    if (in.mode > 7.5 && in.mode < 8.5) {
        let blur = max(in.blur, 0.5);
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        // Inside box (d < -blur): full alpha
        // Outside (d > blur): zero alpha
        // Edge: smoothstep
        let alpha = 1.0 - smoothstep(-blur, blur, d);
        return vec4<f32>(in.color.rgb, in.color.a * alpha);
    }
    // Mode 4: image - sample RGBA z image atlasu
    if (in.mode > 3.5 && in.mode < 4.5) {
        var rgba = textureSample(image_tex, atlas_smp, in.uv);
        rgba.a = rgba.a * in.color.a;
        if (in.radius > 0.5) {
            let d = sdf_rounded_box(in.local, in.half_size, in.radius);
            // AA range = 1 physical px = 1/zoom logical px (smoothstep symetric).
            let aa_range = 1.0 / max(u.viewport.z, 0.0001);
            let aa = 1.0 - smoothstep(-aa_range, aa_range, d);
            rgba.a = rgba.a * aa;
        }
        return rgba;
    }
    // Mode 0: solid s rounded corners
    if (in.radius > 0.5) {
        let d = sdf_rounded_box(in.local, in.half_size, in.radius);
        let aa_range = 1.0 / max(u.viewport.z, 0.0001);
        let aa = 1.0 - smoothstep(-aa_range, aa_range, d);
        return vec4<f32>(in.color.rgb, in.color.a * aa);
    }
    return in.color;
}
"#;

/// Separate shader pro LCD subpixel text (dual-source blending).
/// Vyzaduje DUAL_SOURCE_BLENDING device feature - kompiluje se jen kdyz HW
/// podporuje. Bez supportu zustava jako None pipeline.
/// WGSL @blend_src(0)/(1) attributes na stejnem @location(0).
pub(super) const LCD_SHADER: &str = r#"
enable dual_source_blending;
struct Uniforms {
    viewport: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(0) @binding(1) var atlas_tex: texture_2d<f32>;
@group(0) @binding(2) var atlas_smp: sampler;
@group(0) @binding(3) var image_tex: texture_2d<f32>;
@group(0) @binding(4) var image_smp: sampler;

struct VertexIn {
    @location(0) pos: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) uv: vec2<f32>,
    @location(3) mode: f32,
    @location(4) local: vec2<f32>,
    @location(5) half_size: vec2<f32>,
    @location(6) radius: f32,
    @location(7) color2: vec4<f32>,
    @location(8) blur: f32,
};

struct VertexOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
};

@vertex
fn vs_main(in: VertexIn) -> VertexOut {
    var out: VertexOut;
    let vp_w = u.viewport.x;
    let vp_h = u.viewport.y;
    let ndc_x = (in.pos.x / vp_w) * 2.0 - 1.0;
    let ndc_y = 1.0 - (in.pos.y / vp_h) * 2.0;
    out.clip = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.color = in.color;
    out.uv = in.uv;
    return out;
}

struct LcdOut {
    @location(0) @blend_src(0) color: vec4<f32>,
    @location(0) @blend_src(1) blend: vec4<f32>,
};

@fragment
fn fs_main(in: VertexOut) -> LcdOut {
    let dims = textureDimensions(atlas_tex);
    let texel_w = 1.0 / f32(dims.x);
    let r_a = textureSample(atlas_tex, atlas_smp, in.uv - vec2<f32>(texel_w, 0.0)).r;
    let g_a = textureSample(atlas_tex, atlas_smp, in.uv).r;
    let b_a = textureSample(atlas_tex, atlas_smp, in.uv + vec2<f32>(texel_w, 0.0)).r;
    var out: LcdOut;
    out.color = vec4<f32>(in.color.rgb, 1.0);
    out.blend = vec4<f32>(r_a * in.color.a, g_a * in.color.a, b_a * in.color.a, 1.0);
    return out;
}
"#;
