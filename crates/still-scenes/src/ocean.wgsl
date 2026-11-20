struct OceanUniforms {
    time: f32,
    seed: f32,
    calm: f32,
    scale: f32,
    flow: f32,
    light: f32,
    grid_size: f32,
    wave_count: f32,
    pointer: vec2<f32>,
    viewport: vec2<f32>,
    padding: vec4<f32>,
};

@group(0) @binding(0) var<uniform> u: OceanUniforms;

struct WaveSample { height: f32, gradient: vec2<f32> };

fn wave_sample(position: vec2<f32>, footprint: f32) -> WaveSample {
    let amplitude = mix(0.24, 0.045, u.calm);
    let frequency = mix(0.72, 1.68, u.scale);
    let speed = mix(0.12, 0.42, u.flow);
    let seed_angle = u.seed * 6.2831853;
    var height = 0.0;
    var gradient = vec2<f32>(0.0);
    var weight = 1.0;
    for (var i = 0u; i < 6u; i += 1u) {
        if (f32(i) >= u.wave_count) { break; }
        let fi = f32(i);
        let angle = seed_angle + fi * 1.941 + sin(fi * 2.17 + u.seed) * 0.35;
        let direction = normalize(vec2<f32>(cos(angle), sin(angle)));
        let k = frequency * (0.68 + fi * 0.41);
        let phase = dot(position, direction) * k + u.time * speed * (0.72 + fi * 0.23);
        let filtering = exp(-0.5 * k * k * footprint * footprint);
        let component = sin(phase) * amplitude * weight * filtering;
        height += component;
        gradient += direction * cos(phase) * k * amplitude * weight * filtering;
        weight *= 0.58;
    }
    return WaveSample(height, gradient);
}

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) distance: f32,
};

@vertex
fn ocean_vertex(@builtin(vertex_index) vertex_index: u32) -> VertexOut {
    let grid = u32(u.grid_size + 0.5);
    let x = vertex_index % grid;
    let y = vertex_index / grid;
    let uv = vec2<f32>(f32(x), f32(y)) / f32(grid - 1u);
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    // A perspective-spaced grid allocates geometry near the viewer and spans
    // the complete frustum through the distant horizon.
    let z = 2.35 - 94.0 * uv.y * uv.y;
    let half_width = (2.7 - z) * aspect * 0.92 + 0.4;
    var world = vec3<f32>((uv.x * 2.0 - 1.0) * half_width, 0.0, z);
    let sample = wave_sample(world.xz, 0.0);
    // Long-distance displacement must converge toward a stable horizon. This
    // also prevents the coarse far rows of the perspective grid from reading
    // as a polygonal mountain silhouette.
    let horizon_attenuation = 1.0 - 0.96 * smoothstep(8.0, 72.0, -world.z);
    world.y = sample.height * horizon_attenuation;
    let normal = normalize(vec3<f32>(-sample.gradient.x * horizon_attenuation, 1.0, -sample.gradient.y * horizon_attenuation));

    let camera = vec3<f32>(u.pointer.x * 0.16, 1.18 + u.pointer.y * 0.07, 2.7);
    let aim = vec3<f32>(u.pointer.x * 0.08, -0.03, -2.8);
    let forward = normalize(aim - camera);
    let right = normalize(cross(forward, vec3<f32>(0.0, 1.0, 0.0)));
    let up = cross(right, forward);
    let relative = world - camera;
    let view = vec3<f32>(dot(relative, right), dot(relative, up), dot(relative, forward));
    let near = 0.08;
    let far = 140.0;
    let focal = 1.62;

    var out: VertexOut;
    out.position = vec4<f32>(view.x * focal / aspect, view.y * focal, view.z * far / (far - near) - near * far / (far - near), view.z);
    out.world = world;
    out.normal = normal;
    out.distance = view.z;
    return out;
}

@fragment
fn ocean_fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let camera = vec3<f32>(u.pointer.x * 0.16, 1.18 + u.pointer.y * 0.07, 2.7);
    let view_direction = normalize(camera - in.world);
    // Resolve the analytic slope per pixel; interpolation exposed distant mesh
    // facets in the choppier presets. Filter frequencies below a pixel's size
    // to avoid shimmer without introducing temporal history.
    let footprint = max(length(dpdx(in.world.xz)), length(dpdy(in.world.xz)));
    let sample = wave_sample(in.world.xz, footprint);
    let horizon_t = clamp((-in.world.z - 8.0) / 64.0, 0.0, 1.0);
    let attenuation = 1.0 - 0.96 * horizon_t * horizon_t * (3.0 - 2.0 * horizon_t);
    let slope_z = sample.gradient.y * attenuation + sample.height * 0.96 * 6.0 * horizon_t * (1.0 - horizon_t) / 64.0;
    let normal = normalize(vec3<f32>(-sample.gradient.x * attenuation, 1.0, -slope_z));
    let light_direction = normalize(vec3<f32>(-0.24 + u.pointer.x * 0.08, 0.40, -0.86));
    let reflected = reflect(-light_direction, normal);
    let fresnel = pow(1.0 - max(dot(normal, view_direction), 0.0), 4.2);
    let roughness = mix(0.34, 0.12, u.calm) + (1.0 - u.light) * 0.08;
    let specular_power = mix(34.0, 118.0, 1.0 - roughness);
    let specular = pow(max(dot(reflected, view_direction), 0.0), specular_power);
    let broad_glint = pow(max(dot(reflected, view_direction), 0.0), 8.0) * 0.055;
    let distance_haze = smoothstep(3.0, 42.0, in.distance);
    let deep = vec3<f32>(0.00040, 0.00070, 0.00110);
    let grazing = vec3<f32>(0.00220, 0.00340, 0.00460);
    var color = mix(deep, grazing, fresnel * 0.78 + distance_haze * 0.18);
    color += vec3<f32>(0.12, 0.14, 0.15) * (specular * mix(0.08, 0.34, u.light) + broad_glint * u.light);
    color += vec3<f32>(0.00110, 0.00170, 0.00230) * distance_haze;
    color = mix(color, vec3<f32>(0.00015, 0.00024, 0.00036), smoothstep(45.0, 90.0, in.distance));
    let sky = ocean_sky_color(1.0 - in.position.y / u.viewport.y * 2.0);
    color = mix(color, sky, smoothstep(45.0, 135.0, in.distance));
    return vec4<f32>(color, 1.0);
}

// Ocean's background is drawn by its own pipeline into the same color/depth
// output. Surface haze converges to this value at the true distant horizon.
fn ocean_sky_color(screen_y: f32) -> vec3<f32> {
    let horizon = exp(-abs(screen_y - 0.34) * 5.0);
    return vec3<f32>(0.00002, 0.000035, 0.000055)
        + vec3<f32>(0.00024, 0.00030, 0.00038) * horizon * (0.35 + u.light);
}

struct SkyOut {
    @builtin(position) position: vec4<f32>,
    @location(0) screen_y: f32,
};

@vertex
fn ocean_sky_vertex(@builtin(vertex_index) index: u32) -> SkyOut {
    let point = vec2<f32>(select(-1.0, 3.0, index == 1u), select(-1.0, 3.0, index == 2u));
    return SkyOut(vec4<f32>(point, 1.0, 1.0), point.y);
}

@fragment
fn ocean_sky_fragment(in: SkyOut) -> @location(0) vec4<f32> {
    return vec4<f32>(ocean_sky_color(in.screen_y), 1.0);
}
