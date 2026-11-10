/*{
  "name": "Switch",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Shows one of up to four pictures and changes between them with a transition: the heart of a VJ set. Wire a Counter (fed by a Beat Trigger: every 4, 8 or 16 beats) into Show, and it moves to the next picture on time. Transition time is in beats, so it stays in step with the music.",
  "images": [
    { "name": "a", "label": "Picture 1" },
    { "name": "b", "label": "Picture 2" },
    { "name": "c", "label": "Picture 3" },
    { "name": "d", "label": "Picture 4" }
  ],
  "inputs": [
    { "name": "show",       "label": "Show (0 = first, 1 = last)", "type": "float",  "min": 0, "max": 1, "default": 0 },
    { "name": "count",      "label": "Pictures used",              "type": "choice", "options": ["2", "3", "4"], "default": 2 },
    { "name": "transition", "label": "Transition",                 "type": "choice", "options": ["Cut", "Dissolve", "Wipe", "Slide", "Zoom through", "Iris"], "default": 1 },
    { "name": "beats",      "label": "Transition time (beats)",    "type": "float",  "min": 0.05, "max": 8, "default": 1 }
  ],
  "buffers": [ { "name": "A", "persistent": true } ],
  "passes": [ { "entry": "fs_state", "target": "A" }, { "entry": "fs_main" } ]
}*/

// Buffer A holds the switch's memory (the same in every pixel):
// r = picture now (0..3) / 4, g = picture before, b = transition progress
// 0..1, a = 1 once started.

fn picture(i: i32, uv: vec2<f32>) -> vec4<f32> {
    switch i {
        case 0: { return img_a(uv); }
        case 1: { return img_b(uv); }
        case 2: { return img_c(uv); }
        default: { return img_d(uv); }
    }
}

fn target_index() -> i32 {
    let n = f32(i32(params.count + 0.5) + 2);
    return i32(round(clamp(params.show, 0.0, 1.0) * (n - 1.0)));
}

@fragment
fn fs_state(in: VsOut) -> @location(0) vec4<f32> {
    let s = sample_a(vec2f(0.5));
    let want = target_index();
    if s.a < 0.5 {
        return vec4f(f32(want) / 4.0, f32(want) / 4.0, 1.0, 1.0);
    }
    let now = i32(round(s.r * 4.0));
    if want != now {
        return vec4f(f32(want) / 4.0, f32(now) / 4.0, 0.0, 1.0);
    }
    // Transition length in seconds from beats (120 BPM until the tempo is known).
    let bpm = select(120.0, audio.bpm, audio.bpm > 30.0);
    let secs = max(params.beats * 60.0 / bpm, 0.001);
    return vec4f(s.r, s.g, min(s.b + audio.delta / secs, 1.0), 1.0);
}

fn inside(uv: vec2<f32>) -> f32 {
    return step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let s = sample_a(vec2f(0.5));
    let now = i32(round(s.r * 4.0));
    let before = i32(round(s.g * 4.0));
    let mode = i32(params.transition + 0.5);
    let t = smoothstep(0.0, 1.0, s.b);
    let uv = in.uv;
    if mode == 0 || s.b >= 1.0 || now == before {
        return picture(now, uv);
    }
    switch mode {
        // Dissolve.
        case 1: {
            return mix(picture(before, uv), picture(now, uv), t);
        }
        // Wipe: a soft diagonal edge sweeps across.
        case 2: {
            let edge = (uv.x + (1.0 - uv.y) * 0.25) / 1.25;
            let k = smoothstep(t * 1.1 - 0.1, t * 1.1, edge);
            return mix(picture(now, uv), picture(before, uv), k);
        }
        // Slide: the new picture pushes the old one off to the left.
        case 3: {
            let old_uv = uv + vec2f(t, 0.0);
            let new_uv = uv + vec2f(t - 1.0, 0.0);
            return picture(before, old_uv) * inside(old_uv) + picture(now, new_uv) * inside(new_uv);
        }
        // Zoom through: the old picture rushes past, the new one grows in.
        case 4: {
            let c = (uv - 0.5) * vec2f(audio.aspect, 1.0);
            let old_uv = c / (1.0 + t * 3.0) / vec2f(audio.aspect, 1.0) + 0.5;
            let new_uv = c / (0.4 + 0.6 * t) / vec2f(audio.aspect, 1.0) + 0.5;
            let old = picture(before, old_uv) * (1.0 - t);
            let fresh = picture(now, new_uv) * inside(new_uv) * t;
            return old + fresh;
        }
        // Iris: a circle opens from the middle.
        default: {
            let c = (uv - 0.5) * vec2f(audio.aspect, 1.0);
            let r = t * 0.75 * max(audio.aspect, 1.0);
            let k = smoothstep(r, r - 0.02, length(c));
            return mix(picture(before, uv), picture(now, uv), k);
        }
    }
}
