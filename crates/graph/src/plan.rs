//! Evaluation planning: which nodes to run, in what order, at what size.
//! Pure and deterministic: the same graph always gives the same plan.

use std::collections::{BTreeMap, BTreeSet};

use crate::model::{Graph, NodeId, Resolution};

impl Graph {
    /// Nodes that feed `node`, directly or not (including itself), following
    /// every wire (pictures, masks, signals, triggers).
    pub fn upstream(&self, node: NodeId) -> BTreeSet<NodeId> {
        let mut seen = BTreeSet::from([node]);
        let mut todo = vec![node];
        while let Some(n) = todo.pop() {
            for l in self.links().iter().filter(|l| l.to.node() == n) {
                if seen.insert(l.from.node) {
                    todo.push(l.from.node);
                }
            }
        }
        seen
    }

    /// The nodes needed for `outputs`, in an order where every node comes after
    /// everything it reads from. Nodes not connected to an output are left out.
    /// Ties are broken by node number, so the order is always the same.
    pub fn plan(&self, outputs: &[NodeId]) -> Vec<NodeId> {
        let needed: BTreeSet<NodeId> = outputs
            .iter()
            .filter(|o| self.node(**o).is_some())
            .flat_map(|o| self.upstream(*o))
            .collect();
        // Kahn's algorithm over the needed nodes.
        let mut waiting: BTreeMap<NodeId, usize> = needed.iter().map(|n| (*n, 0)).collect();
        for l in self.links() {
            if needed.contains(&l.from.node)
                && let Some(c) = waiting.get_mut(&l.to.node())
            {
                *c += 1;
            }
        }
        let mut ready: BTreeSet<NodeId> = waiting
            .iter()
            .filter(|(_, c)| **c == 0)
            .map(|(n, _)| *n)
            .collect();
        let mut order = Vec::with_capacity(needed.len());
        while let Some(n) = ready.pop_first() {
            order.push(n);
            for l in self.links().iter().filter(|l| l.from.node == n) {
                let to = l.to.node();
                if let Some(c) = waiting.get_mut(&to) {
                    *c -= 1;
                    if *c == 0 {
                        ready.insert(to);
                    }
                }
            }
        }
        order
    }
}

/// Pixel size of a node's picture. `output` is the size of the final output;
/// `proxy` scales everything down for the editor preview (1 = full size).
pub fn node_size(res: Resolution, output: (u32, u32), proxy: f32) -> (u32, u32) {
    let (w, h) = match res {
        Resolution::Inherit => (output.0 as f32, output.1 as f32),
        Resolution::Half => (output.0 as f32 * 0.5, output.1 as f32 * 0.5),
        Resolution::Quarter => (output.0 as f32 * 0.25, output.1 as f32 * 0.25),
        Resolution::Custom { width, height } => (width as f32, height as f32),
    };
    let p = proxy.clamp(0.05, 1.0);
    (
        ((w * p).round() as u32).max(1),
        ((h * p).round() as u32).max(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;

    fn spec(kind: &str, inputs: &[&str], out: bool) -> NodeSpec {
        NodeSpec {
            kind: kind.into(),
            label: kind.into(),
            inputs: inputs
                .iter()
                .map(|n| PortDef::new(n, PortType::Image))
                .collect(),
            outputs: if out {
                vec![PortDef::new("out", PortType::Image)]
            } else {
                vec![]
            },
            params: vec![],
        }
    }

    fn add(g: &mut Graph, s: &NodeSpec) -> NodeId {
        let n = g.new_node(s);
        let id = n.id;
        g.apply(Command::AddNode(Box::new(n))).unwrap();
        id
    }

    fn wire(g: &mut Graph, a: NodeId, b: NodeId, port: &str) {
        g.apply(Command::Connect(Link::new(
            OutputRef::new(a, "out"),
            Endpoint::input(b, port),
        )))
        .unwrap();
    }

    #[test]
    fn plan_orders_dependencies_and_skips_unconnected_nodes() {
        let mut g = Graph::new();
        let out = add(&mut g, &spec("out", &["image"], false));
        let merge = add(&mut g, &spec("merge", &["base", "top"], true));
        let a = add(&mut g, &spec("a", &[], true));
        let fx = add(&mut g, &spec("fx", &["input"], true));
        let b = add(&mut g, &spec("b", &[], true));
        let stray = add(&mut g, &spec("stray", &[], true));
        wire(&mut g, a, fx, "input");
        wire(&mut g, fx, merge, "top");
        wire(&mut g, b, merge, "base");
        wire(&mut g, merge, out, "image");
        let order = g.plan(&[out]);
        assert_eq!(order.len(), 5);
        assert!(!order.contains(&stray));
        let pos = |n: NodeId| order.iter().position(|x| *x == n).unwrap();
        assert!(pos(a) < pos(fx));
        assert!(pos(fx) < pos(merge));
        assert!(pos(b) < pos(merge));
        assert_eq!(*order.last().unwrap(), out);
        // Same graph, same plan, every time.
        assert_eq!(g.plan(&[out]), order);
        assert_eq!(Graph::from_ron(&g.to_ron()).unwrap().plan(&[out]), order);
        // Unknown outputs plan nothing.
        assert!(g.plan(&[NodeId(99)]).is_empty());
    }

    #[test]
    fn node_sizes() {
        assert_eq!(
            node_size(Resolution::Inherit, (1920, 1080), 1.0),
            (1920, 1080)
        );
        assert_eq!(node_size(Resolution::Half, (1920, 1080), 1.0), (960, 540));
        assert_eq!(
            node_size(Resolution::Quarter, (1920, 1080), 0.5),
            (240, 135)
        );
        assert_eq!(
            node_size(
                Resolution::Custom {
                    width: 64,
                    height: 32
                },
                (1920, 1080),
                1.0
            ),
            (64, 32)
        );
        assert_eq!(node_size(Resolution::Quarter, (2, 2), 1.0), (1, 1));
    }
}
