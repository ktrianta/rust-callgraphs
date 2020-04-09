use crate::callgraph::{CallGraph, NodeId};
use crate::info::{FunctionsInfo, InterningInfo, MacrosInfo, ModulesInfo, TypeInfo};
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
    types: TypeInfo,
    functions: FunctionsInfo<'a>,
    macros: MacrosInfo<'a>,
    modules: ModulesInfo,
    interning: InterningInfo<'a>,
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
            types: TypeInfo::new(tables),
            functions: FunctionsInfo::new(tables),
            macros: MacrosInfo::new(tables),
            modules: ModulesInfo::new(tables),
            interning: InterningInfo::new(&tables.interning_tables),
        }
    }
    fn add_function_to_callgraph(&self, callgraph: &mut CallGraph, def_path: &DefPath) -> NodeId {
        self.add_node_to_callgraph(callgraph, def_path, false)
    }
    fn add_macro_to_callgraph(&self, callgraph: &mut CallGraph, def_path: &DefPath) -> NodeId {
        self.add_node_to_callgraph(callgraph, def_path, true)
    }
    fn add_node_to_callgraph(
        &self,
        callgraph: &mut CallGraph,
        def_path: &DefPath,
        is_macro: bool,
    ) -> NodeId {
        if let Some(node_id) = callgraph.get_node_by_def_path(def_path) {
            *node_id
        } else {
            let crate_name = self.interning.def_path_to_crate(def_path);
            let relative_def_id = self.interning.def_path_to_string(def_path);
            let package_info = self.interning.def_path_to_package(def_path);
            let num_lines = match is_macro {
                true => self.macros.macros_num_lines(def_path),
                false => self.functions.functions_num_lines(def_path),
            };
            let source_location = match is_macro {
                true => self.macros.macros_source_location(def_path),
                false => self.functions.functions_source_location(def_path),
            };
            let is_externally_visible = match is_macro {
                true => self.macros.is_externally_visible(def_path, &self.modules),
                false => self
                    .functions
                    .is_externally_visible(def_path, &self.modules, &self.types),
            };
            callgraph.add_node(
                def_path,
                package_info,
                crate_name,
                relative_def_id,
                is_externally_visible,
                num_lines,
                is_macro,
                source_location,
            )
        }
    }
    fn add_function_calls_to_callgraph(&self, callgraph: &mut CallGraph) {
        // Add function definitions into the callgraph.
        for def_path in self.functions.iter_def_paths() {
            self.add_function_to_callgraph(callgraph, &def_path);
        }
        // Analyze function calls and extend the callgraph accordingly.
        for (call_id, caller, callee) in self.call_graph.iter() {
            let caller_id = self.add_function_to_callgraph(callgraph, &caller);
            if self.virtual_calls.contains(&call_id) {
                match self.resolve_virtual_call(&callee) {
                    Ok(resolved_callees) => {
                        for callee in resolved_callees {
                            let callee_id = self.add_function_to_callgraph(callgraph, &callee);
                            callgraph.add_virtual_function_call_edge(caller_id, callee_id);
                        }
                    }
                    Err(_) => {}
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
            } else if self.generic_calls.contains(&call_id) {
                // Add concrete (static dispatch) calls.
                let mut instantiations_set: HashSet<DefPath> = HashSet::new();
                if let Some(instantiations) = self.generic_calls_instantiations.get(&call_id) {
                    for inst in instantiations {
                        let inst_id = self.add_function_to_callgraph(callgraph, inst);
                        callgraph.add_static_function_call_edge(caller_id, inst_id);
                        instantiations_set.insert(*inst);
                    }
                }
                // Overaproximate non-concrete calls, i.e., treat call as virtual.
                match self.resolve_virtual_call(&callee) {
                    Ok(resolved_callees) => {
                        for callee in resolved_callees {
                            if instantiations_set.get(&callee).is_none() {
                                // Add only if there is no concrete call already added.
                                let callee_id = self.add_function_to_callgraph(callgraph, &callee);
                                callgraph.add_virtual_function_call_edge(caller_id, callee_id);
                                instantiations_set.insert(callee);
                            }
                        }
                    }
                    Err(_) => {}
                    // Err(error) => println!("Resoltion failed: {}", error),
                }
                if instantiations_set.is_empty() {
                    // No instantiations found, so we add a static edge to the original callee.
                    // This can happen if there are no available concretizations of the callee or
                    // if the function is generic, but not its receiver, thus we cannot treat it
                    // the call as a virtual dispatch call.
                    let callee_id = self.add_function_to_callgraph(callgraph, callee);
                    callgraph.add_static_function_call_edge(caller_id, callee_id);
                }
            } else {
                let callee_id = self.add_function_to_callgraph(callgraph, &callee);
                callgraph.add_static_function_call_edge(caller_id, callee_id);
            }
        }
    }
    fn add_macro_calls_to_callgraph(&self, callgraph: &mut CallGraph) {
        for def_path in self.macros.iter_def_paths() {
            self.add_macro_to_callgraph(callgraph, def_path);
        }
        for (caller_def_path, macro_def_path) in self.macros.iter_macro_calls() {
            let caller_id = self.add_function_to_callgraph(callgraph, caller_def_path);
            let callee_id = self.add_macro_to_callgraph(callgraph, macro_def_path);
            callgraph.add_macro_call_edge(caller_id, callee_id);
        }
    }
    pub fn run(&'a self) -> CallGraph {
        let mut callgraph = CallGraph::new();
        self.add_function_calls_to_callgraph(&mut callgraph);
        self.add_macro_calls_to_callgraph(&mut callgraph);
        callgraph
    }
    fn resolve_virtual_call(
        &'a self,
        function_def_path: &DefPath,
    ) -> Result<Vec<DefPath>, Box<dyn std::error::Error>> {
        let (function_name, _, trait_def_path) = self
            .types
            .trait_items
            .get(function_def_path)
            .ok_or("Trait method is not registered as a trait item.")?;
        let trait_impls = self
            .types
            .trait_to_impls
            .get(trait_def_path)
            .ok_or("Trait is not registered for impls.")?;
        let mut is_implemented_by_all = true;
        let mut resolved_functions = Vec::new();
        for trait_impl in trait_impls {
            if let Some(items) = self.types.trait_impl_to_items.get(trait_impl) {
                if let Some(item) = items.get(function_name) {
                    resolved_functions.push(*item);
                    continue;
                }
            }
            is_implemented_by_all = false;
        }
        if !is_implemented_by_all {
            // TODO: Handle impl specialization implemented in the following pull request
            // https://github.com/rust-lang/rfcs/pull/1210
            // Specialization is available only in the nightly rustc.
            // Package im-rc 13.0.0, the specialization feature in files
            //   * https://docs.rs/crate/im-rc/13.0.0/source/src/ord/map.rs
            //   * https://docs.rs/crate/im-rc/13.0.0/source/src/ord/set.rs
            resolved_functions.push(*function_def_path);
        }
        Ok(resolved_functions)
    }
    pub fn types(&self) -> TypeHierarchy {
        TypeHierarchy::new(&self.types, &self.interning)
    }
}
