struct FogUniforms {
    time: f32,
    seed: f32,
    density: f32,
    drift: f32,
    depth: f32,
    light: f32,
    pointer: vec2<f32>,
    viewport: vec2<f32>,
    ray_steps: u32,
    noise_octaves: u32,
    padding: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: FogUniforms;

struct FullOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn fullscreen_vertex(@builtin(vertex_index) index: u32) -> FullOut {
    let point = vec2<f32>(
        select(-1.0, 3.0, index == 1u),
        select(-1.0, 3.0, index == 2u),
    );
    var out: FullOut;
    out.position = vec4<f32>(point, 0.0, 1.0);
    out.uv = point * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);
    return out;
}

fn hash31(point: vec3<f32>) -> f32 {
    var p = fract(point * 0.1031);
    p += dot(p, p.yzx + vec3<f32>(33.33));
    return fract((p.x + p.y) * p.z);
}

fn value_noise(point: vec3<f32>) -> f32 {
    let cell = floor(point);
    let local = fract(point);
    let curve = local * local * (vec3<f32>(3.0) - 2.0 * local);
    let x00 = mix(hash31(cell), hash31(cell + vec3<f32>(1.0, 0.0, 0.0)), curve.x);
    let x10 = mix(hash31(cell + vec3<f32>(0.0, 1.0, 0.0)), hash31(cell + vec3<f32>(1.0, 1.0, 0.0)), curve.x);
    let x01 = mix(hash31(cell + vec3<f32>(0.0, 0.0, 1.0)), hash31(cell + vec3<f32>(1.0, 0.0, 1.0)), curve.x);
    let x11 = mix(hash31(cell + vec3<f32>(0.0, 1.0, 1.0)), hash31(cell + vec3<f32>(1.0, 1.0, 1.0)), curve.x);
    return mix(mix(x00, x10, curve.y), mix(x01, x11, curve.y), curve.z);
}

fn fbm(point: vec3<f32>) -> f32 {
    var p = point;
    var amplitude = 0.55;
    var result = 0.0;
    var amplitude_sum = 0.0;
    for (var octave = 0u; octave < 4u; octave += 1u) {
        if (octave >= u.noise_octaves) { break; }
        result += value_noise(p) * amplitude;
        amplitude_sum += amplitude;
        p = p * 2.03 + vec3<f32>(17.1, 9.2, 13.7);
        amplitude *= 0.48;
    }
    return result * 0.94072 / max(amplitude_sum, 0.01);
}

fn fog_density(point: vec3<f32>, flow: vec2<f32>) -> f32 {
    let phase = vec3<f32>(flow * u.time * (0.035 + u.drift * 0.08), u.time * 0.018);
    let seed_offset = vec3<f32>(u.seed * 31.7, u.seed * 19.3, u.seed * 43.1);
    let base_point = point * mix(0.72, 1.42, u.depth) + phase + seed_offset;
    let warp = vec3<f32>(
        fbm(base_point * 0.43 + vec3<f32>(2.7, 0.0, 5.1)),
        fbm(base_point * 0.39 + vec3<f32>(0.0, 7.3, 1.8)),
        0.0,
    ) - vec3<f32>(0.5);
    let coherent_warp = vec3<f32>(warp.xy, mix(warp.x, warp.y, 0.43));
    let volume = fbm(base_point + coherent_warp * mix(1.15, 2.35, u.depth));
    let layer = 1.0 - smoothstep(0.55, 1.65, abs(point.y + sin(point.z * 0.32) * 0.18));
    // Keep empty pockets genuinely empty so the volume reads as suspended
    // layers instead of an evenly lit gray overlay.
    return smoothstep(0.50, 0.79, volume + u.density * 0.075) * layer;
}

@fragment
fn fog_fragment(in: FullOut) -> @location(0) vec4<f32> {
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let screen = (in.uv * 2.0 - vec2<f32>(1.0)) * vec2<f32>(aspect, 1.0);
    let origin = vec3<f32>(0.0, 0.08, -2.4);
    let ray = normalize(vec3<f32>(screen * vec2<f32>(0.72, 0.62), 1.45));
    let flow = normalize(vec2<f32>(0.84 + u.pointer.x * 0.12, 0.26 + u.pointer.y * 0.08));
    let step_size = mix(1.55, 2.2, u.depth) / f32(max(u.ray_steps, 1u));
    var distance = 0.25;
    var transmittance = 1.0;
    var color = vec3<f32>(0.0);
    let light_direction = normalize(vec3<f32>(-0.62 + u.pointer.x * 0.08, 0.46, -0.64));
    // Fog is authored as a low-luminance linear-light volume. The shared
    // compositor performs the final exposure and tone map; keeping the
    // scattering albedo restrained here prevents a thin mist from turning
    // into a flat white card after tone mapping.
    let cold = vec3<f32>(0.0050, 0.0070, 0.0090);
    let warm = vec3<f32>(0.0180, 0.0140, 0.0100);

    for (var step = 0u; step < 32u; step += 1u) {
        if (step >= u.ray_steps || transmittance < 0.012) { break; }
        let point = origin + ray * distance;
        let density = fog_density(point, flow) * mix(0.09, 0.46, u.density);
        // A single low-frequency lookup is enough for directional variation;
        // evaluating the whole warped density field a second time doubled the
        // cost of every ray step without a visible benefit.
        let toward_light = value_noise(point * 0.55 + light_direction * 0.8 + vec3<f32>(u.seed));
        let scattering = mix(0.28, 0.88, u.light) * mix(0.32, 1.34, 1.0 - toward_light);
        let height_tint = smoothstep(-0.8, 0.75, point.y);
        let tint = mix(cold, warm, height_tint * u.light * 0.42);
        // Integrate extinction over the travelled distance. The previous
        // version added raw density once per ray step, making brightness
        // depend on the quality tier and blowing out the Balanced/High image.
        let step_opacity = 1.0 - exp(-density * step_size * mix(1.2, 2.15, u.depth));
        color += transmittance * step_opacity * tint * scattering;
        transmittance *= 1.0 - step_opacity;
        distance += step_size;
    }

    let horizon = exp(-abs(screen.y + 0.08) * 2.6) * (0.001 + u.light * 0.003);
    color += vec3<f32>(0.038, 0.046, 0.052) * horizon * transmittance;
    // The compositor lifts deep shadows substantially; a display-space-sized
    // dither here becomes visible grain after that curve, so keep it tiny.
    let dither = (hash31(vec3<f32>(in.position.xy, u.seed)) - 0.5) / 32768.0;
    return vec4<f32>(max(color + vec3<f32>(dither), vec3<f32>(0.0)), 1.0);
}
