struct Particle { position: vec2<f32>, previous_position: vec2<f32>, velocity: vec2<f32>, normalized_age: f32, seed: u32 };
struct Uniforms {
    dt: f32, time: f32, active_count: u32, seed: u32,
    speed: f32, flow: f32, light: f32, interpolation: f32,
    pointer_position: vec2<f32>, pointer_velocity: vec2<f32>,
    viewport: vec2<f32>, trail_decay: f32, noise_octaves: u32,
};
@group(0) @binding(0) var<storage, read> particles: array<Particle>;
@group(0) @binding(1) var<uniform> u: Uniforms;

struct ParticleOut { @builtin(position) position: vec4<f32>, @location(0) local: vec2<f32>, @location(1) energy: f32 };
@vertex fn particle_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> ParticleOut {
    let p = particles[instance];
    let center = mix(p.previous_position, p.position, u.interpolation);
    let delta = p.position - p.previous_position;
    let direction = normalize(delta + vec2<f32>(0.00001, 0.0)); let normal = vec2(-direction.y, direction.x);
    let speed = min(length(p.velocity) * 3.0, 1.0);
    let half_width = mix(0.75, 1.25, speed) / u.viewport.y * 2.0;
    let half_length = mix(1.8, 7.0, speed) / u.viewport.y * 2.0;
    let q = vec2<f32>(select(-1.0, 1.0, vertex == 1u || vertex == 4u || vertex == 5u), select(-1.0, 1.0, (vertex >= 2u && vertex != 4u)));
    let aspect = u.viewport.y / u.viewport.x;
    var offset = direction * q.x * half_length + normal * q.y * half_width;
    offset.x *= aspect;
    var out: ParticleOut; out.position = vec4(center + offset, 0.0, 1.0); out.local = q; out.energy = (1.0 - smoothstep(0.72, 1.0, p.normalized_age)) * u.light; return out;
}
@fragment fn particle_fragment(in: ParticleOut) -> @location(0) vec4<f32> {
    let capsule = length(vec2(max(abs(in.local.x) - 0.55, 0.0), in.local.y));
    let alpha = (1.0 - smoothstep(0.62, 1.0, capsule)) * in.energy * 0.075;
    let color = mix(vec3(0.47, 0.53, 0.57), vec3(0.82, 0.78, 0.69), in.energy * 0.45);
    return vec4(color * alpha, alpha);
}

@group(0) @binding(0) var history_texture: texture_2d<f32>;
@group(0) @binding(1) var history_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Uniforms;
struct FullOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex fn fullscreen_vertex(@builtin(vertex_index) i: u32) -> FullOut {
    let p = vec2<f32>(select(-1.0, 3.0, i == 1u), select(-1.0, 3.0, i == 2u));
    var o: FullOut; o.position = vec4(p,0.0,1.0); o.uv = p*vec2(0.5,-0.5)+vec2(0.5); return o;
}
@fragment fn decay_fragment(in: FullOut) -> @location(0) vec4<f32> { return textureSample(history_texture, history_sampler, in.uv) * post.trail_decay; }
