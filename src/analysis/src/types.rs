use crate::info::{InterningInfo, TypeInfo};
use corpus_database::types::*;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;

#[derive(Serialize)]
pub struct Adt {
    id: usize,
    relative_def_id: String,
}

#[derive(Serialize)]
pub struct Trait {
    id: usize,
    relative_def_id: String,
}

#[derive(Serialize)]
pub struct Impl {
    id: usize,
    adt_id: usize,
    trait_id: Option<usize>,
    relative_def_id: String,
}

#[derive(Default, Serialize)]
pub struct TypeHierarchy {
    adts: Vec<Adt>,
    traits: Vec<Trait>,
    impls: Vec<Impl>,
    #[serde(skip)]
    type_registry: HashMap<DefPath, usize>,
}

impl TypeHierarchy {
    pub(crate) fn new(types: &TypeInfo, interning: &InterningInfo) -> Self {
        let mut type_hierarchy = TypeHierarchy::default();
        for def_path in types.iter_adt_def_paths() {
            type_hierarchy.add_adt(&interning, *def_path);
        }
        for def_path in types.iter_trait_def_paths() {
            type_hierarchy.add_trait(&interning, *def_path);
        }
        for def_path in types.iter_impl_def_paths() {
            type_hierarchy.add_impl(&types, &interning, *def_path);
        }
        type_hierarchy
    }
    fn register_type(&mut self, def_path: DefPath) -> usize {
        let id = self.type_registry.len();
        self.type_registry.insert(def_path, id);
        id
    }
    fn add_adt(&mut self, interning: &InterningInfo, def_path: DefPath) {
        let id = self.register_type(def_path);
        let relative_def_id = interning.def_path_to_string(&def_path);
        self.adts.push(Adt {
            id,
            relative_def_id,
        });
    }
    fn add_trait(&mut self, interning: &InterningInfo, def_path: DefPath) {
        let id = self.register_type(def_path);
        let relative_def_id = interning.def_path_to_string(&def_path);
        self.traits.push(Trait {
            id,
            relative_def_id,
        })
    }
    fn add_impl(&mut self, types: &TypeInfo, interning: &InterningInfo, def_path: DefPath) {
        let id = self.register_type(def_path);
        let (trait_def_path, adt_def_path) = types.get_impl_related_def_paths(&def_path);
        let adt_id = self.type_registry[&adt_def_path];
        let trait_id = match trait_def_path {
            Some(def_path) => Some(self.type_registry[&def_path]),
            None => None,
        };
        let relative_def_id = interning.def_path_to_string(&def_path);
        self.impls.push(Impl {
            id,
            adt_id,
            trait_id,
            relative_def_id,
        });
    }
    pub fn save(&self, path: &std::path::Path) {
        let mut file =
            File::create(path).unwrap_or_else(|e| panic!("Unable to create {:?}: {}", path, e));
        serde_json::to_writer_pretty(&mut file, self)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    }
}
