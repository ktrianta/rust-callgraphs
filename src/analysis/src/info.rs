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
    pub fn def_path_to_string(&self, def_path: &DefPath) -> String {
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

pub struct TypeInfo {
    // Mapping from Adt to info.
    adts: HashMap<DefPath, (InternedString, Visibility, Module)>,
    // Mapping from Type to DefPath.
    type_to_def_path: HashMap<Type, DefPath>,
    // Mapping from Item to DefPath.
    item_to_def_path: HashMap<Item, DefPath>,
    // Mapping from Trait DefPath to info.
    traits: HashMap<DefPath, (InternedString, Visibility, Module)>,
    // Mapping from Impl DefPath to Trait and Adt DefPaths.
    impls: HashMap<DefPath, (Option<DefPath>, DefPath)>,
    // Mapping from Trait DefPath to vector of its Impl DefPaths.
    pub trait_to_impls: HashMap<DefPath, Vec<DefPath>>,
    // Mapping from Trait Impl DefPath to mapping of Item Name to DefPath.
    pub trait_impl_to_items: HashMap<DefPath, HashMap<InternedString, DefPath>>,
    // Mapping from Trait Item DefPath to (Item Name, Item Defaultness, Trait DefPath).
    pub trait_items: HashMap<DefPath, (InternedString, Defaultness, DefPath)>,
}

impl TypeInfo {
    pub fn new(tables: &Tables) -> Self {
        let mut item_to_def_path = HashMap::new();
        let mut adts = HashMap::new();
        let mut impls = HashMap::new();
        let mut type_to_def_path = HashMap::new();
        for (item, typ, def_path, module, name, visibility) in tables.relations.type_defs.iter() {
            item_to_def_path.insert(*item, *def_path);
            type_to_def_path.insert(*typ, *def_path);
            adts.insert(*def_path, (*name, *visibility, *module));
        }
        for (typ, def_path, _, _, _) in tables.relations.types_adt_def.iter() {
            type_to_def_path.insert(*typ, *def_path);
        }
        for (def_path, item, _, _, _, _, _, _, typ) in tables.relations.impl_definitions.iter() {
            item_to_def_path.insert(*item, *def_path);
            if let Some(adt_def_path) = type_to_def_path.get(typ) {
                impls.insert(*def_path, (None, *adt_def_path));
            } else {
                panic!("Impl definition: missing adt def path.");
            }
        }
        let mut traits = HashMap::new();
        for (item, def_path, module, name, visibility, _, _, _) in tables.relations.traits.iter() {
            item_to_def_path.insert(*item, *def_path);
            traits.insert(*def_path, (*name, *visibility, *module));
        }
        let mut trait_to_impls: HashMap<DefPath, Vec<DefPath>> = HashMap::new();
        for (impl_id, typ, trait_def_path) in tables.relations.trait_impls.iter() {
            let impl_def_path = item_to_def_path[impl_id];
            if let Some(impl_def_paths) = trait_to_impls.get_mut(trait_def_path) {
                impl_def_paths.push(impl_def_path);
            } else {
                trait_to_impls.insert(*trait_def_path, vec![impl_def_path]);
            }
            if let Some(adt_def_path) = type_to_def_path.get(typ) {
                impls.insert(impl_def_path, (Some(*trait_def_path), *adt_def_path));
            } else {
                panic!("Trait impl definition: missing adt def path.");
            }
        }
        let mut trait_impl_to_items: HashMap<_, HashMap<_, _>> = HashMap::new();
        for (impl_id, item_def_path, item_name) in tables.relations.trait_impl_items.iter() {
            let impl_def_path = item_to_def_path[impl_id];
            if let Some(items) = trait_impl_to_items.get_mut(&impl_def_path) {
                items.insert(*item_name, *item_def_path);
            } else {
                let mut items = HashMap::new();
                items.insert(*item_name, *item_def_path);
                trait_impl_to_items.insert(impl_def_path, items);
            }
        }
        let mut trait_items = HashMap::new();
        for (trait_id, def_path, name, defaultness) in tables.relations.trait_items.iter() {
            let trait_def_path = item_to_def_path[trait_id];
            trait_items.insert(*def_path, (*name, *defaultness, trait_def_path));
        }
        Self {
            adts,
            type_to_def_path,
            item_to_def_path,
            traits,
            trait_to_impls,
            impls,
            trait_impl_to_items,
            trait_items,
        }
    }
}
