// Corners, borders and shapes are shaded with a signed distance field, so no per-shape
// geometry is generated.

struct Instance {
    @location(0) rect: vec4<f32>,          // x, y, width, height (logical px)
    @location(1) radii: vec4<f32>,         // top-left, top-right, bottom-right, bottom-left
    @location(2) color: vec4<f32>,         // sRGB, straight alpha
    @location(3) border_color: vec4<f32>,  // sRGB, straight alpha
    @location(4) border_width: f32,        // logical px; half the width for lines
    @location(5) shape: vec2<f32>,         // kind (0 box, 1 line, 2 triangle, 3 gradient, 4 star, 5 arc, 6 shadow), parameter
};

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    // Fragment position relative to the rectangle's center (logical px).
    @location(0) local: vec2<f32>,
    @location(1) @interpolate(flat) half_size: vec2<f32>,
    @location(2) @interpolate(flat) radii: vec4<f32>,
    @location(3) @interpolate(flat) color: vec4<f32>,
    @location(4) @interpolate(flat) border_color: vec4<f32>,
    @location(5) @interpolate(flat) border_width: f32,
    @location(6) @interpolate(flat) shape: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32, instance: Instance) -> VertexOutput {
    // Pad by one physical pixel so the antialiased edge is not cut off.
    let pad = 1.0 / globals.scale;
    let origin = instance.rect.xy - vec2<f32>(pad);
    let size = instance.rect.zw + vec2<f32>(2.0 * pad);
    let p = origin + strip_corner(vertex_index) * size;

    let half_size = instance.rect.zw * 0.5;
    let max_radius = min(half_size.x, half_size.y);

    var out: VertexOutput;
    out.position = to_clip(p);
    out.local = p - (instance.rect.xy + half_size);
    out.half_size = half_size;
    out.shape = instance.shape;
    if instance.shape.x > 3.5 {
        out.radii = instance.radii;
    } else if instance.shape.x > 0.5 && instance.shape.x < 1.5 {
        // Line end points, relative to the center like `local`.
        let center = instance.rect.xy + half_size;
        out.radii = vec4<f32>(instance.radii.xy - center, instance.radii.zw - center);
    } else {
        out.radii = clamp(instance.radii, vec4<f32>(0.0), vec4<f32>(max_radius));
    }
    if instance.shape.x > 2.5 && instance.shape.x < 3.5 {
        // Gradient end points stay sRGB; the fragment interpolates, then converts.
        out.color = instance.color;
        out.border_color = instance.border_color;
    } else {
        out.color = output_color(instance.color);
        out.border_color = output_color(instance.border_color);
    }
    out.border_width = instance.border_width;
    return out;
}

// Signed distance to a box with per-corner radii (negative inside).
fn rounded_box_sdf(p: vec2<f32>, half_size: vec2<f32>, radii: vec4<f32>) -> f32 {
    let top = select(radii.x, radii.y, p.x > 0.0);
    let bottom = select(radii.w, radii.z, p.x > 0.0);
    let r = select(top, bottom, p.y > 0.0);
    let q = abs(p) - half_size + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

// Signed distance to the segment a-b (zero on the segment).
fn segment_sdf(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let pa = p - a;
    let ba = b - a;
    let h = clamp(dot(pa, ba) / max(dot(ba, ba), 1e-6), 0.0, 1.0);
    return length(pa - ba * h);
}

// Signed distance to the triangle p0-p1-p2 (negative inside).
fn triangle_sdf(p: vec2<f32>, p0: vec2<f32>, p1: vec2<f32>, p2: vec2<f32>) -> f32 {
    let e0 = p1 - p0;
    let e1 = p2 - p1;
    let e2 = p0 - p2;
    let v0 = p - p0;
    let v1 = p - p1;
    let v2 = p - p2;
    let pq0 = v0 - e0 * clamp(dot(v0, e0) / dot(e0, e0), 0.0, 1.0);
    let pq1 = v1 - e1 * clamp(dot(v1, e1) / dot(e1, e1), 0.0, 1.0);
    let pq2 = v2 - e2 * clamp(dot(v2, e2) / dot(e2, e2), 0.0, 1.0);
    let s = sign(e0.x * e2.y - e0.y * e2.x);
    let d = min(
        min(
            vec2<f32>(dot(pq0, pq0), s * (v0.x * e0.y - v0.y * e0.x)),
            vec2<f32>(dot(pq1, pq1), s * (v1.x * e1.y - v1.y * e1.x)),
        ),
        vec2<f32>(dot(pq2, pq2), s * (v2.x * e2.y - v2.y * e2.x)),
    );
    return -sqrt(d.x) * sign(d.y);
}

// Signed distance to a five-pointed star of outer radius `r` and inner ratio `rf`,
// point up (Inigo Quilez, with y pointing down).
fn star_sdf(p_in: vec2<f32>, r: f32, rf: f32) -> f32 {
    let k1 = vec2<f32>(0.809016994375, -0.587785252292);
    let k2 = vec2<f32>(-k1.x, k1.y);
    var p = vec2<f32>(abs(p_in.x), -p_in.y);
    p -= 2.0 * max(dot(k1, p), 0.0) * k1;
    p -= 2.0 * max(dot(k2, p), 0.0) * k2;
    p.x = abs(p.x);
    p.y -= r;
    let ba = rf * vec2<f32>(-k1.y, k1.x) - vec2<f32>(0.0, 1.0);
    let h = clamp(dot(p, ba) / dot(ba, ba), 0.0, r);
    return length(p - ba * h) * sign(p.y * ba.x - p.x * ba.y);
}

// Distance to a round-capped arc of radius `r` from 12 o'clock clockwise over
// `sweep` of a turn (zero on the arc's center line).
fn arc_sdf(p: vec2<f32>, r: f32, sweep: f32) -> f32 {
    let tau = 6.283185307;
    let angle = atan2(p.x, -p.y);
    let a = select(angle, angle + tau, angle < 0.0);
    let end_angle = sweep * tau;
    if a <= end_angle {
        return abs(length(p) - r);
    }
    let start = vec2<f32>(0.0, -r);
    let end = vec2<f32>(r * sin(end_angle), -r * cos(end_angle));
    return min(length(p - start), length(p - end));
}

// A triangle filling the box `h` (half size), apex toward `direction`
// (0 up, 1 down, 2 left, 3 right).
fn pointing_triangle_sdf(p: vec2<f32>, h: vec2<f32>, direction: f32) -> f32 {
    if direction < 0.5 {
        return triangle_sdf(p, vec2<f32>(0.0, -h.y), vec2<f32>(h.x, h.y), vec2<f32>(-h.x, h.y));
    } else if direction < 1.5 {
        return triangle_sdf(p, vec2<f32>(0.0, h.y), vec2<f32>(-h.x, -h.y), vec2<f32>(h.x, -h.y));
    } else if direction < 2.5 {
        return triangle_sdf(p, vec2<f32>(-h.x, 0.0), vec2<f32>(h.x, -h.y), vec2<f32>(h.x, h.y));
    }
    return triangle_sdf(p, vec2<f32>(h.x, 0.0), vec2<f32>(-h.x, h.y), vec2<f32>(-h.x, -h.y));
}

// The error function, to within 5e-4 (Abramowitz and Stegun 7.1.27).
fn erf2(x: vec2<f32>) -> vec2<f32> {
    let s = sign(x);
    let a = abs(x);
    var r = 1.0 + (0.278393 + (0.230389 + 0.078108 * (a * a)) * a) * a;
    r = r * r;
    return s - s / (r * r);
}

fn gaussian(x: f32, sigma: f32) -> f32 {
    return exp(-(x * x) / (2.0 * sigma * sigma)) / (2.50662827 * sigma);
}

// How far the row `y` of a box (half size `h`) reaches from its center toward a side
// whose corner has radius `r`.
fn shadow_reach(r: f32, y: f32, h: vec2<f32>) -> f32 {
    let delta = min(h.y - r - abs(y), 0.0);
    return h.x - r + sqrt(max(0.0, r * r - delta * delta));
}

// How much of the row `y` of a rounded box (half size `h`, radii as `radii`) blurred
// horizontally by `sigma` covers `x`. The row's corners are the top ones above the
// center and the bottom ones below, left and right each its own, so the shape is the
// same whichever quadrant the fragment is in.
fn shadow_row(x: f32, y: f32, sigma: f32, radii: vec4<f32>, h: vec2<f32>) -> f32 {
    let left = select(radii.x, radii.w, y >= 0.0);
    let right = select(radii.y, radii.z, y >= 0.0);
    let k = 0.70710678 / sigma;
    let integral = 0.5 + 0.5 * erf2(vec2<f32>(x - shadow_reach(right, y, h), x + shadow_reach(left, y, h)) * k);
    return integral.y - integral.x;
}

// Coverage of a rounded box blurred by a Gaussian of `sigma`: exact across the rows,
// sampled four times down the column (Evan Wallace, "Fast Rounded Rectangle Shadows").
fn box_shadow(p: vec2<f32>, h: vec2<f32>, radii: vec4<f32>, sigma: f32) -> f32 {
    let start = clamp(-3.0 * sigma, p.y - h.y, p.y + h.y);
    let end = clamp(3.0 * sigma, p.y - h.y, p.y + h.y);
    let step = (end - start) / 4.0;
    var y = start + step * 0.5;
    var value = 0.0;
    for (var i = 0; i < 4; i++) {
        value += shadow_row(p.x, p.y - y, sigma, radii, h) * gaussian(y, sigma) * step;
        y += step;
    }
    return value;
}

// The shadow's quad reaches 1.5 * blur past its box; `shape.y` is the blur.
fn shadow_coverage(in: VertexOutput) -> f32 {
    let blur = in.shape.y;
    let h = max(in.half_size - vec2<f32>(1.5 * blur), vec2<f32>(0.0));
    let radii = clamp(in.radii, vec4<f32>(0.0), vec4<f32>(min(h.x, h.y)));
    let sigma = blur * 0.5;
    if sigma * globals.scale < 0.25 {
        // Too little blur to sample: a sharp, antialiased edge.
        let d = rounded_box_sdf(in.local, h, radii) * globals.scale;
        return clamp(0.5 - d, 0.0, 1.0);
    }
    return clamp(box_shadow(in.local, h, radii, sigma), 0.0, 1.0);
}

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    if in.shape.x > 5.5 {
        return vec4<f32>(in.color.rgb, in.color.a * shadow_coverage(in));
    }
    // Distance in physical pixels drives a one-pixel antialiasing ramp.
    var d: f32;
    if in.shape.x > 4.5 {
        d = arc_sdf(in.local, in.radii.x, in.shape.y) - in.border_width;
    } else if in.shape.x > 3.5 {
        let r = min(in.half_size.x, in.half_size.y);
        d = star_sdf(in.local + vec2<f32>(0.0, -0.1 * r), r, 0.45);
        if in.border_width > 0.0 {
            d = abs(d + in.border_width) - in.border_width;
        }
    } else if in.shape.x > 2.5 {
        d = rounded_box_sdf(in.local, in.half_size, in.radii);
    } else if in.shape.x > 1.5 {
        d = pointing_triangle_sdf(in.local, in.half_size, in.shape.y);
    } else if in.shape.x > 0.5 {
        d = segment_sdf(in.local, in.radii.xy, in.radii.zw) - in.border_width;
    } else {
        d = rounded_box_sdf(in.local, in.half_size, in.radii);
    }
    d = d * globals.scale;
    let coverage = clamp(0.5 - d, 0.0, 1.0);

    var color = in.color;
    if in.shape.x > 2.5 && in.shape.x < 3.5 {
        let along = select(in.local.x / in.half_size.x, in.local.y / in.half_size.y, in.shape.y > 0.5);
        let t = clamp(along * 0.5 + 0.5, 0.0, 1.0);
        color = output_color(mix(in.color, in.border_color, t));
    }
    if in.shape.x < 0.5 && in.border_width > 0.0 {
        let fill = clamp(0.5 - (d + in.border_width * globals.scale), 0.0, 1.0);
        color = mix(in.border_color, in.color, fill);
    }
    return vec4<f32>(color.rgb, color.a * coverage);
}
