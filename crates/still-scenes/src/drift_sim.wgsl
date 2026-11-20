struct Particle {
    position: vec2<f32>,
    previous_position: vec2<f32>,
    velocity: vec2<f32>,
    normalized_age: f32,
    seed: u32,
};

struct Uniforms {
    dt: f32, time: f32, active_count: u32, seed: u32,
    speed: f32, flow: f32, light: f32, interpolation: f32,
    pointer_position: vec2<f32>, pointer_velocity: vec2<f32>,
    viewport: vec2<f32>, trail_decay: f32, noise_octaves: u32,
};

@group(0) @binding(0) var<storage, read_write> particles: array<Particle>;
@group(0) @binding(1) var<uniform> u: Uniforms;

fn hash(x0: u32) -> u32 {
    var x = x0;
    x = x ^ (x >> 16u); x = x * 0x7feb352du;
    x = x ^ (x >> 15u); x = x * 0x846ca68bu;
    return x ^ (x >> 16u);
}
fn rand(x: u32) -> f32 { return f32(hash(x)) / 4294967295.0; }

fn gradient(cell: vec3<i32>) -> vec3<f32> {
    let h = hash(bitcast<u32>(cell.x) ^ hash(bitcast<u32>(cell.y)) ^ hash(bitcast<u32>(cell.z)) ^ u.seed);
    let z = rand(h) * 2.0 - 1.0;
    let a = rand(h ^ 0x9e3779b9u) * 6.2831853;
    let r = sqrt(max(0.0, 1.0 - z * z));
    return vec3<f32>(r * cos(a), r * sin(a), z);
}
fn fade(t: vec3<f32>) -> vec3<f32> { return t * t * t * (t * (t * 6.0 - vec3<f32>(15.0)) + vec3<f32>(10.0)); }
fn noise3(p: vec3<f32>) -> f32 {
    let i = vec3<i32>(floor(p)); let f = fract(p); let w = fade(f);
    var n = 0.0;
    for (var z = 0i; z <= 1i; z++) { for (var y = 0i; y <= 1i; y++) { for (var x = 0i; x <= 1i; x++) {
        let o = vec3<i32>(x, y, z); let d = f - vec3<f32>(o);
        let weight = mix(1.0 - w.x, w.x, f32(x)) * mix(1.0 - w.y, w.y, f32(y)) * mix(1.0 - w.z, w.z, f32(z));
        n += dot(gradient(i + o), d) * weight;
    } } }
    return n;
}
fn potential(p0: vec2<f32>) -> f32 {
    var p = vec3<f32>(p0 * 1.35, u.time * 0.045);
    var amplitude = 0.5; var total = 0.0;
    for (var octave = 0u; octave < 3u; octave++) {
        if (octave >= u.noise_octaves) { break; }
        total += noise3(p) * amplitude; p *= 2.0; amplitude *= 0.5;
    }
    return total;
}
fn curl(p: vec2<f32>) -> vec2<f32> {
    let e = 0.018;
    let dx = (potential(p + vec2<f32>(e, 0.0)) - potential(p - vec2<f32>(e, 0.0))) / (2.0 * e);
    let dy = (potential(p + vec2<f32>(0.0, e)) - potential(p - vec2<f32>(0.0, e))) / (2.0 * e);
    return vec2<f32>(dy, -dx);
}
fn spawn(index: u32, cycle: u32) -> Particle {
    let s = hash(index ^ u.seed ^ hash(cycle));
    let angle = rand(s) * 6.2831853; let radius = sqrt(rand(s ^ 0x68bc21ebu)) * 1.08;
    let pos = vec2<f32>(cos(angle), sin(angle)) * radius;
    return Particle(pos, pos, vec2<f32>(0.0), rand(s ^ 0x02e5be93u) * 0.25, s);
}

@compute @workgroup_size(256)
fn simulate(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x; if (i >= u.active_count) { return; }
    var p = particles[i];
    if (p.seed == 0u || p.normalized_age >= 1.0 || any(abs(p.position) > vec2<f32>(1.25))) { p = spawn(i, p.seed + 1u); }
    p.previous_position = p.position;
    var force = curl(p.position) * (0.55 * u.flow) + vec2<f32>(0.032, -0.008);
    let delta = p.position - u.pointer_position; let d2 = dot(delta, delta);
    let pointer_speed = min(length(u.pointer_velocity), 2.0);
    let wake = exp(-d2 * 18.0) * pointer_speed;
    let tangent = normalize(vec2<f32>(-delta.y, delta.x) + vec2<f32>(0.0001));
    force += (tangent * 0.8 + normalize(u.pointer_velocity + vec2<f32>(0.0001)) * 0.35) * wake;
    p.velocity = p.velocity * exp(-u.dt * 1.9) + force * u.dt;
    let speed_limit = 0.42; let speed = length(p.velocity);
    if (speed > speed_limit) { p.velocity *= speed_limit / speed; }
    p.position += p.velocity * u.dt * u.speed;
    p.normalized_age += u.dt * mix(0.025, 0.075, rand(p.seed));
    particles[i] = p;
}
