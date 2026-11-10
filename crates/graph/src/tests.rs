use signals::modulation::{ModSlot, Shape, Source, Wave};

use crate::*;

fn shader() -> NodeSpec {
    NodeSpec {
        kind: "shader".into(),
        label: "Shader".into(),
        inputs: vec![],
        outputs: vec![PortDef::new("out", PortType::Image)],
        params: vec![
            ParamDef::float("speed", 0.0, 2.0, 1.0),
            ParamDef::choice("mode", &["A", "B", "C"], 0),
        ],
    }
}

fn merge() -> NodeSpec {
    NodeSpec {
        kind: "merge".into(),
        label: "Merge".into(),
        inputs: vec![
            PortDef::new("base", PortType::Image),
            PortDef::new("top", PortType::Image),
        ],
        outputs: vec![PortDef::new("out", PortType::Image)],
        params: vec![ParamDef::float("opacity", 0.0, 1.0, 1.0)],
    }
}

fn lfo() -> NodeSpec {
    NodeSpec {
        kind: "lfo".into(),
        label: "LFO".into(),
        inputs: vec![],
        outputs: vec![PortDef::new("value", PortType::Signal)],
        params: vec![ParamDef::float("beats", 0.25, 32.0, 4.0)],
    }
}

fn kick() -> NodeSpec {
    NodeSpec {
        kind: "audio_trigger".into(),
        label: "Kick".into(),
        inputs: vec![],
        outputs: vec![PortDef::new("hit", PortType::Trigger)],
        params: vec![],
    }
}

fn program_out() -> NodeSpec {
    NodeSpec {
        kind: "program_out".into(),
        label: "Program Out".into(),
        inputs: vec![PortDef::new("image", PortType::Image)],
        outputs: vec![],
        params: vec![],
    }
}

fn add(g: &mut Graph, h: &mut History, spec: &NodeSpec) -> NodeId {
    let n = g.new_node(spec);
    let id = n.id;
    h.exec(g, Command::AddNode(Box::new(n))).unwrap();
    id
}

fn wire(from: NodeId, out: &str, to: NodeId, input: &str) -> Command {
    Command::Connect(Link::new(
        OutputRef::new(from, out),
        Endpoint::input(to, input),
    ))
}

fn to_param(from: NodeId, out: &str, to: NodeId, param: &str) -> Command {
    Command::Connect(Link::new(
        OutputRef::new(from, out),
        Endpoint::param(to, param),
    ))
}

/// Shader A + shader B → Merge → Out, with an LFO on A's speed.
fn sample(g: &mut Graph, h: &mut History) -> [NodeId; 5] {
    let a = add(g, h, &shader());
    let b = add(g, h, &shader());
    let m = add(g, h, &merge());
    let o = add(g, h, &program_out());
    let l = add(g, h, &lfo());
    h.exec(g, wire(a, "out", m, "base")).unwrap();
    h.exec(g, wire(b, "out", m, "top")).unwrap();
    h.exec(g, wire(m, "out", o, "image")).unwrap();
    h.exec(g, to_param(l, "value", a, "speed")).unwrap();
    [a, b, m, o, l]
}

#[test]
fn nodes_are_numbered_and_image_nodes_get_a_mask_input() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, m, o, l] = sample(&mut g, &mut h);
    assert_eq!([a.0, b.0, m.0, o.0, l.0], [1, 2, 3, 4, 5]);
    assert_eq!(a.to_string(), "01");
    let mask = g.node(m).unwrap().input("mask").unwrap();
    assert_eq!(mask.ty, PortType::Mask);
    assert!(mask.optional);
    // No picture out → no mask input.
    assert!(g.node(o).unwrap().input("mask").is_none());
    assert!(g.node(l).unwrap().input("mask").is_none());
    // Numbers are never reused, even after undo.
    h.undo(&mut g).unwrap();
    let next = g.new_node(&shader());
    assert_eq!(next.id.0, 6);
}

#[test]
fn type_mismatches_are_rejected() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, _, m, _, l] = sample(&mut g, &mut h);
    let k = add(&mut g, &mut h, &kick());
    let before = g.clone();
    let err = |r: Result<(), GraphError>| r.unwrap_err();
    // Signal into an Image input.
    assert_eq!(
        err(h.exec(&mut g, wire(l, "value", m, "top"))),
        GraphError::TypeMismatch {
            from: "Signal",
            to: "Image"
        }
    );
    // Trigger into an Image input.
    assert!(h.exec(&mut g, wire(k, "hit", m, "base")).is_err());
    // An Image into a parameter.
    assert_eq!(
        err(h.exec(&mut g, to_param(a, "out", m, "opacity"))),
        GraphError::NotASignal
    );
    // A Signal into a non-number parameter.
    assert_eq!(
        err(h.exec(&mut g, to_param(l, "value", a, "mode"))),
        GraphError::NotModulatable("mode".into())
    );
    // Unknown plugs.
    assert!(h.exec(&mut g, wire(a, "nope", m, "base")).is_err());
    assert!(h.exec(&mut g, wire(a, "out", m, "nope")).is_err());
    assert_eq!(g, before, "failed edits must not change the graph");
    // An Image works as a Mask.
    h.exec(&mut g, wire(a, "out", m, "mask")).unwrap();
}

#[test]
fn cycles_are_rejected() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, _, m, _, l] = sample(&mut g, &mut h);
    let m2 = add(&mut g, &mut h, &merge());
    h.exec(&mut g, wire(m, "out", m2, "base")).unwrap();
    // m2 → m would close m → m2 → m.
    assert_eq!(
        h.exec(&mut g, wire(m2, "out", m, "top")).unwrap_err(),
        GraphError::Cycle
    );
    // A node into itself.
    assert_eq!(
        h.exec(&mut g, wire(m, "out", m, "mask")).unwrap_err(),
        GraphError::Cycle
    );
    // Through a parameter wire: LFO → shader A → merge; LFO's own parameter
    // can't be driven by something downstream of it... only Signals can drive
    // parameters, so build a loop with a second LFO chain instead.
    let l2 = add(&mut g, &mut h, &lfo());
    h.exec(&mut g, to_param(l2, "value", l, "beats")).unwrap();
    assert_eq!(
        h.exec(&mut g, to_param(l, "value", l2, "beats"))
            .unwrap_err(),
        GraphError::Cycle
    );
    let _ = a;
}

#[test]
fn connecting_an_occupied_input_replaces_and_undo_restores() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, m, _, _] = sample(&mut g, &mut h);
    let before = g.clone();
    h.exec(&mut g, wire(a, "out", m, "top")).unwrap();
    assert_eq!(g.input_link(m, "top").unwrap().from.node, a);
    assert_eq!(g.links().len(), before.links().len());
    h.undo(&mut g).unwrap();
    assert_eq!(g.input_link(m, "top").unwrap().from.node, b);
    assert_eq!(g, before);
    // The same wire twice.
    assert_eq!(
        h.exec(&mut g, wire(b, "out", m, "top")).unwrap_err(),
        GraphError::AlreadyConnected
    );
}

#[test]
fn signal_wires_into_parameters_carry_the_mapping_rule() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, _, _, _, l] = sample(&mut g, &mut h);
    let link = g.param_links(a, "speed").next().unwrap().clone();
    assert_eq!(link.modulation, Some(WireMod::default()));
    let new = WireMod {
        shape: Shape::Curve { gamma: 2.0 },
        depth: -0.3,
        enabled: true,
    };
    h.exec(
        &mut g,
        Command::SetWireMod {
            from: OutputRef::new(l, "value"),
            to: Endpoint::param(a, "speed"),
            modulation: new,
        },
    )
    .unwrap();
    assert_eq!(
        g.param_links(a, "speed").next().unwrap().modulation,
        Some(new)
    );
    h.undo(&mut g).unwrap();
    assert_eq!(g.param_links(a, "speed").next().unwrap(), &link);
    // Several Signals can drive one parameter (they add up).
    let l2 = add(&mut g, &mut h, &lfo());
    h.exec(&mut g, to_param(l2, "value", a, "speed")).unwrap();
    assert_eq!(g.param_links(a, "speed").count(), 2);
}

#[test]
fn parameters_are_checked_and_clamped() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, ..] = sample(&mut g, &mut h);
    let set = |v: ParamValue, p: &str| Command::SetParam {
        node: a,
        param: p.into(),
        value: v,
    };
    h.exec(&mut g, set(ParamValue::Float(9.0), "speed"))
        .unwrap();
    assert_eq!(
        g.node(a).unwrap().param("speed").unwrap().value,
        ParamValue::Float(2.0)
    );
    h.exec(&mut g, set(ParamValue::Choice(7), "mode")).unwrap();
    assert_eq!(
        g.node(a).unwrap().param("mode").unwrap().value,
        ParamValue::Choice(2)
    );
    assert!(
        h.exec(&mut g, set(ParamValue::Bool(true), "speed"))
            .is_err()
    );
    let slot = ModSlot::new(
        Source::Lfo {
            wave: Wave::Sine,
            beats: 4.0,
        },
        Shape::Linear,
        0.2,
    );
    assert!(
        h.exec(
            &mut g,
            Command::SetMods {
                node: a,
                param: "mode".into(),
                mods: vec![slot.clone()]
            }
        )
        .is_err()
    );
    h.exec(
        &mut g,
        Command::SetMods {
            node: a,
            param: "speed".into(),
            mods: vec![slot],
        },
    )
    .unwrap();
}

#[test]
fn removing_a_node_takes_its_wires_and_group_place_and_undo_restores_it() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, m, _, _] = sample(&mut g, &mut h);
    let gid = g.new_group_id();
    h.exec(
        &mut g,
        Command::PutGroup(Group {
            id: gid,
            label: "Pair".into(),
            parent: None,
            nodes: vec![a, b],
            published: vec![PublishedParam {
                label: "Speed".into(),
                node: a,
                param: "speed".into(),
            }],
            collapsed: false,
        }),
    )
    .unwrap();
    let before = g.clone();
    h.exec(&mut g, Command::RemoveNode(a)).unwrap();
    assert!(g.node(a).is_none());
    assert!(
        g.links()
            .iter()
            .all(|l| l.from.node != a && l.to.node() != a)
    );
    assert!(g.input_link(m, "base").is_none());
    let grp = g.group(gid).unwrap();
    assert_eq!(grp.nodes, vec![b]);
    assert!(grp.published.is_empty());
    h.undo(&mut g).unwrap();
    assert_eq!(g, before);
}

#[test]
fn undo_redo_round_trips() {
    let mut g = Graph::new();
    let mut h = History::new();
    let empty = g.clone();
    let [a, b, m, _, _] = sample(&mut g, &mut h);
    h.exec(
        &mut g,
        Command::SetLabel {
            node: m,
            label: "Main mix".into(),
        },
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::Move {
            node: a,
            pos: [120.0, -40.0],
        },
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::SetBypass {
            node: b,
            bypass: true,
        },
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::SetResolution {
            node: b,
            resolution: Resolution::Half,
        },
    )
    .unwrap();
    h.exec(&mut g, Command::RemoveNode(b)).unwrap();
    let full = g.clone();

    let mut steps = 0;
    while h.undo(&mut g).unwrap() {
        steps += 1;
    }
    assert_eq!(steps, 14);
    assert_eq!(g.nodes(), empty.nodes());
    assert!(g.links().is_empty());
    assert!(!h.can_undo());

    while h.redo(&mut g).unwrap() {}
    assert_eq!(g, full);
    // A new edit clears the redo stack.
    h.undo(&mut g).unwrap();
    assert!(h.can_redo());
    h.exec(
        &mut g,
        Command::SetBypass {
            node: a,
            bypass: true,
        },
    )
    .unwrap();
    assert!(!h.can_redo());
}

#[test]
fn a_slider_drag_is_one_undo_step() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, ..] = sample(&mut g, &mut h);
    let before = g.clone();
    for v in [1.1, 1.2, 1.3, 1.4] {
        h.exec_merge(
            &mut g,
            Command::SetParam {
                node: a,
                param: "speed".into(),
                value: ParamValue::Float(v),
            },
            Some("drag speed"),
        )
        .unwrap();
    }
    h.seal();
    h.undo(&mut g).unwrap();
    assert_eq!(g, before);
}

#[test]
fn a_failing_batch_changes_nothing() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, _, m, _, l] = sample(&mut g, &mut h);
    let before = g.clone();
    let r = h.exec(
        &mut g,
        Command::Batch(vec![
            Command::SetBypass {
                node: a,
                bypass: true,
            },
            Command::Disconnect(Link::new(
                OutputRef::new(a, "out"),
                Endpoint::input(m, "base"),
            )),
            wire(l, "value", m, "top"), // type mismatch
        ]),
    );
    assert!(r.is_err());
    assert_eq!(g, before);
}

#[test]
fn nested_groups_and_ungrouping() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, m, _, _] = sample(&mut g, &mut h);
    let outer = g.new_group_id();
    let inner = g.new_group_id();
    let put = |id, parent, nodes: Vec<NodeId>, published| {
        Command::PutGroup(Group {
            id,
            label: format!("G{}", id.0),
            parent,
            nodes,
            published,
            collapsed: false,
        })
    };
    h.exec(&mut g, put(outer, None, vec![m], vec![])).unwrap();
    h.exec(&mut g, put(inner, Some(outer), vec![a, b], vec![]))
        .unwrap();
    // The outer group can publish a parameter from inside its sub-group.
    let publish = vec![PublishedParam {
        label: "Speed".into(),
        node: a,
        param: "speed".into(),
    }];
    h.exec(&mut g, put(outer, None, vec![m], publish.clone()))
        .unwrap();
    // A node can't be in two groups; a group can't be its own ancestor;
    // published controls must point inside.
    assert!(
        h.exec(&mut g, put(GroupId(9), None, vec![a], vec![]))
            .is_err()
    );
    assert!(
        h.exec(&mut g, put(outer, Some(inner), vec![m], vec![]))
            .is_err()
    );
    let o = add(&mut g, &mut h, &program_out());
    let outside = vec![PublishedParam {
        label: "x".into(),
        node: o,
        param: "speed".into(),
    }];
    assert!(
        h.exec(&mut g, put(inner, Some(outer), vec![a, b], outside))
            .is_err()
    );

    let before = g.clone();
    h.exec(&mut g, Command::RemoveGroup(outer)).unwrap();
    assert!(g.group(outer).is_none());
    assert_eq!(g.group(inner).unwrap().parent, None);
    h.undo(&mut g).unwrap();
    assert_eq!(g, before);
}

#[test]
fn save_then_load_gives_an_identical_graph() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, _, _, _] = sample(&mut g, &mut h);
    let gid = g.new_group_id();
    h.exec(
        &mut g,
        Command::PutGroup(Group {
            id: gid,
            label: "Pair".into(),
            parent: None,
            nodes: vec![a, b],
            published: vec![PublishedParam {
                label: "Speed".into(),
                node: a,
                param: "speed".into(),
            }],
            collapsed: false,
        }),
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::SetMods {
            node: a,
            param: "speed".into(),
            mods: vec![ModSlot::new(
                Source::Signal("kick_age".into()),
                Shape::Envelope { decay: 0.15 },
                0.33,
            )],
        },
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::SetParam {
            node: b,
            param: "speed".into(),
            value: ParamValue::Float(0.1 + 0.2), // not exactly representable
        },
    )
    .unwrap();
    let text = g.to_ron();
    let loaded = Graph::from_ron(&text).unwrap();
    assert_eq!(loaded, g);
    assert_eq!(loaded.to_ron(), text);
    // Numbering continues after loading.
    let mut loaded = loaded;
    assert_eq!(loaded.new_node(&shader()).id.0, 6);
}

#[test]
fn broken_files_are_reported_not_loaded() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [_, _, m, o, _] = sample(&mut g, &mut h);
    let text = g.to_ron();
    assert!(matches!(
        Graph::from_ron("(oops"),
        Err(GraphError::Parse(_))
    ));
    // Hand-edit the file into a loop: Program Out has no output, so wire the
    // merge's output into its own mask instead.
    let mut looped = Graph::from_ron(&text).unwrap();
    looped.links_mut_for_test().push(Link::new(
        OutputRef::new(m, "out"),
        Endpoint::input(m, "mask"),
    ));
    assert_eq!(
        Graph::from_ron(&looped.to_ron()).unwrap_err(),
        GraphError::Cycle
    );
    let _ = o;
}

#[test]
fn frames_and_colours_undo_and_save() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, ..] = sample(&mut g, &mut h);
    let mut h = History::new();
    let before = g.clone();
    let id = g.new_frame_id();
    let f = Frame {
        id,
        label: "Drums".into(),
        rect: [0.0, 0.0, 300.0, 200.0],
        colour: [200, 80, 60],
    };
    h.exec(&mut g, Command::PutFrame(f.clone())).unwrap();
    h.exec(
        &mut g,
        Command::SetColour {
            node: a,
            colour: Some([10, 200, 90]),
        },
    )
    .unwrap();
    h.exec(
        &mut g,
        Command::PutFrame(Frame {
            label: "Kick stuff".into(),
            ..f.clone()
        }),
    )
    .unwrap();
    assert_eq!(g.frame(id).unwrap().label, "Kick stuff");
    assert_eq!(g.node(a).unwrap().colour, Some([10, 200, 90]));
    // Save / load keeps them; numbering continues.
    let mut loaded = Graph::from_ron(&g.to_ron()).unwrap();
    assert_eq!(loaded, g);
    assert!(loaded.new_frame_id() > id);
    h.exec(&mut g, Command::RemoveFrame(id)).unwrap();
    assert!(g.frames().is_empty());
    while h.undo(&mut g).unwrap() {}
    assert_eq!(g.nodes(), before.nodes());
    assert!(g.frames().is_empty());
    assert!(h.redo(&mut g).unwrap());
    assert_eq!(g.frame(id).unwrap().label, "Drums");
    // Older files without frames still load.
    let old = before.to_ron().replace("frames: [],", "");
    assert!(Graph::from_ron(&old).is_ok());
}

impl Graph {
    fn links_mut_for_test(&mut self) -> &mut Vec<Link> {
        self.links_mut()
    }
}

#[test]
fn grouping_selected_nodes_nests_and_ungroups_cleanly() {
    let mut g = Graph::new();
    let mut h = History::new();
    let [a, b, m, o, l] = sample(&mut g, &mut h);
    let mut h = History::new();
    let empty = g.clone();

    // Group a + b, then publish a control on it.
    let (inner, cmd) = g.group_nodes(&[a, b], "Pair").unwrap();
    h.exec(&mut g, cmd).unwrap();
    assert_eq!(g.group(inner).unwrap().nodes, vec![a, b]);
    assert_eq!(g.group(inner).unwrap().parent, None);
    let mut grp = g.group(inner).unwrap().clone();
    grp.published.push(PublishedParam {
        label: "Speed".into(),
        node: a,
        param: "speed".into(),
    });
    h.exec(&mut g, Command::PutGroup(grp)).unwrap();

    // Grouping b (already inside "Pair") with m pulls the whole "Pair" in.
    let (outer, cmd) = g.group_nodes(&[b, m], "Mix").unwrap();
    h.exec(&mut g, cmd).unwrap();
    assert_eq!(g.group(outer).unwrap().nodes, vec![m]);
    assert_eq!(g.group(inner).unwrap().parent, Some(outer));
    assert_eq!(g.groups_around(a), vec![inner, outer]);
    assert_eq!(g.group_members(outer), vec![a, b, m]);
    assert_eq!(g.group_depth(inner), 1);
    assert!(g.group_contains(g.group(outer).unwrap(), a));

    // Grouping just `a` inside "Pair" makes a group inside "Pair", and the
    // published control on "Pair" survives (it still points inside).
    let (deep, cmd) = g.group_nodes(&[a], "Solo").unwrap();
    h.exec(&mut g, cmd).unwrap();
    assert_eq!(g.group(deep).unwrap().parent, Some(inner));
    assert_eq!(g.group(inner).unwrap().nodes, vec![b]);
    assert_eq!(g.group(inner).unwrap().published.len(), 1);
    assert_eq!(g.groups_around(a), vec![deep, inner, outer]);

    // Ungrouping "Solo" puts `a` back into "Pair".
    let before = g.clone();
    h.exec(&mut g, Command::RemoveGroup(deep)).unwrap();
    assert_eq!(g.group_of(a).unwrap().id, inner);
    h.undo(&mut g).unwrap();
    assert_eq!(g, before);

    // Collapsing is saved with the graph.
    let mut grp = g.group(outer).unwrap().clone();
    grp.collapsed = true;
    h.exec(&mut g, Command::PutGroup(grp)).unwrap();
    let back = Graph::from_ron(&g.to_ron()).unwrap();
    assert!(back.group(outer).unwrap().collapsed);

    // Nodes in no group, mixed with grouped ones: the new group is top level.
    let (top, cmd) = g.group_nodes(&[o, l, m], "All").unwrap();
    h.exec(&mut g, cmd).unwrap();
    assert_eq!(g.group(top).unwrap().parent, None);
    assert_eq!(g.group(outer).unwrap().parent, Some(top));
    assert!(g.group_nodes(&[], "x").is_err());

    while h.can_undo() {
        h.undo(&mut g).unwrap();
    }
    assert_eq!(g.groups(), empty.groups());
}

#[test]
fn old_nodes_gain_new_plugs_and_settings() {
    let mut g = Graph::new();
    let mut spec = merge();
    let n = g.new_node(&spec);
    let mut old = n.clone();
    // A newer version adds a plug and a setting.
    spec.inputs
        .insert(0, PortDef::new("extra", PortType::Signal).optional());
    spec.params.push(ParamDef::float("depth", 0.0, 1.0, 0.25));
    assert!(old.add_missing_from(&spec));
    assert_eq!(old.inputs.first().map(|p| p.name.as_str()), Some("extra"));
    assert_eq!(
        old.inputs.last().map(|p| p.name.as_str()),
        Some(crate::types::MASK_PORT)
    );
    assert!(old.param("depth").is_some());
    // Already up to date: nothing to do.
    assert!(!old.add_missing_from(&spec));
}
