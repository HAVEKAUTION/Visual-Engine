//! Building graphs in code: the helper that templates and Looks use to lay
//! out nodes on a grid, set them, wire them and add the usual endings.

use graph::{Command, Endpoint, Frame, Graph, Link, NodeId, OutputRef, ParamValue, WireMod};
use scenes::SceneLibrary;
use signals::modulation::Shape;

use crate::registry::{
    self, AUDIO_TRIGGER, BEAT_TRIGGER, COUNTER, ENVELOPE, IMAGE_OUT, PROGRAM_OUT, TRIGGER_OUT,
    VALUE_OUT,
};

// ---------------------------------------------------------------- builder

pub const COL: f32 = 250.0;
pub const ROW: f32 = 240.0;

pub struct Builder<'a> {
    pub g: Graph,
    pub lib: &'a SceneLibrary,
    pub out: Option<NodeId>,
}

/// When a Switch moves on.
pub struct Changes {
    /// Every this many beats (0 = not on a clock).
    pub every: f32,
    /// Also on these audio events (indexes into `registry::TRIGGER_EVENTS`).
    pub events: &'static [(u32, &'static str)],
    pub random: bool,
    /// Index into the Switch's transitions (6 = a different one each time).
    pub transition: u32,
    pub beats: f32,
}

pub const SECTION: (u32, &str) = (8, "Section change");
pub const DROP: (u32, &str) = (7, "Drop");
pub const MIX_IT_UP: u32 = 6;

impl<'a> Builder<'a> {
    pub fn new(lib: &'a SceneLibrary) -> Self {
        Self {
            g: Graph::new(),
            lib,
            out: None,
        }
    }

    fn apply(&mut self, c: Command) -> Result<(), String> {
        self.g.apply(c).map(|_| ()).map_err(|e| e.to_string())
    }

    pub fn node(&mut self, kind: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let spec = registry::spec_for(kind, self.lib)?;
        let mut n = self.g.new_node(&spec);
        n.pos = [col * COL, row * ROW];
        if !label.is_empty() {
            n.label = label.into();
        }
        let id = n.id;
        self.apply(Command::AddNode(Box::new(n)))?;
        Ok(id)
    }

    pub fn shader(
        &mut self,
        file: &str,
        label: &str,
        col: f32,
        row: f32,
    ) -> Result<NodeId, String> {
        self.node(&registry::shader_kind(file), label, col, row)
    }

    pub fn set(&mut self, node: NodeId, param: &str, value: ParamValue) -> Result<(), String> {
        // A misspelt setting would silently do nothing: refuse it.
        let n = self.g.node(node).ok_or("no such node")?;
        if n.param(param).is_none() {
            return Err(format!("{} has no setting \"{param}\"", n.kind));
        }
        self.apply(Command::SetParam {
            node,
            param: param.into(),
            value,
        })
    }

    pub fn num(&mut self, node: NodeId, values: &[(&str, f32)]) -> Result<(), String> {
        for (p, v) in values {
            self.set(node, p, ParamValue::Float(*v))?;
        }
        Ok(())
    }

    pub fn choice(&mut self, node: NodeId, param: &str, index: u32) -> Result<(), String> {
        self.set(node, param, ParamValue::Choice(index))
    }

    pub fn wire(&mut self, from: NodeId, to: NodeId, port: &str) -> Result<(), String> {
        self.apply(Command::Connect(Link::new(
            OutputRef::new(from, IMAGE_OUT),
            Endpoint::input(to, port),
        )))
    }

    pub fn trigger(&mut self, from: NodeId, to: NodeId, port: &str) -> Result<(), String> {
        self.apply(Command::Connect(Link::new(
            OutputRef::new(from, TRIGGER_OUT),
            Endpoint::input(to, port),
        )))
    }

    /// A signal wire into a setting (the modulation rule: base + depth × value).
    pub fn drive(
        &mut self,
        from: NodeId,
        to: NodeId,
        param: &str,
        depth: f32,
    ) -> Result<(), String> {
        self.apply(Command::Connect(Link {
            modulation: Some(WireMod {
                shape: Shape::Linear,
                depth,
                enabled: true,
            }),
            ..Link::new(OutputRef::new(from, VALUE_OUT), Endpoint::param(to, param))
        }))
    }

    pub fn event(&mut self, index: u32, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(AUDIO_TRIGGER, label, col, row)?;
        self.choice(n, "event", index)?;
        Ok(n)
    }

    /// A Switch over `sources` (2 to 4), moving on as `when` says.
    pub fn switch(
        &mut self,
        sources: &[NodeId],
        when: &Changes,
        col: f32,
        row: f32,
    ) -> Result<NodeId, String> {
        let n = sources.len().clamp(2, 4);
        let sw = self.shader("fx_switch.wgsl", "", col, row)?;
        for (i, s) in sources.iter().take(4).enumerate() {
            self.wire(*s, sw, ["a", "b", "c", "d"][i])?;
        }
        self.choice(sw, "count", n as u32 - 2)?;
        self.choice(sw, "order", u32::from(when.random))?;
        self.choice(sw, "transition", when.transition)?;
        self.num(sw, &[("beats", when.beats)])?;
        let count = self.node(COUNTER, "Next picture", col, row - 1.0)?;
        self.num(count, &[("steps", n as f32)])?;
        if when.every > 0.0 {
            let label = format!("Every {} bars", when.every / 4.0);
            let beat = self.node(BEAT_TRIGGER, &label, col - 1.0, row - 1.5)?;
            self.num(beat, &[("every", when.every)])?;
            self.trigger(beat, count, "count")?;
        }
        for (k, (index, label)) in when.events.iter().enumerate() {
            let e = self.event(*index, label, col - 2.0, row - 1.5 + k as f32 * 0.6)?;
            self.trigger(e, count, "count")?;
        }
        self.drive(count, sw, "show", 1.0)?;
        Ok(sw)
    }

    /// The shared ending: Glow → Colour Grade → Program Out. Returns the Glow.
    pub fn finish(
        &mut self,
        from: NodeId,
        col: f32,
        row: f32,
        glow: f32,
    ) -> Result<NodeId, String> {
        let gl = self.shader("fx_glow.wgsl", "", col, row)?;
        self.num(
            gl,
            &[
                ("amount", glow),
                ("threshold", 0.55),
                ("radius", 1.6),
                ("kick_boost", 0.5),
            ],
        )?;
        let grade = self.shader("fx_colour.wgsl", "Final look", col + 1.0, row)?;
        self.num(grade, &[("contrast", 1.12), ("saturation", 1.1)])?;
        let out = self.node(PROGRAM_OUT, "", col + 2.0, row)?;
        self.wire(from, gl, "input")?;
        self.wire(gl, grade, "input")?;
        self.wire(grade, out, "image")?;
        self.out = Some(out);
        Ok(gl)
    }

    /// The drop blooms: a burst of extra glow for a couple of seconds.
    pub fn drop_bloom(&mut self, glow: NodeId, col: f32, row: f32) -> Result<(), String> {
        let drop = self.event(DROP.0, "Drop", col, row)?;
        let burst = self.node(ENVELOPE, "Bloom (2 s)", col + 1.0, row)?;
        self.num(burst, &[("attack", 0.02), ("decay", 2.0)])?;
        self.trigger(drop, burst, "trigger")?;
        self.drive(burst, glow, "amount", 0.5)
    }

    pub fn image(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(registry::IMAGE, label, col, row)?;
        self.set(n, registry::FILE_PARAM, ParamValue::Text(file.into()))?;
        Ok(n)
    }

    pub fn video(&mut self, file: &str, label: &str, col: f32, row: f32) -> Result<NodeId, String> {
        let n = self.node(registry::VIDEO, label, col, row)?;
        self.set(n, registry::FILE_PARAM, ParamValue::Text(file.into()))?;
        Ok(n)
    }

    /// Picture → Auto Cut-out → Liquid: a subject that floats and ripples.
    pub fn subject(
        &mut self,
        file: &str,
        label: &str,
        col: f32,
        row: f32,
    ) -> Result<NodeId, String> {
        let pic = self.image(file, label, col, row)?;
        self.choice(pic, "fit", 0)?; // show all of it
        let cut = self.shader("mask_auto_cutout.wgsl", "", col + 1.0, row)?;
        let liquid = self.shader("fx_liquid.wgsl", "", col + 2.0, row)?;
        self.wire(pic, cut, "input")?;
        self.wire(cut, liquid, "input")?;
        Ok(liquid)
    }

    pub fn mixer_layer(
        &mut self,
        mix: NodeId,
        from: NodeId,
        i: usize,
        mode: &str,
        opacity: f32,
    ) -> Result<(), String> {
        self.wire(from, mix, &registry::layer_port(i))?;
        let m = scenes::blend::mode_index(mode).ok_or("no such blend mode")? as u32;
        self.set(
            mix,
            &registry::layer_param("mode", i),
            ParamValue::Choice(m),
        )?;
        self.num(mix, &[(&registry::layer_param("opacity", i), opacity)])
    }

    /// A frame (titled backdrop) around everything.
    pub fn frame_all(&mut self, title: &str, colour: [u8; 3]) -> Result<(), String> {
        let ps: Vec<[f32; 2]> = self.g.nodes().iter().map(|n| n.pos).collect();
        if ps.is_empty() {
            return Ok(());
        }
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in ps {
            x0 = x0.min(p[0]);
            y0 = y0.min(p[1]);
            x1 = x1.max(p[0]);
            y1 = y1.max(p[1]);
        }
        let id = self.g.new_frame_id();
        self.apply(Command::PutFrame(Frame {
            id,
            label: title.to_string(),
            rect: [x0 - 30.0, y0 - 40.0, x1 - x0 + 270.0, y1 - y0 + 260.0],
            colour,
        }))
    }
}
