use corpus_database::tables::{InterningTables, Tables};
use corpus_database::types::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "callgraph-analyzer",
    about = "Call-graph analyzer for Rust programs."
)]
struct CMDArgs {
    #[structopt(
        parse(from_os_str),
        default_value = "../../database",
        long = "database",
        help = "The directory in which the database is stored."
    )]
    database_root: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Node {
    pub id: usize,
    pub package: String,
    pub crate_name: String,
    pub relative_def_path: String,
}

#[derive(Serialize, Deserialize)]
struct NodeInfo {
    pub id: usize,
    pub num_lines: i32,
}

type NodeId = usize;

#[derive(Serialize, Deserialize)]
struct CallGraph {
    // Call-graph nodes, i.e., functions
    pub nodes: Vec<Node>,
    // Call-graph edges, i.e., caller function calls callee function.
    // The boolean value indicates if the call is statically dispatched.
    pub edges: Vec<(NodeId, NodeId, bool)>,
    // Extra node information
    pub nodes_info: Vec<NodeInfo>,
    #[serde(skip_serializing)]
    node_registry: HashMap<DefPath, usize>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            nodes_info: Vec::new(),
            node_registry: HashMap::new(),
        }
    }
    pub fn add_node(
        &mut self,
        def_path: &DefPath,
        package: String,
        crate_name: String,
        relative_def_path: String,
        num_lines: i32,
    ) -> NodeId {
        assert!(!self.node_registry.contains_key(def_path));
        let id = self.node_registry.len();
        self.node_registry.insert(*def_path, id);
        self.nodes.push(Node {
            id,
            package,
            crate_name,
            relative_def_path,
        });
        self.nodes_info.push(NodeInfo {
            id,
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

struct CallGraphAnalysis {
    // Generic calls.
    generic_calls: HashSet<DefPath>,
    // Dynamic dispatch calls.
    virtual_calls: HashSet<DefPath>,
    // Call-graph.
    call_graph: Vec<(FunctionCall, DefPath, DefPath)>,
    // Mapping from trait def_path to vector of its impl ids.
    traits_impls: HashMap<DefPath, Vec<Item>>,
    // Mapping from trait impl id to map of item_name to item_def_path.
    traits_impl_items: HashMap<Item, HashMap<InternedString, DefPath>>,
    // Mapping from trait item def_path to tuple (trait_id, item_name, item_defaultness).
    trait_items: HashMap<DefPath, (Item, InternedString, Defaultness)>,
    // Mapping from trait id (item) to tuple (def_path, name).
    trait_ids: HashMap<Item, (DefPath, InternedString)>,
    // Mapping from generic function to its instantiations.
    generic_calls_instantiations: HashMap<FunctionCall, Vec<DefPath>>,
    // Mapping from crate hash to package information.
    package_info: HashMap<CrateHash, (Package, PackageVersion)>,
    // The following three maps capture function scopes and location information.
    functions_scopes: HashMap<DefPath, Scope>,
    scopes_spans: HashMap<Scope, Span>,
    spans_locations: HashMap<Span, SpanLocation>,
    // Interning tables.
    interning_tables: InterningTables,
}

impl CallGraphAnalysis {
    pub fn new(tables: Tables) -> Self {
        let mut generic_calls = HashSet::new();
        for (call,) in tables.relations.generic_calls.iter() {
            generic_calls.insert(*call);
        }
        let mut virtual_calls = HashSet::new();
        for (call,) in tables.relations.virtual_calls.iter() {
            virtual_calls.insert(*call);
        }
        let mut call_graph = Vec::new();
        for (call_id, caller, callee) in tables.relations.call_graph.iter() {
            call_graph.push((*call_id, *caller, *callee));
        }
        let mut traits_impls: HashMap<DefPath, Vec<Item>> = HashMap::new();
        for (impl_id, _, trait_def_path) in tables.relations.trait_impls.iter() {
            if let Some(impl_ids) = traits_impls.get_mut(trait_def_path) {
                impl_ids.push(*impl_id);
            } else {
                traits_impls.insert(*trait_def_path, vec![*impl_id]);
            }
        }
        let mut traits_impl_items: HashMap<Item, HashMap<InternedString, DefPath>> = HashMap::new();
        for (impl_id, item_def_path, item_name) in tables.relations.trait_impl_items.iter() {
            if let Some(items) = traits_impl_items.get_mut(impl_id) {
                items.insert(*item_name, *item_def_path);
            } else {
                let mut items = HashMap::new();
                items.insert(*item_name, *item_def_path);
                traits_impl_items.insert(*impl_id, items);
            }
        }
        let mut trait_items = HashMap::new();
        for (trait_id, item_def_path, item_name, item_defaultness) in
            tables.relations.trait_items.iter()
        {
            trait_items.insert(*item_def_path, (*trait_id, *item_name, *item_defaultness));
        }
        let mut trait_ids = HashMap::new();
        for (id, def_path, name, _, _, _, _) in tables.relations.traits.iter() {
            trait_ids.insert(*id, (*def_path, *name));
        }
        let mut generic_calls_instantiations: HashMap<FunctionCall, Vec<DefPath>> = HashMap::new();
        for (call_id, instantiation) in tables.relations.instantiations.iter() {
            if let Some(instantiations) = generic_calls_instantiations.get_mut(call_id) {
                instantiations.push(*instantiation);
            } else {
                generic_calls_instantiations.insert(*call_id, vec![*instantiation]);
            }
        }
        let mut functions_scopes = HashMap::new();
        for (_, def_path, scope) in tables.relations.mir_cfgs.iter() {
            functions_scopes.insert(*def_path, *scope);
        }
        let mut scopes_spans = HashMap::new();
        for (scope, _, _, span) in tables.relations.subscopes.iter() {
            scopes_spans.insert(*scope, *span);
        }
        let mut spans_locations = HashMap::new();
        for (span, _, _, location) in tables.relations.spans.iter() {
            spans_locations.insert(*span, *location);
        }
        let mut package_info = HashMap::new();
        for (_, (pkg, version, _, crate_hash, _)) in tables.interning_tables.builds.iter() {
            if package_info.get(crate_hash).is_none() {
                package_info.insert(crate_hash.clone(), (pkg.clone(), version.clone()));
            }
        }
        Self {
            generic_calls,
            virtual_calls,
            call_graph,
            traits_impls,
            traits_impl_items,
            trait_items,
            trait_ids,
            generic_calls_instantiations,
            package_info,
            functions_scopes,
            scopes_spans,
            spans_locations,
            interning_tables: tables.interning_tables,
        }
    }
    fn relative_def_path_string(&self, def_path: RelativeDefId) -> String {
        let interned_string = self.interning_tables.relative_def_paths[def_path];
        self.interning_tables.strings[interned_string].clone()
    }
    fn functions_num_lines(&self, def_path: &DefPath) -> i32 {
        if let Some(scope) = self.functions_scopes.get(def_path) {
            use std::str::FromStr;
            let span = self.scopes_spans[&scope];
            let location = self.spans_locations[&span];
            let interned_string = self.interning_tables.span_locations[location];
            let string = &self.interning_tables.strings[interned_string];
            let tokens: Vec<&str> = string.split(':').collect();
            let n = tokens.len();
            i32::from_str(&tokens[n-2][1..]).unwrap_or(0) - i32::from_str(tokens[n-4]).unwrap_or(0)
        } else {
            0
        }
    }
    #[allow(dead_code)]
    fn summary_key_string(&self, summary_id: SummaryId) -> String {
        let interned_string = self.interning_tables.summary_keys[summary_id];
        self.interning_tables.strings[interned_string].clone()
    }
    pub fn crate_name_string(&self, crate_name: Crate) -> String {
        let interned_string = self.interning_tables.crate_names[crate_name];
        self.interning_tables.strings[interned_string].clone()
    }
    pub fn package_string(&self, crate_hash_id: &CrateHash) -> String {
        if let Some((pkg_id, version_id)) = self.package_info.get(crate_hash_id) {
            let pkg_interned_string = self.interning_tables.package_names[*pkg_id];
            let version_interned_string = self.interning_tables.package_versions[*version_id];
            format!(
                "{} {}",
                self.interning_tables.strings[pkg_interned_string],
                self.interning_tables.strings[version_interned_string]
            )
        } else {
            String::from("NULL")
        }
    }
    fn add_or_get_callgraph_node(&self, callgraph: &mut CallGraph, def_path: &DefPath) -> NodeId {
        if let Some(node_id) = callgraph.get_node_by_def_path(def_path) {
            *node_id
        } else {
            let (crate_name_id, crate_hash_id, relative_def_id, _, _) =
                self.interning_tables.def_paths[*def_path];
            let crate_name = self.crate_name_string(crate_name_id);
            let relative_def_path = self.relative_def_path_string(relative_def_id);
            let package = self.package_string(&crate_hash_id);
            let num_lines = self.functions_num_lines(&def_path);
            callgraph.add_node(def_path, package, crate_name, relative_def_path, num_lines)
        }
    }
    fn run(&mut self) -> CallGraph {
        let mut callgraph = CallGraph::new();

        for (call_id, caller, callee) in self.call_graph.iter() {
            let caller_id = self.add_or_get_callgraph_node(&mut callgraph, &caller);

            if self.virtual_calls.contains(&callee) {
                match self.resolve_virtual_call(&callee) {
                    Ok(resolved_callees) => {
                        for callee in resolved_callees {
                            let callee_id = self.add_or_get_callgraph_node(&mut callgraph, &callee);
                            callgraph.add_virtual_edge(caller_id, callee_id);
                        }
                    }
                    Err(_) => {},
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
            } else if self.generic_calls.contains(&callee) {
                // Add concrete (static dispatch) calls.
                let mut instantiations_set = HashSet::new();
                if let Some(instantiations) = self.generic_calls_instantiations.get(&call_id) {
                    for inst in instantiations {
                        let inst_id = self.add_or_get_callgraph_node(&mut callgraph, &inst);
                        callgraph.add_static_edge(caller_id, inst_id);
                        instantiations_set.insert(inst);
                    }
                }
                // Overaproximate non-concrete calls, i.e., treat call as virtual.
                match self.resolve_virtual_call(&callee) {
                    Ok(resolved_callees) => {
                        for callee in resolved_callees {
                            if instantiations_set.get(&callee).is_none() {
                                // Add only if there is no concrete call already added.
                                let callee_id =
                                    self.add_or_get_callgraph_node(&mut callgraph, &callee);
                                callgraph.add_virtual_edge(caller_id, callee_id);
                            }
                        }
                    }
                    Err(_) => {},
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
            } else {
                let callee_id = self.add_or_get_callgraph_node(&mut callgraph, &callee);
                callgraph.add_static_edge(caller_id, callee_id);
            }
        }
        callgraph
    }
    fn resolve_virtual_call(
        &self,
        function_def_path: &DefPath,
    ) -> Result<Vec<DefPath>, Box<dyn std::error::Error>> {
        let (trait_id, function_name, defaultness) = self
            .trait_items
            .get(function_def_path)
            .ok_or("Trait method is not registered as a trait item.")?;
        let (trait_def_path, _) = self
            .trait_ids
            .get(trait_id)
            .ok_or("Trait is not registered.")?;
        let trait_impls = self
            .traits_impls
            .get(trait_def_path)
            .ok_or("Trait is not registered for impls.")?;
        let mut is_implemented_by_all = true;
        let mut resolved_functions = Vec::new();
        for trait_impl in trait_impls {
            if let Some(items) = self.traits_impl_items.get(trait_impl) {
                if let Some(item) = items.get(function_name) {
                    resolved_functions.push(*item);
                    continue;
                }
            }
            is_implemented_by_all = false;
        }
        if !is_implemented_by_all {
            assert!(*defaultness == Defaultness::DefaultWithValue);
            resolved_functions.push(*function_def_path);
        }
        Ok(resolved_functions)
    }
}

fn main() {
    let args = CMDArgs::from_args();
    let database_root = Path::new(&args.database_root);
    let tables = Tables::load_multifile(database_root).unwrap();
    let mut analysis = CallGraphAnalysis::new(tables);
    // println!("Loaded database");

    let callgraph = analysis.run();
    println!("{}", serde_json::to_string_pretty(&callgraph).unwrap());
}
