use crate::info::{InterningInfo, TypeInfo};
use corpus_database::types::Type as CorpusType;
use corpus_database::types::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;

#[derive(Serialize)]
pub struct Type {
    id: usize,
    string_id: String,
    package_name: Option<String>,
    package_version: Option<String>,
    relative_def_id: Option<String>,
}

#[derive(Serialize)]
pub struct Trait {
    id: usize,
    package_name: Option<String>,
    package_version: Option<String>,
    relative_def_id: String,
}

#[derive(Serialize)]
pub struct Impl {
    id: usize,
    type_id: usize,
    trait_id: Option<usize>,
    package_name: Option<String>,
    package_version: Option<String>,
    relative_def_id: String,
}

#[derive(Default, Serialize)]
pub struct TypeHierarchy {
    types: Vec<Type>,
    traits: Vec<Trait>,
    impls: Vec<Impl>,
    #[serde(skip)]
    type_registry: HashMap<String, usize>,
    #[serde(skip)]
    def_path_registry: HashMap<DefPath, usize>,
}

impl TypeHierarchy {
    pub(crate) fn new(types: &TypeInfo, interning: &InterningInfo) -> Self {
        let mut type_hierarchy = TypeHierarchy::default();
        for typ in types.iter_adt_types() {
            type_hierarchy.register_type(*typ, types, &interning);
        }
        for def_path in types.iter_trait_def_paths() {
            type_hierarchy.register_trait(*def_path, interning);
        }
        for def_path in types.iter_impl_def_paths() {
            type_hierarchy.register_impl(*def_path, types, &interning);
        }
        type_hierarchy
    }
    fn register_type(
        &mut self,
        typ: CorpusType,
        types: &TypeInfo,
        interning: &InterningInfo,
    ) -> usize {
        let (string_id, opt_def_path) = types.resolve_type(&typ, interning);
        if let Some(id) = self.type_registry.get(&string_id) {
            *id
        } else {
            let id = self.type_registry.len() + self.def_path_registry.len();
            self.type_registry.insert(string_id.clone(), id);
            let mut package_name = None;
            let mut package_version = None;
            let mut relative_def_id = None;
            if let Some(def_path) = opt_def_path {
                relative_def_id = Some(interning.def_path_to_string(&def_path));
                if let Some((name, version)) = interning.def_path_to_package(&def_path) {
                    package_name = Some(name);
                    package_version = Some(version);
                }
            }
            self.types.push(Type {
                id,
                string_id,
                package_name,
                package_version,
                relative_def_id,
            });
            id
        }
    }
    fn register_trait(&mut self, def_path: DefPath, interning: &InterningInfo) -> usize {
        if let Some(id) = self.def_path_registry.get(&def_path) {
            *id
        } else {
            let id = self.type_registry.len() + self.def_path_registry.len();
            self.def_path_registry.insert(def_path, id);
            let mut package_name = None;
            let mut package_version = None;
            if let Some((name, version)) = interning.def_path_to_package(&def_path) {
                package_name = Some(name);
                package_version = Some(version);
            }
            let relative_def_id = interning.def_path_to_string(&def_path);
            self.traits.push(Trait {
                id,
                package_name,
                package_version,
                relative_def_id,
            });
            id
        }
    }
    fn register_impl(
        &mut self,
        def_path: DefPath,
        types: &TypeInfo,
        interning: &InterningInfo,
    ) -> usize {
        if let Some(id) = self.def_path_registry.get(&def_path) {
            *id
        } else {
            let id = self.type_registry.len() + self.def_path_registry.len();
            self.def_path_registry.insert(def_path, id);
            let (opt_trait_def_path, typ) = types.get_impl_types(&def_path);
            let type_id = self.register_type(typ, types, interning);
            let trait_id = match opt_trait_def_path {
                Some(trait_def_path) => Some(self.register_trait(trait_def_path, interning)),
                None => None,
            };
            let mut package_name = None;
            let mut package_version = None;
            if let Some((name, version)) = interning.def_path_to_package(&def_path) {
                package_name = Some(name);
                package_version = Some(version);
            }
            let relative_def_id = interning.def_path_to_string(&def_path);
            self.impls.push(Impl {
                id,
                type_id,
                trait_id,
                package_name,
                package_version,
                relative_def_id,
            });
            id
        }
    }
    pub fn save(&self, path: &std::path::Path) {
        let mut file =
            File::create(path).unwrap_or_else(|e| panic!("Unable to create {:?}: {}", path, e));
        serde_json::to_writer_pretty(&mut file, self)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    }
}
