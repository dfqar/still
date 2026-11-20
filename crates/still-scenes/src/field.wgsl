struct FieldUniforms {
    time: f32, seed: f32, density: f32, motion: f32,
    bend: f32, light: f32, line_count: u32, steps: u32,
    pointer: vec2<f32>, viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: FieldUniforms;
@group(0) @binding(1) var<storage, read_write> path_points: array<vec4<f32>>;

fn hash(n: f32) -> f32 { return fract(sin(n * 127.1 + u.seed * 413.7) * 43758.5453); }

fn direction(p: vec2<f32>) -> vec2<f32> {
    let phase = u.seed * 6.2831853;
    let drift = u.time * 0.075;
    // A spatial field: broad directed flow gradually becomes curled ribbons.
    // Paths are integrated from fixed seed points anew, with no particles or
    // persistent position/velocity simulation and no temporal history texture.
    let angle = 0.16 + phase * 0.12
        + u.bend * (1.4 * sin(p.y * 1.9 + phase + drift)
        + 0.72 * cos(p.x * 1.6 - phase * 0.7 - drift * 0.8)
        + 0.46 * sin((p.x + p.y) * 2.6 + drift * 0.6));
    let delta = p - u.pointer * vec2<f32>(u.viewport.x / max(u.viewport.y, 1.0), 1.0);
    let wake = exp(-dot(delta, delta) * 2.5) * 0.28 * u.bend;
    return normalize(vec2<f32>(cos(angle), sin(angle)) + vec2<f32>(-delta.y, delta.x) * wake);
}

@compute @workgroup_size(64)
fn integrate_lines(@builtin(global_invocation_id) gid: vec3<u32>) {
    let line = gid.x;
    if line >= u.line_count { return; }
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let f = f32(line);
    let x = mix(-aspect * 1.18, aspect * 0.78, hash(f + 1.7));
    let y = mix(-1.3, 1.3, hash(f + 129.1));
    var p = vec2<f32>(x, y);
    let length = mix(0.48, 1.8, hash(f + 291.3));
    let stride = length / f32(u.steps);
    let weight = mix(0.45, 1.0, hash(f + 72.3));
    for (var step = 0u; step <= 56u; step += 1u) {
        if step > u.steps { break; }
        let offset = line * 57u + step;
        path_points[offset] = vec4<f32>(p, weight, f32(step) / f32(u.steps));
        // Midpoint integration keeps curvature smooth even at the Eco budget.
        let first = direction(p);
        p += direction(p + first * stride * 0.5) * stride;
    }
}
