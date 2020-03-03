use corpus_database::types::DefPath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = usize;

#[derive(Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub package: String,
    pub crate_name: String,
    pub relative_def_id: String,
    pub num_lines: i32,
}

#[derive(Serialize, Deserialize)]
pub struct CallGraph {
    // Call-graph nodes, i.e., functions
    nodes: Vec<Node>,
    // Call-graph edges, i.e., caller function calls callee function.
    // The boolean value indicates if the call is statically dispatched.
    edges: Vec<(NodeId, NodeId, bool)>,
    #[serde(skip)]
    node_registry: HashMap<DefPath, usize>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            node_registry: HashMap::new(),
        }
    }
    pub fn add_node(
        &mut self,
        def_path: &DefPath,
        package: String,
        crate_name: String,
        relative_def_id: String,
        num_lines: i32,
    ) -> NodeId {
        let id = self.node_registry.len();
        self.node_registry.insert(*def_path, id);
        self.nodes.push(Node {
            id,
            package,
            crate_name,
            relative_def_id,
            num_lines,
        });
        id
    }
    pub fn add_static_edge(&mut self, caller_id: NodeId, callee_id: NodeId) {
        self.edges.push((caller_id, callee_id, true));
    }
    pub fn add_virtual_edge(&mut self, caller_id: NodeId, callee_id: NodeId) {
        self.edges.push((caller_id, callee_id, false));
    }
    pub fn get_node_by_def_path(&self, def_path: &DefPath) -> Option<&NodeId> {
        self.node_registry.get(def_path)
    }
}