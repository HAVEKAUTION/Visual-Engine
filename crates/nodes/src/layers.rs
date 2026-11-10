//! Layer Mixer editing: moving a layer up or down swaps everything about
//! two layer slots (the picture wired in, blend mode, opacity, solo, mute,
//! their modulations and any Signal wires into them), as one undo step.

use graph::{Command, Endpoint, Graph, Link, NodeId};

use crate::registry::{layer_param, layer_port};

/// The settings each layer slot has.
pub const LAYER_SETTINGS: [&str; 4] = ["mode", "opacity", "solo", "mute"];

/// Commands that swap layer slots `a` and `b` of the Layer Mixer `node`
/// (run them as one [`Command::Batch`]).
pub fn swap_layers(g: &Graph, node: NodeId, a: usize, b: usize) -> Vec<Command> {
    let mut cmds = Vec::new();
    let Some(n) = g.node(node) else { return cmds };
    // Wires: take all four kinds off first, then put them back swapped.
    let mut put = Vec::new();
    let pa = layer_port(a);
    let pb = layer_port(b);
    for (from, to) in [(&pa, &pb), (&pb, &pa)] {
        if let Some(l) = g.input_link(node, from) {
            cmds.push(Command::Disconnect(l.clone()));
            put.push(Link {
                to: Endpoint::input(node, to),
                ..l.clone()
            });
        }
    }
    for what in LAYER_SETTINGS {
        let (na, nb) = (layer_param(what, a), layer_param(what, b));
        let (Some(xa), Some(xb)) = (n.param(&na), n.param(&nb)) else {
            continue;
        };
        for (name, from) in [(&na, xb), (&nb, xa)] {
            cmds.push(Command::SetParam {
                node,
                param: name.clone(),
                value: from.value.clone(),
            });
            cmds.push(Command::SetMods {
                node,
                param: name.clone(),
                mods: from.mods.clone(),
            });
        }
        for (from, to) in [(&na, &nb), (&nb, &na)] {
            for l in g.param_links(node, from) {
                cmds.push(Command::Disconnect(l.clone()));
                put.push(Link {
                    to: Endpoint::param(node, to),
                    ..l.clone()
                });
            }
        }
    }
    cmds.extend(put.into_iter().map(Command::Connect));
    cmds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{self, IMAGE_OUT, LAYER_MIXER, VALUE_OUT};
    use graph::{History, OutputRef, ParamValue};

    #[test]
    fn swapping_layers_moves_pictures_settings_and_wires_and_undoes() {
        let lib = scenes::SceneLibrary::new(None);
        let mut g = Graph::new();
        let add = |g: &mut Graph, kind: &str| {
            let n = g.new_node(&registry::spec_for(kind, &lib).unwrap());
            let id = n.id;
            g.apply(Command::AddNode(Box::new(n))).unwrap();
            id
        };
        let a = add(&mut g, registry::IMAGE);
        let b = add(&mut g, registry::IMAGE);
        let lfo = add(&mut g, registry::LFO);
        let mix = add(&mut g, LAYER_MIXER);
        let wire = |from: NodeId, port: &str, to: Endpoint| {
            Command::Connect(Link::new(OutputRef::new(from, port), to))
        };
        for c in [
            wire(a, IMAGE_OUT, Endpoint::input(mix, &layer_port(1))),
            wire(b, IMAGE_OUT, Endpoint::input(mix, &layer_port(2))),
            wire(
                lfo,
                VALUE_OUT,
                Endpoint::param(mix, &layer_param("opacity", 2)),
            ),
            Command::SetParam {
                node: mix,
                param: layer_param("mode", 2),
                value: ParamValue::Choice(12),
            },
        ] {
            g.apply(c).unwrap();
        }
        let before = g.to_ron();
        let mut h = History::new();
        let swap = swap_layers(&g, mix, 1, 2);
        h.exec(&mut g, Command::Batch(swap)).unwrap();
        let from = |port: &str| g.input_link(mix, port).map(|l| l.from.node);
        assert_eq!(from(&layer_port(1)), Some(b));
        assert_eq!(from(&layer_port(2)), Some(a));
        let n = g.node(mix).unwrap();
        assert_eq!(
            n.param(&layer_param("mode", 1)).unwrap().value,
            ParamValue::Choice(12)
        );
        assert_eq!(g.param_links(mix, &layer_param("opacity", 1)).count(), 1);
        assert_eq!(g.param_links(mix, &layer_param("opacity", 2)).count(), 0);
        h.undo(&mut g).unwrap();
        assert_eq!(g.to_ron(), before);
    }
}
