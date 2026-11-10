//! Plain-English "how to use" notes shown in the Inspector, one per kind of
//! node, plus the getting-started steps shown when nothing is selected.

use nodes::registry::{self, Category};

/// Steps shown in the Inspector when nothing is selected.
pub const GETTING_STARTED: &[(&str, &str)] = &[
    (
        "1. Pick the sound",
        "Left panel (Media), scroll down: Open song…, a live input, or Computer audio for \
         whatever your PC is playing.",
    ),
    (
        "2. Add a picture",
        "Drag a picture, video or camera from the Media panel into the graph below. Or \
         right-click the graph → Sources.",
    ),
    (
        "3. Show it",
        "Drag from its round plug on the right to Program Out's plug. The viewer shows what \
         goes into Program Out.",
    ),
    (
        "4. Do something to it",
        "Right-click the node: Add effect after, Add a layer on top, Mask this node. The same \
         buttons are here in the Inspector when a node is selected.",
    ),
    (
        "5. Make it follow the music",
        "Click a node, then click ~ next to any slider and pick what drives it (kick, bass, \
         the beat…). Or drag a blue wire from an Audio Signal onto the slider.",
    ),
];

/// The "How to use" note for a node, if it has one.
pub fn for_node(kind: &str, category: Category) -> Option<&'static str> {
    let shader = registry::shader_file(kind).unwrap_or("");
    Some(match kind {
        registry::IMAGE => {
            "Make it react to the music:\n\
             • Add effect after → Transform, then raise Kick punch: it jumps on every kick.\n\
             • Or click ~ next to Scale and pick Bass: it breathes with the bass.\n\
             • Add effect after → Glitch, Trails or Mirror to bend it.\n\
             Combine it:\n\
             • Add a layer on top → pick a blend mode in the Layer Mixer (Screen for glowing \
             things on black, Multiply for dark textures, Overlay to mix them in).\n\
             • Mask this node → Shape Mask with Pulse on kick: a window that pumps.\n\
             • A white logo on black? Add effect after → Luma Key drops the black."
        }
        registry::VIDEO => {
            "Playback: Loop, Ping-pong, Beat-locked (the clip fits N bars of the song) or \
             Scrub (click ~ next to Position and pick a signal to drive it).\n\
             Wire triggers into its plugs: Audio Trigger (Kick) → Jump for a new spot on every \
             kick, or → Stutter for a repeat.\n\
             Everything an Image can do works too: effects after, layers, masks."
        }
        registry::CAMERA => {
            "Pick the camera above. Its movement becomes the Motion signal: wave at it.\n\
             Green screen? Add effect after → Chroma Key, then Add a layer on top of a scene.\n\
             Mask this node → Motion Mask shows only what moves."
        }
        registry::LAYER_MIXER => {
            "Layer 1 is the bottom; each next layer sits on top with its own blend mode and \
             opacity. S solos a layer, M mutes it, ▲ ▼ reorder. To add a layer, wire into the \
             free plug, or select a layer's node and press L."
        }
        registry::AUDIO_SIGNAL => {
            "Pick a signal, then drag its blue plug onto any slider in the Inspector (or onto a \
             node in the graph). 1.0 means 'typical for this song'."
        }
        registry::PROGRAM_OUT => {
            "What the audience sees. Plug the last node of your chain into it."
        }
        _ if shader == "mask_shape.wgsl" || shader == "mask_gradient.wgsl" => {
            "Plug its picture into another node's green Mask plug: that node then shows only \
             where this is white. Easiest: right-click the node you want to mask → Mask this \
             node. Use Invert to swap inside and outside, Soft edge to blur the border."
        }
        _ if shader == "mask_luma_key.wgsl" || shader == "mask_chroma_key.wgsl" => {
            "Cut-out picture: use it as a layer (the keyed parts are see-through).\n\
             Black & white: use it in a green Mask plug of another node.\n\
             Start with Tolerance / Threshold, then Softness for smooth edges."
        }
        _ if shader == "mask_motion.wgsl" => {
            "Feed it a camera or video. White = moving. Plug it into a Mask plug so an effect \
             only happens where things move; raise Trail for longer streaks."
        }
        _ if shader == "mask_key_mixer.wgsl" => {
            "Plug two masks in (A and B) and combine them, e.g. a circle minus a smaller \
             circle = a ring, or a shape intersected with motion."
        }
        _ if shader == "fx_transform.wgsl" => {
            "Drag the handles in the viewer: the middle moves the picture, a corner scales              and turns it (hold Shift to only scale). Kick punch makes it jump on every kick;              Spin speed turns it with the music."
        }
        _ if shader == "fx_corner_pin.wgsl" => {
            "Drag the four corners in the viewer to pin the picture onto anything: a screen              in a photo, a wall, a tilted card. The middle handle moves all four. Outside the              corners is see-through, so put it on a layer (+ Layer on what's behind)."
        }
        _ if shader == "fx_merge.wgsl" => {
            "Background below, Foreground on top, blended with the mode. For more than two \
             pictures use a Layer Mixer (right-click → Add a layer on top)."
        }
        _ => match category {
            Category::Source => {
                "A scene made by a shader. Its sliders already follow the music; click ~ \
                 next to any of them to change what drives it."
            }
            Category::Effect => {
                "Wire a picture into its left plug and its right plug onward (or to Program \
                 Out). Click ~ next to a slider to make it follow the music. The green Mask \
                 plug limits the effect to an area."
            }
            Category::Signal => {
                "Makes a number. Drag its blue plug onto a slider in the Inspector, or onto a \
                 node, to drive it."
            }
            Category::Trigger => {
                "Fires an event (orange plug). Plug it into a trigger plug: Envelope, Hold, \
                 Counter, or a Video's Jump / Restart / Stutter."
            }
            _ => return None,
        },
    })
}
