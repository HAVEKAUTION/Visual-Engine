/*{
  "name": "Iron Furnace",
  "author": "Visual Engine",
  "style": "aggressive / industrial",
  "genres": ["Metal", "Hardcore", "Rock"],
  "palette": ["#060404", "#4a0c04", "#ff3c00", "#ffae1a", "#fff1d6"],
  "description": "Staring into a furnace through rings of spinning sawblades. The guitars (mids and upper mids) feed the molten core and spin the blades; every kick punches the view and throws a burst of sparks out of the fire (double-kick = a storm of them), every snare cracks the iron with lightning, and a drop blasts everything white-hot.",
  "inputs": [
    { "name": "heat",        "label": "Heat",              "type": "float", "min": 0.2,  "max": 2,    "default": 1.0 },
    { "name": "core_size",   "label": "Core size",         "type": "float", "min": 0.05, "max": 0.4,  "default": 0.15 },
    { "name": "ring_count",  "label": "Blade rings",       "type": "float", "min": 1,    "max": 6,    "default": 4 },
    { "name": "teeth",       "label": "Teeth",             "type": "float", "min": 4,    "max": 40,   "default": 14 },
    { "name": "tooth_depth", "label": "Tooth depth",       "type": "float", "min": 0,    "max": 1,    "default": 0.6 },
    { "name": "spin_speed",  "label": "Blade spin",        "type": "float", "min": 0,    "max": 1.5,  "default": 0.3 },
    { "name": "punch",       "label": "Kick punch",        "type": "float", "min": 0,    "max": 1,    "default": 0.5 },
    { "name": "sparks",      "label": "Sparks",            "type": "float", "min": 0,    "max": 1.5,  "default": 0.9 },
    { "name": "lightning",   "label": "Snare lightning",   "type": "float", "min": 0,    "max": 1.5,  "default": 1.0 },
    { "name": "smear",       "label": "Heat smear",        "type": "float", "min": 0,    "max": 0.9,  "default": 0.4 },
    { "name": "grain",       "label": "Grain",             "type": "float", "min": 0,    "max": 1,    "default": 0.3 }
  ],
  "clocks": [
    { "name": "spin",  "speed": "spin_speed", "signal": "high_mid_slow", "amount": 1.2 },
    { "name": "churn", "speed": 0.5,          "signal": "mid_slow",      "amount": 1.5 }
  ]
}*/

// Palette: 0 = iron / darkness, 1 = ember, 2 = flame, 3 = molten, 4 = white-hot.

// Heat ramp through palette colours 1..4 (0 = dull ember, 1 = white-hot).
fn heat_col(t: f32) -> vec3<f32> {
    let x = clamp(t, 0.0, 1.0) * 3.0;
    let i = i32(floor(min(x, 2.999)));
    return mix(pal_color(1 + i), pal_color(2 + i), x - f32(i));
}

// Angle difference wrapped to -PI..PI.
fn wrap_a(a: f32) -> f32 {
    return a - TAU * floor(a / TAU + 0.5);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let kick = env(audio.kick_age, 0.08);
    let snare = env(audio.snare_age, 0.1);
    let guitars = clamp(0.5 * (audio.mid_fast + audio.high_mid_fast), 0.0, 3.0);
    let heat = params.heat * (0.55 + 0.35 * guitars + 0.4 * audio.build_amount)
        + 1.5 * env(audio.drop_age, 0.7);

    // Kick: a short jolt and a punch towards the viewer.
    let kc = audio.kick_count;
    let jolt = vec2f(hash11(kc * 1.31) - 0.5, hash11(kc * 1.31 + 5.1) - 0.5);
    var xy = in.xy * (1.0 - 0.06 * kick * params.punch) + jolt * 0.04 * kick * params.punch;
    // Heat haze: a faint shimmer rising off the fire.
    xy += (vec2f(noise2(xy * 6.0 + vec2f(0.0, -params.churn * 3.0)),
                 noise2(xy * 6.0 + vec2f(5.2, -params.churn * 3.0))) - 0.5) * 0.008 * heat;

    let r = max(length(xy), 1e-3);
    let a = atan2(xy.y, xy.x);
    let dir = xy / r;
    let aa = fwidth(r) * 1.2;

    // --- The furnace: molten turbulence pouring outwards from the core ----
    let flow = fbm2(dir * 1.8 + vec2f(1.0, 0.7) * (log(r) * 2.5 - params.churn));
    let core_r = params.core_size * (0.75 + 0.25 * guitars + 0.35 * kick);
    let glow = exp(-max(r - core_r, 0.0) * 3.2 / (0.5 + 0.5 * heat));
    let t = glow * (0.45 + 0.75 * flow) * heat;
    var col = pal_color(0) + heat_col(t * 0.85) * min(t * 1.4, 1.8);

    // --- Sawblade rings ---------------------------------------------------
    let rings = i32(floor(params.ring_count));
    for (var i = 0; i < 6; i++) {
        if i >= rings {
            break;
        }
        let fi = f32(i);
        let r0 = (params.core_size + 0.08) * pow(1.6, fi) + 0.04;
        let w = r0 * 0.3;
        // Neighbouring rings spin in opposite directions.
        let sgn = select(-1.0, 1.0, (i & 1) == 0);
        let n_teeth = floor(params.teeth * (1.0 + 0.45 * fi));
        let u = (a / TAU + params.spin * sgn * (0.35 - 0.04 * fi)) * n_teeth;
        let tooth = 1.0 - fract(u);
        let r_in = r0 - params.tooth_depth * w * 0.9 * tooth * tooth;
        let r_out = r0 + w;
        let metal = smoothstep(r_in - aa, r_in + aa, r) * smoothstep(r_out + aa, r_out - aa, r);
        if metal <= 0.0 {
            continue;
        }

        // Brushed iron, each tooth a separate plate.
        let brushed = noise2(vec2f(fract(u) * 2.0 + floor(u) * 3.7, r * 90.0));
        var iron = mix(vec3f(0.05, 0.045, 0.045), vec3f(0.16, 0.15, 0.15), brushed);
        // Light from the furnace catches the inner (toothed) edge.
        let light_here = exp(-r_in * 2.2 / (0.5 + 0.5 * heat)) * heat;
        let rim = exp(-(r - r_in) / (w * 0.12));
        iron += heat_col(0.55 + 0.3 * kick) * rim * light_here * 1.6;
        // A cold highlight on the outer edge.
        iron += vec3f(0.2) * smoothstep(w * 0.12, 0.0, r_out - r) * 0.5;
        // Rivets, one per tooth.
        let arc = TAU * r / n_teeth;
        let dv = vec2f((fract(u) - 0.5) * arc, r - (r0 + w * 0.6));
        let rivet = smoothstep(w * 0.1, w * 0.06, length(dv));
        iron = mix(iron, vec3f(0.35) + heat_col(0.6) * light_here * 0.5, rivet);
        col = mix(col, iron, metal);
    }

    // --- Sparks: every kick throws a burst out of the fire ----------------
    let sectors = 56.0;
    let sec = floor((a / TAU + 0.5) * sectors);
    for (var l = 0; l < 3; l++) {
        let fl = f32(l);
        let h = hash21(vec2f(sec, kc * 3.1 + fl * 17.0));
        if h > params.sparks * 0.45 {
            continue;
        }
        let h2 = hash21(vec2f(sec + 13.0, kc + fl * 5.0));
        let age = audio.kick_age;
        let ac = ((sec + 0.15 + 0.7 * h2) / sectors - 0.5) * TAU + age * (h2 - 0.5) * 0.6;
        let rs = core_r + age * (1.2 + 2.2 * h2);
        let len = 0.03 + 0.12 * min(age * 4.0, 1.0);
        let across = abs(wrap_a(a - ac)) * r;
        let along = rs - r;
        let streak = smoothstep(0.006, 0.0, across) * step(0.0, along) * smoothstep(len, 0.0, along);
        col += heat_col(0.75 + 0.25 * h) * streak * env(age, 0.45 + 0.4 * h2) * 2.5;
    }
    // A few lazy embers drift out all the time.
    let eh = hash11(sec * 7.7);
    let ep = fract(audio.time * (0.1 + 0.15 * eh) + eh * 9.0);
    let ea = ((sec + 0.5) / sectors - 0.5) * TAU + sin(audio.time + eh * 20.0) * 0.03;
    let ember = smoothstep(0.008, 0.0, length(xy - vec2f(cos(ea), sin(ea)) * (core_r + ep * 2.0)));
    col += heat_col(0.6) * ember * (1.0 - ep) * params.sparks * step(0.6, eh);

    // --- Snare: lightning cracks across the iron --------------------------
    if snare > 0.01 && params.lightning > 0.0 {
        let sc = audio.snare_count;
        for (var k = 0; k < 3; k++) {
            let fk = f32(k);
            let ak = (hash11(sc * 7.3 + fk * 3.1) - 0.5) * TAU;
            let jag = (noise2(vec2f(r * 9.0, sc + fk * 11.0)) - 0.5) * 0.5
                + (noise2(vec2f(r * 31.0, sc * 2.0 + fk)) - 0.5) * 0.15;
            let d = abs(wrap_a(a - ak - jag)) * r;
            let bolt = smoothstep(0.005, 0.0, d) + exp(-d * 60.0) * 0.35;
            col += pal_color(4) * bolt * snare * params.lightning * smoothstep(core_r, core_r + 0.1, r) * 2.0;
        }
    }

    // --- Heat smear: last frame drifts outward and fades (sparks streak) ---
    let puv = 0.5 + (in.uv - 0.5) * 0.985;
    let old = prev(puv).rgb * pow(params.smear, audio.delta * 60.0);
    col = max(col, old);

    // Drop: white-hot blast.
    col += pal_color(4) * env(audio.drop_age, 0.5) * 0.6;

    // Grain and vignette.
    col += (hash21(in.uv * vec2f(audio.res_x, audio.res_y) + fract(audio.time) * 100.0) - 0.5) * 0.08 * params.grain;
    col *= 1.0 - 0.45 * pow(length(in.uv - 0.5) * 1.35, 2.0);
    return vec4f(max(col, vec3f(0.0)), 1.0);
}
