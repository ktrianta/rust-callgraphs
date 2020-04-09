use corpus_database::types::DefPath;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type NodeId = usize;

#[derive(Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub package_name: Option<String>,
    pub package_version: Option<String>,
    pub crate_name: String,
    pub relative_def_id: String,
    pub is_externally_visible: bool,
    pub num_lines: i32,
    pub source_location: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct CallGraph {
    // Call-graph function nodes
    functions: Vec<Node>,
    // Call-graph function nodes
    macros: Vec<Node>,
    // Call-graph edges, i.e., caller function calls callee function.
    // The boolean value indicates if the call is statically dispatched.
    function_calls: Vec<(NodeId, NodeId, bool)>,
    macro_calls: Vec<(NodeId, NodeId)>,
    #[serde(skip)]
    node_registry: HashMap<DefPath, usize>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            macros: Vec::new(),
            function_calls: Vec::new(),
            macro_calls: Vec::new(),
            node_registry: HashMap::new(),
        }
    }
    pub fn add_node(
        &mut self,
        def_path: &DefPath,
        package_info: Option<(String, String)>,
        crate_name: String,
        relative_def_id: String,
        is_externally_visible: bool,
        num_lines: i32,
        is_macro: bool,
        source_location: Option<String>,
    ) -> NodeId {
        let mut package_name = None;
        let mut package_version = None;
        if let Some((name, version)) = package_info {
            package_name = Some(name);
            package_version = Some(version);
        }
        let id = self.node_registry.len();
        self.node_registry.insert(*def_path, id);
        let nodes = match is_macro {
            true => &mut self.macros,
            false => &mut self.functions,
        };
        nodes.push(Node {
            id,
            package_name,
            package_version,
            crate_name,
            relative_def_id,
            is_externally_visible,
            num_lines,
            source_location,
        });
        id
    }
    pub fn add_static_function_call_edge(&mut self, caller_id: NodeId, callee_id: NodeId) {
        self.function_calls.push((caller_id, callee_id, true));
    }
    pub fn add_virtual_function_call_edge(&mut self, caller_id: NodeId, callee_id: NodeId) {
        self.function_calls.push((caller_id, callee_id, false));
    }
    pub fn add_macro_call_edge(&mut self, caller_id: NodeId, callee_id: NodeId) {
        self.macro_calls.push((caller_id, callee_id));
    }
    pub fn get_node_by_def_path(&self, def_path: &DefPath) -> Option<&NodeId> {
        self.node_registry.get(def_path)
    }
}
