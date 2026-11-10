/*{
  "name": "Switch",
  "kind": "effect",
  "author": "Visual Engine",
  "description": "Shows one of up to four pictures and changes between them with a transition: the heart of a VJ set. Anything wired into Show moves it on: a Counter fed by a Beat Trigger (every 8 or 16 beats), a section change or a drop. Order Random never shows the same picture twice in a row, and Mix it up picks a different transition each time, so a set doesn't feel like a loop.",
  "images": [
    { "name": "a", "label": "Picture 1" },
    { "name": "b", "label": "Picture 2" },
    { "name": "c", "label": "Picture 3" },
    { "name": "d", "label": "Picture 4" }
  ],
  "inputs": [
    { "name": "show",       "label": "Show (0 = first, 1 = last)", "type": "float",  "min": 0, "max": 1, "default": 0 },
    { "name": "count",      "label": "Pictures used",              "type": "choice", "options": ["2", "3", "4"], "default": 2 },
    { "name": "order",      "label": "Order",                      "type": "choice", "options": ["In turn", "Random (never the same twice)"], "default": 0 },
    { "name": "transition", "label": "Transition",                 "type": "choice", "options": ["Cut", "Dissolve", "Wipe", "Slide", "Zoom through", "Iris", "Mix it up (a different one each time)"], "default": 1 },
    { "name": "beats",      "label": "Transition time (beats)",    "type": "float",  "min": 0.05, "max": 8, "default": 1 },
    { "name": "_now",       "type": "float", "min": 0, "max": 3, "default": 0 },
    { "name": "_before",    "type": "float", "min": 0, "max": 3, "default": 0 },
    { "name": "_progress",  "type": "float", "min": 0, "max": 1, "default": 1 },
    { "name": "_mode",      "type": "float", "min": 0, "max": 5, "default": 0 }
  ]
}*/

// Which picture shows, the one before it, how far the transition has got
// and which transition it is are worked out by the engine (nodes::switch) and
// arrive as the _now, _before, _progress and _mode settings. The engine also
// skips drawing pictures that can't be seen, so four scenes in a Switch cost
// about as much as one or two.

fn picture(i: i32, uv: vec2<f32>) -> vec4<f32> {
    switch i {
        case 0: { return img_a(uv); }
        case 1: { return img_b(uv); }
        case 2: { return img_c(uv); }
        default: { return img_d(uv); }
    }
}

fn inside(uv: vec2<f32>) -> f32 {
    return step(0.0, uv.x) * step(uv.x, 1.0) * step(0.0, uv.y) * step(uv.y, 1.0);
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let now = i32(params._now + 0.5);
    let before = i32(params._before + 0.5);
    let mode = i32(params._mode + 0.5);
    let progress = params._progress;
    let t = smoothstep(0.0, 1.0, progress);
    let uv = in.uv;
    if mode == 0 || progress >= 1.0 || now == before {
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
