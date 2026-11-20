struct GlassUniforms {
    time: f32, seed: f32, form: f32, motion: f32,
    refraction: f32, light: f32, steps: u32, padding: u32,
    pointer: vec2<f32>, viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: GlassUniforms;
@group(1) @binding(0) var backdrop: texture_2d<f32>;
@group(1) @binding(1) var backdrop_sampler: sampler;

struct FullOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn fullscreen_vertex(@builtin(vertex_index) i: u32) -> FullOut {
    let p = vec2<f32>(select(-1.0, 3.0, i == 1u), select(-1.0, 3.0, i == 2u));
    var out: FullOut;
    out.position = vec4<f32>(p, 0.0, 1.0);
    out.uv = p * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return out;
}

fn rotate(p: vec2<f32>, angle: f32) -> vec2<f32> {
    let c = cos(angle); let s = sin(angle);
    return vec2<f32>(c * p.x - s * p.y, s * p.x + c * p.y);
}

// A conservative distance estimate for a seeded, gently folded ellipsoid.
// The object is genuinely intersected in 3D; its normal and optical path
// come from this surface rather than a screen-space highlight mask.
fn shape(point: vec3<f32>) -> f32 {
    let phase = u.seed * 6.2831853;
    let angle = 0.18 * sin(u.time * 0.13 + phase) + u.pointer.x * 0.16;
    let xy = rotate(point.xy, -0.25 + phase * 0.08 + angle);
    let xz = rotate(vec2<f32>(xy.x, point.z), u.time * 0.09 + phase);
    let p = vec3<f32>(xz.x, xy.y, xz.y);
    let axes = mix(vec3<f32>(0.88, 1.02, 0.88), vec3<f32>(1.12, 0.78, 0.83), u.form);
    let q = p / axes;
    let fold = sin(q.y * 3.1 + phase) * sin(q.x * 2.2 - u.time * 0.17) * sin(q.z * 2.7 + phase * 0.7);
    return (length(q) - 1.0 - fold * u.form * 0.16) * 0.52;
}

fn normal_at(p: vec3<f32>) -> vec3<f32> {
    let e = 0.0015;
    return normalize(vec3<f32>(
        shape(p + vec3<f32>(e, 0.0, 0.0)) - shape(p - vec3<f32>(e, 0.0, 0.0)),
        shape(p + vec3<f32>(0.0, e, 0.0)) - shape(p - vec3<f32>(0.0, e, 0.0)),
        shape(p + vec3<f32>(0.0, 0.0, e)) - shape(p - vec3<f32>(0.0, 0.0, e)),
    ));
}

fn environment(direction: vec3<f32>) -> vec3<f32> {
    let key = pow(max(dot(direction, normalize(vec3<f32>(-0.58, 0.68, 0.46))), 0.0), 42.0);
    let strip = exp(-pow((direction.x - 0.58) * 15.0, 2.0)) * smoothstep(-0.6, 0.45, direction.y);
    let fill = pow(max(dot(direction, normalize(vec3<f32>(0.7, -0.12, -0.65))), 0.0), 12.0);
    return vec3<f32>(0.0007, 0.0009, 0.0011)
        + vec3<f32>(0.90, 0.96, 1.02) * key * u.light
        + vec3<f32>(0.32, 0.28, 0.22) * strip * u.light
        + vec3<f32>(0.018, 0.022, 0.027) * fill;
}

@fragment fn background_fragment(in: FullOut) -> @location(0) vec4<f32> {
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let p = (in.uv - vec2<f32>(0.5)) * vec2<f32>(aspect, 1.0);
    let phase = u.seed * 6.2831853;
    let diagonal = p.y + p.x * 0.22 + 0.045 * sin(u.time * 0.08 + phase);
    let band = exp(-pow((diagonal + 0.12) * 12.0, 2.0));
    let broad = exp(-dot(p - vec2<f32>(-0.3, -0.18), p - vec2<f32>(-0.3, -0.18)) * 4.5);
    let shadow = 1.0 - 0.55 * exp(-pow(p.x * 4.0, 2.0) - pow((p.y - 0.35) * 18.0, 2.0));
    let color = (vec3<f32>(0.00015, 0.00020, 0.00025) + vec3<f32>(0.0014, 0.0018, 0.0022) * broad
        + vec3<f32>(0.0060, 0.0052, 0.0042) * band * u.light) * shadow;
    return vec4<f32>(color, 1.0);
}

fn project_uv(p: vec3<f32>) -> vec2<f32> {
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let xy = p.xy * (2.7 / max(3.6 - p.z, 0.1));
    return vec2<f32>(0.5 + xy.x / (2.0 * aspect), 0.5 - xy.y * 0.5);
}

@fragment fn glass_fragment(in: FullOut) -> @location(0) vec4<f32> {
    let base = textureSampleLevel(backdrop, backdrop_sampler, in.uv, 0.0).rgb;
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let screen = (in.uv * 2.0 - vec2<f32>(1.0)) * vec2<f32>(aspect, -1.0);
    let origin = vec3<f32>(0.0, 0.0, 3.6);
    let ray = normalize(vec3<f32>(screen, -2.7));
    // Skip rays outside the object's bounding sphere before doing any march.
    let b = dot(origin, ray);
    let discriminant = b * b - dot(origin, origin) + 1.45 * 1.45;
    if discriminant < 0.0 { return vec4<f32>(base, 1.0); }
    var travel = max(0.0, -b - sqrt(discriminant));
    let far = -b + sqrt(discriminant);
    var hit = false;
    for (var step = 0u; step < 64u; step += 1u) {
        if step >= u.steps || travel > far { break; }
        let distance = shape(origin + ray * travel);
        if distance < 0.0012 { hit = true; break; }
        travel += max(distance, 0.0006);
    }
    if !hit { return vec4<f32>(base, 1.0); }
    let position = origin + ray * travel;
    let normal = normal_at(position);
    let ior = mix(1.12, 1.62, u.refraction);
    let inside = refract(ray, normal, 1.0 / ior);
    // Bracket the back face by the bounding sphere, then bisect the surface.
    // This bounded solve avoids abrupt optical seams when a second distance
    // march runs out of steps at grazing angles.
    let exit_b = dot(position, inside);
    var lower = 0.005;
    var upper = -exit_b + sqrt(max(exit_b * exit_b - dot(position, position) + 1.45 * 1.45, 0.0));
    for (var step = 0u; step < 12u; step += 1u) {
        let middle = (lower + upper) * 0.5;
        if shape(position + inside * middle) < 0.0 { lower = middle; }
        else { upper = middle; }
    }
    let thickness = (lower + upper) * 0.5;
    let exit_position = position + inside * thickness;
    let exit_normal = normal_at(exit_position);
    var transmitted = refract(inside, -exit_normal, ior);
    if dot(transmitted, transmitted) < 0.01 { transmitted = reflect(inside, -exit_normal); }
    let background_position = exit_position + transmitted * max((exit_position.z + 2.0) / max(-transmitted.z, 0.2), 0.0);
    let uv = clamp(project_uv(background_position), vec2<f32>(0.001), vec2<f32>(0.999));
    let dispersion = vec2<f32>(0.0015, -0.0005) * u.refraction * (1.0 - abs(dot(normal, ray)));
    let refracted = vec3<f32>(
        textureSampleLevel(backdrop, backdrop_sampler, uv + dispersion, 0.0).r,
        textureSampleLevel(backdrop, backdrop_sampler, uv, 0.0).g,
        textureSampleLevel(backdrop, backdrop_sampler, uv - dispersion, 0.0).b,
    );
    let f0 = pow((ior - 1.0) / (ior + 1.0), 2.0);
    let fresnel = f0 + (1.0 - f0) * pow(1.0 - max(dot(-ray, normal), 0.0), 5.0);
    let absorption = exp(-vec3<f32>(0.13, 0.08, 0.045) * thickness * mix(0.35, 1.8, u.refraction));
    let reflected = environment(reflect(ray, normal));
    let edge_light = pow(1.0 - max(dot(-ray, normal), 0.0), 3.0) * 0.011 * u.light;
    let color = refracted * absorption * (1.0 - fresnel) + reflected * fresnel
        + vec3<f32>(0.70, 0.79, 0.88) * edge_light;
    return vec4<f32>(max(color, vec3<f32>(0.0)), 1.0);
}
