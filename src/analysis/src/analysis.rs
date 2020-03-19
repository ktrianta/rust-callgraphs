use crate::callgraph::{CallGraph, NodeId};
use crate::info::{FunctionsInfo, InterningInfo, TypeInfo};
use crate::types::TypeHierarchy;
use corpus_database::tables::Tables;
use corpus_database::types::*;
use std::collections::{HashMap, HashSet};

pub struct CallGraphAnalysis<'a> {
    // Generic calls.
    generic_calls: HashSet<FunctionCall>,
    // Dynamic dispatch calls.
    virtual_calls: HashSet<FunctionCall>,
    // Call-graph.
    call_graph: Vec<(FunctionCall, DefPath, DefPath)>,
    // Mapping from generic function to its instantiations.
    generic_calls_instantiations: HashMap<FunctionCall, Vec<DefPath>>,
    type_info: TypeInfo,
    functions_info: FunctionsInfo<'a>,
    interning_info: InterningInfo<'a>,
}

impl<'a> CallGraphAnalysis<'a> {
    pub fn new(tables: &'a Tables) -> Self {
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
        let mut generic_calls_instantiations: HashMap<FunctionCall, Vec<DefPath>> = HashMap::new();
        for (call_id, instantiation) in tables.relations.instantiations.iter() {
            if let Some(instantiations) = generic_calls_instantiations.get_mut(call_id) {
                instantiations.push(*instantiation);
            } else {
                generic_calls_instantiations.insert(*call_id, vec![*instantiation]);
            }
        }
        Self {
            generic_calls,
            virtual_calls,
            call_graph,
            generic_calls_instantiations,
            type_info: TypeInfo::new(tables),
            functions_info: FunctionsInfo::new(tables),
            interning_info: InterningInfo::new(&tables.interning_tables),
        }
    }
    fn add_node_to_callgraph(&self, callgraph: &mut CallGraph, def_path: &DefPath) -> NodeId {
        if let Some(node_id) = callgraph.get_node_by_def_path(def_path) {
            *node_id
        } else {
            let crate_name = self.interning_info.def_path_to_crate(def_path);
            let relative_def_id = self.interning_info.def_path_to_string(def_path);
            let package = self.interning_info.def_path_to_package(def_path);
            let num_lines = self.functions_info.functions_num_lines(def_path);
            callgraph.add_node(def_path, package, crate_name, relative_def_id, num_lines)
        }
    }
    pub fn run(&'a self) -> CallGraph {
        let mut callgraph = CallGraph::new();

        for (call_id, caller, callee) in self.call_graph.iter() {
            let caller_id = self.add_node_to_callgraph(&mut callgraph, &caller);
            if self.virtual_calls.contains(&call_id) {
                match self.resolve_virtual_call(&callee) {
                    Ok(resolved_callees) => {
                        for callee in resolved_callees {
                            let callee_id = self.add_node_to_callgraph(&mut callgraph, &callee);
                            callgraph.add_virtual_edge(caller_id, callee_id);
                        }
                    }
                    Err(_) => {}
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
            } else if self.generic_calls.contains(&call_id) {
                // Add concrete (static dispatch) calls.
                let mut instantiations_set = HashSet::new();
                if let Some(instantiations) = self.generic_calls_instantiations.get(&call_id) {
                    for inst in instantiations {
                        let inst_id = self.add_node_to_callgraph(&mut callgraph, &inst);
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
                                let callee_id = self.add_node_to_callgraph(&mut callgraph, &callee);
                                callgraph.add_virtual_edge(caller_id, callee_id);
                            }
                        }
                    }
                    Err(_) => {}
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
            } else {
                let callee_id = self.add_node_to_callgraph(&mut callgraph, &callee);
                callgraph.add_static_edge(caller_id, callee_id);
            }
        }
        callgraph
    }
    pub fn types(&self) -> TypeHierarchy {
        let types = TypeHierarchy::new(&self.type_info, &self.interning_info);
        types
    }
    fn resolve_virtual_call(
        &'a self,
        function_def_path: &DefPath,
    ) -> Result<Vec<DefPath>, Box<dyn std::error::Error>> {
        let (function_name, defaultness, trait_def_path) = self
            .type_info
            .trait_items
            .get(function_def_path)
            .ok_or("Trait method is not registered as a trait item.")?;
        let trait_impls = self
            .type_info
            .trait_to_impls
            .get(trait_def_path)
            .ok_or("Trait is not registered for impls.")?;
        let mut is_implemented_by_all = true;
        let mut resolved_functions = Vec::new();
        for trait_impl in trait_impls {
            if let Some(items) = self.type_info.trait_impl_to_items.get(trait_impl) {
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
