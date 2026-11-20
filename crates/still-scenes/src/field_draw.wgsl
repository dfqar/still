struct FieldUniforms {
    time: f32, seed: f32, density: f32, motion: f32,
    bend: f32, light: f32, line_count: u32, steps: u32,
    pointer: vec2<f32>, viewport: vec2<f32>,
};
@group(0) @binding(0) var<uniform> u: FieldUniforms;
@group(0) @binding(1) var<storage, read> path_points: array<vec4<f32>>;

struct LineOut {
    @builtin(position) position: vec4<f32>,
    @location(0) transverse: f32,
    @location(1) weight: f32,
    @location(2) along: f32,
};

@vertex fn line_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> LineOut {
    let line = instance / u.steps;
    let segment = instance % u.steps;
    let a = path_points[line * 57u + segment];
    let b = path_points[line * 57u + segment + 1u];
    let at_end = vertex == 1u || vertex == 2u || vertex == 4u;
    let side = select(-1.0, 1.0, vertex == 2u || vertex == 4u || vertex == 5u);
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let tangent = normalize(b.xy - a.xy + vec2<f32>(0.000001));
    let normal = vec2<f32>(-tangent.y, tangent.x);
    let half_width = (1.0 + a.z * 0.45) * 2.0 / max(u.viewport.y, 1.0);
    let point = select(a.xy, b.xy, at_end) + normal * side * half_width;
    var out: LineOut;
    out.position = vec4<f32>(point.x / aspect, point.y, 0.0, 1.0);
    out.transverse = side;
    out.weight = a.z;
    out.along = select(a.w, b.w, at_end);
    return out;
}

@fragment fn line_fragment(in: LineOut) -> @location(0) vec4<f32> {
    let edge = 1.0 - smoothstep(0.28, 1.0, abs(in.transverse));
    let ends = smoothstep(0.0, 0.12, in.along) * (1.0 - smoothstep(0.78, 1.0, in.along));
    let pulse = 0.82 + 0.18 * sin(in.along * 5.0 - u.time * 0.11 + u.seed * 6.28);
    let intensity = edge * ends * in.weight * pulse * mix(0.006, 0.019, u.light);
    let tint = mix(vec3<f32>(0.73, 0.81, 0.89), vec3<f32>(0.94, 0.84, 0.69), u.seed * 0.42);
    return vec4<f32>(tint * intensity, 1.0);
}
