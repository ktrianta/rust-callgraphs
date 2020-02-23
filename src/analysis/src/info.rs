use corpus_database::tables::{InterningTables, Tables};
use corpus_database::types::*;
use std::collections::HashMap;

pub(crate) struct InterningInfo<'a> {
    package_names: Vec<String>,
    package_names_registry: HashMap<CrateHash, usize>,
    interning_tables: &'a InterningTables,
}

impl<'a> InterningInfo<'a> {
    pub fn new(interning_tables: &'a InterningTables) -> Self {
        let mut package_names = Vec::new();
        let mut package_names_registry = HashMap::new();
        for (_, (pkg, version, _, crate_hash, _)) in interning_tables.builds.iter() {
            let pkg_interned_string = interning_tables.package_names[*pkg];
            let version_interned_string = interning_tables.package_versions[*version];
            if package_names_registry.get(crate_hash).is_none() {
                package_names_registry.insert(*crate_hash, package_names.len());
                package_names.push(format!(
                    "{} {}",
                    interning_tables.strings[pkg_interned_string],
                    interning_tables.strings[version_interned_string]
                ));
            }
        }
        Self {
            package_names,
            package_names_registry,
            interning_tables,
        }
    }
    pub fn span_location_to_string(&self, location: SpanLocation) -> String {
        let interned_string = self.interning_tables.span_locations[location];
        self.interning_tables.strings[interned_string].clone()
    }
    pub fn relative_def_id_to_string(&self, def_path: &DefPath) -> String {
        let (_, _, relative_def_id, _, _) = self.interning_tables.def_paths[*def_path];
        let interned_string = self.interning_tables.relative_def_paths[relative_def_id];
        self.interning_tables.strings[interned_string].clone()
    }
    pub fn def_path_to_crate(&self, def_path: &DefPath) -> String {
        let (crate_name, _, _, _, _) = self.interning_tables.def_paths[*def_path];
        let interned_string = self.interning_tables.crate_names[crate_name];
        self.interning_tables.strings[interned_string].clone()
    }
    pub fn def_path_to_package(&self, def_path: &DefPath) -> String {
        let (_, crate_hash, _, _, _) = self.interning_tables.def_paths[*def_path];
        if let Some(index) = self.package_names_registry.get(&crate_hash) {
            self.package_names[*index].clone()
        } else {
            String::from("NULL")
        }
    }
}

pub(crate) struct FunctionsInfo<'a> {
    functions: HashMap<DefPath, (Module, Visibility, Option<SpanLocation>)>,
    // Interning tables.
    interning: InterningInfo<'a>,
}

impl<'a> FunctionsInfo<'a> {
    pub fn new(tables: &'a Tables) -> Self {
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
        let mut functions = HashMap::new();
        for (_, def_path, module, visibility, _, _, _) in
            tables.relations.function_definitions.iter()
        {
            if let Some(scope) = functions_scopes.get(def_path) {
                let span = scopes_spans[&scope];
                let location = spans_locations[&span];
                functions.insert(*def_path, (*module, *visibility, Some(location)));
            } else {
                functions.insert(*def_path, (*module, *visibility, None));
            }
        }
        Self {
            functions,
            interning: InterningInfo::new(&tables.interning_tables),
        }
    }
    pub fn iter_def_paths(&self) -> impl Iterator<Item = &DefPath> {
        self.functions.iter().map(|(def_path, _)| def_path)
    }
    pub fn functions_num_lines(&self, def_path: &DefPath) -> i32 {
        if let Some((_, _, Some(location))) = self.functions.get(def_path) {
            use std::str::FromStr;
            let string = self.interning.span_location_to_string(*location);
            let tokens: Vec<&str> = string.split(':').collect();
            let n = tokens.len();
            i32::from_str(&tokens[n - 2][1..]).unwrap_or(0)
                - i32::from_str(tokens[n - 4]).unwrap_or(0)
        } else {
            0
        }
    }
}
