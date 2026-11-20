struct Light {
    color_power: vec4<f32>,
    position_size: vec4<f32>,
    rotation: vec4<f32>,
};
struct Uniforms {
    lights: array<Light, 3>,
    material: vec4<f32>, // diffusion, frost, grain, ambient
    animation: vec4<f32>, // time, motion, seed, media energy
    viewport: vec4<f32>, // target width, height, sample count, brightness
};
@group(0) @binding(0) var<uniform> u: Uniforms;
@group(1) @binding(0) var illumination: texture_2d<f32>;
@group(1) @binding(1) var light_sampler: sampler;

struct FullOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn fullscreen_vertex(@builtin(vertex_index) i: u32) -> FullOut {
    let p = vec2<f32>(select(-1.0, 3.0, i == 1u), select(-1.0, 3.0, i == 2u));
    var out: FullOut;
    out.position = vec4(p, 0.0, 1.0);
    out.uv = p * vec2(0.5, -0.5) + vec2(0.5);
    return out;
}
fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}
fn noise(p: vec2<f32>) -> f32 {
    let cell = floor(p);
    let f = fract(p);
    let w = f * f * (vec2(3.0) - 2.0 * f);
    return mix(mix(hash(cell), hash(cell + vec2(1.0, 0.0)), w.x),
               mix(hash(cell + vec2(0.0, 1.0)), hash(cell + vec2(1.0)), w.x), w.y);
}

// Irradiance behind a flat sheet: three independently shaped area lights.
@fragment fn light_fragment(in: FullOut) -> @location(0) vec4<f32> {
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    var radiance = vec3(0.0);
    for (var i = 0u; i < 3u; i += 1u) {
        let light = u.lights[i];
        let phase = f32(i) * 2.13 + u.animation.z * 6.28;
        let t = u.animation.x * 0.15;
        let delta = (in.uv - light.position_size.xy) * vec2(aspect, 1.0);
        let angle = light.rotation.x;
        let p = vec2(cos(angle) * delta.x + sin(angle) * delta.y,
                    -sin(angle) * delta.x + cos(angle) * delta.y);
        let size = max(light.position_size.zw, vec2(0.025));
        let q = dot(p / size, p / size);
        let core = exp(-q * 2.7);
        let halo = exp(-q * 0.45) * 0.13;
        let breath = 1.0 + sin(t * 0.45 + phase) * u.animation.y * 0.06;
        radiance += light.color_power.rgb * light.color_power.a * (core + halo) * breath;
    }
    return vec4(radiance, 1.0);
}

@fragment fn frost_fragment(in: FullOut) -> @location(0) vec4<f32> {
    let aspect = u.viewport.x / max(u.viewport.y, 1.0);
    let physical = in.uv * vec2(aspect, 1.0);
    // Both surface grain and its refractive normal remain fixed on the sheet.
    let cloud = noise(physical * 19.0 + u.animation.z * 7.0);
    let micro = vec2(noise(physical * 145.0), noise(physical * 145.0 + vec2(38.2, 9.7))) - vec2(0.5);
    let distortion = micro * u.material.y * 0.012;
    let base_uv = in.uv + distortion / vec2(aspect, 1.0);
    let radius = (0.006 + u.material.x * 0.125) * (0.35 + u.material.y * 0.65);
    let count = u32(u.viewport.z);
    var sum = vec3(0.0);
    var weight = 0.0;
    // Deterministic disk quadrature approximates the diffusion lobe. Explicit
    // LOD keeps sampling valid in bounded dynamic loops on every quality tier.
    for (var i = 0u; i < count; i += 1u) {
        let f = (f32(i) + 0.5) / f32(count);
        let r = sqrt(f);
        let angle = f32(i) * 2.399963;
        let offset = vec2(cos(angle) / aspect, sin(angle)) * r * radius;
        let w = exp(-f * 2.2);
        sum += textureSampleLevel(illumination, light_sampler, base_uv + offset, 0.0).rgb * w;
        weight += w;
    }
    let diffuse = sum / max(weight, 0.001);
    let direct = textureSampleLevel(illumination, light_sampler, base_uv, 0.0).rgb;
    let light = mix(direct, diffuse, u.material.y);
    let transmission = 0.90 - u.material.y * 0.13;
    let texture = (cloud - 0.5) * 0.09 + (hash(floor(physical * 1650.0)) - 0.5) * 0.13;
    let surface = 1.0 + texture * u.material.z;
    let base = vec3(0.0028, 0.0032, 0.0042) * u.material.w;
    let media_gain = 1.0 + u.animation.w * 0.35;
    return vec4((base + light * transmission * surface * media_gain) * u.viewport.w, 1.0);
}
