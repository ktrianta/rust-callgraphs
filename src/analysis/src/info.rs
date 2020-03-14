use corpus_database::tables::{InterningTables, Tables};
use corpus_database::types::*;
use std::collections::{HashMap, HashSet};

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
    type_to_adt_def_path: HashMap<Type, DefPath>,
    // Mapping from Item to DefPath.
    item_to_def_path: HashMap<Item, DefPath>,
    // Mapping from Trait DefPath to info.
    traits: HashMap<DefPath, (InternedString, Visibility, Module)>,
    // Mapping from Impl DefPath to Trait DefPath and Type.
    impls: HashMap<DefPath, (Option<DefPath>, Type)>,
    // Mapping from Trait DefPath to vector of its Impl DefPaths.
    pub trait_to_impls: HashMap<DefPath, Vec<DefPath>>,
    // Mapping from Trait Impl DefPath to mapping of Item Name to DefPath.
    pub trait_impl_to_items: HashMap<DefPath, HashMap<InternedString, DefPath>>,
    // Mapping from Trait Item DefPath to (Item Name, Item Defaultness, Trait DefPath).
    pub trait_items: HashMap<DefPath, (InternedString, Defaultness, DefPath)>,

    types_primitive: HashMap<Type, TyPrimitive>,
    types_slice: HashMap<Type, Type>,
    types_array: HashMap<Type, Type>,
    types_raw_ptr: HashMap<Type, (Type, Mutability)>,
    types_ref: HashMap<Type, (Type, Mutability)>,
    types_dynamic_trait: HashMap<Type, DefPath>,
    types_tuple: HashSet<Type>,
    types_tuple_elements: HashMap<Type, Vec<(TupleFieldIndex, Type)>>,
    types_projection: HashMap<Type, (DefPath, DefPath)>,
    types_param: HashMap<Type, String>,
}

impl TypeInfo {
    pub fn new(tables: &Tables) -> Self {
        let mut item_to_def_path = HashMap::new();
        let mut adts = HashMap::new();
        let mut impls = HashMap::new();
        let mut type_to_adt_def_path = HashMap::new();
        for (item, typ, def_path, module, name, visibility) in tables.relations.type_defs.iter() {
            item_to_def_path.insert(*item, *def_path);
            type_to_adt_def_path.insert(*typ, *def_path);
            adts.insert(*def_path, (*name, *visibility, *module));
        }
        for (typ, def_path, _, _, _) in tables.relations.types_adt_def.iter() {
            type_to_adt_def_path.insert(*typ, *def_path);
        }
        for (def_path, item, _, _, _, _, _, _, typ) in tables.relations.impl_definitions.iter() {
            item_to_def_path.insert(*item, *def_path);
            impls.insert(*def_path, (None, *typ));
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
            // If the implementation is a trait implementation update it in the mapping.
            impls.insert(impl_def_path, (Some(*trait_def_path), *typ));
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
        let mut types_primitive = HashMap::new();
        for (typ, primitive) in tables.relations.types_primitive.iter() {
            types_primitive.insert(*typ, *primitive);
        }
        let mut types_slice = HashMap::new();
        for (typ, element_type) in tables.relations.types_slice.iter() {
            types_slice.insert(*typ, *element_type);
        }
        let mut types_array = HashMap::new();
        for (typ, element_type) in tables.relations.types_array.iter() {
            types_array.insert(*typ, *element_type);
        }
        let mut types_raw_ptr = HashMap::new();
        for (raw_ptr_type, typ, mutability) in tables.relations.types_raw_ptr.iter() {
            types_raw_ptr.insert(*raw_ptr_type, (*typ, *mutability));
        }
        let mut types_ref = HashMap::new();
        for (ref_type, typ, mutability) in tables.relations.types_ref.iter() {
            types_ref.insert(*ref_type, (*typ, *mutability));
        }
        let mut types_dynamic_trait = HashMap::new();
        for (typ, def_path) in tables.relations.types_dynamic_trait.iter() {
            types_dynamic_trait.insert(*typ, *def_path);
        }
        for (typ, def_path) in tables.relations.types_dynamic_auto_trait.iter() {
            types_dynamic_trait.insert(*typ, *def_path);
        }
        let mut types_tuple = HashSet::new();
        for (typ,) in tables.relations.types_tuple.iter() {
            types_tuple.insert(*typ);
        }
        let mut types_tuple_elements: HashMap<Type, Vec<(TupleFieldIndex, Type)>> = HashMap::new();
        for (typ, index, element_type) in tables.relations.types_tuple_element.iter() {
            if let Some(elements) = types_tuple_elements.get_mut(typ) {
                elements.push((*index, *element_type));
                elements.sort();
            } else {
                types_tuple_elements.insert(*typ, vec![(*index, *element_type)]);
            }
        }
        let mut types_param = HashMap::new();
        for (typ, _, param_type) in tables.relations.types_param.iter() {
            types_param.insert(*typ, tables.interning_tables.strings[*param_type].clone());
        }
        let mut types_projection = HashMap::new();
        for (typ, def_path, item_def_path) in tables.relations.types_projection.iter() {
            types_projection.insert(*typ, (*def_path, *item_def_path));
        }
        Self {
            adts,
            type_to_adt_def_path,
            item_to_def_path,
            traits,
            trait_to_impls,
            impls,
            trait_impl_to_items,
            trait_items,
            types_primitive,
            types_slice,
            types_array,
            types_raw_ptr,
            types_ref,
            types_dynamic_trait,
            types_tuple,
            types_tuple_elements,
            types_param,
            types_projection
        }
    }
    pub fn iter_adt_types(&self) -> impl Iterator<Item = &Type> {
        self.type_to_adt_def_path.iter().map(|(typ, _)| typ)
    }
    pub fn iter_trait_def_paths(&self) -> impl Iterator<Item = &DefPath> {
        self.traits.iter().map(|(def_path, (_, _, _))| def_path)
    }
    pub fn iter_impl_def_paths(&self) -> impl Iterator<Item = &DefPath> {
        self.impls.iter().map(|(def_path, (_, _))| def_path)
    }
    pub fn get_impl_types(&self, def_path: &DefPath) -> (Option<DefPath>, Type) {
        self.impls[def_path]
    }
    pub(crate) fn resolve_type(&self, typ: &Type, interning: &InterningInfo) -> (String, Option<DefPath>) {
        if let Some(def_path) = self.type_to_adt_def_path.get(typ) {
            (Self::def_path_to_type_name(def_path, interning), Some(*def_path))
        } else if let Some(primitive) = self.types_primitive.get(typ) {
            (Self::primitive_to_string(primitive), None)
        } else if let Some(element_type) = self.types_slice.get(typ) {
            let (string_id, opt_def_path) = self.resolve_type(element_type, interning);
            (format!("[{}]", string_id), opt_def_path)
        } else if let Some(element_type) = self.types_array.get(typ) {
            let (string_id, opt_def_path) = self.resolve_type(element_type, interning);
            (format!("[{}]", string_id), opt_def_path)
        } else if let Some((typ, mutability)) = self.types_raw_ptr.get(typ) {
            let (string_id, opt_def_path) = self.resolve_type(typ, interning);
            match mutability.to_string().as_ref() {
                "Mutable" => (format!("*mut {}", string_id), opt_def_path),
                _ => (format!("*const {}", string_id), opt_def_path),
            }
        } else if let Some((typ, mutability)) = self.types_ref.get(typ) {
            let (string_id, opt_def_path) = self.resolve_type(typ, interning);
            (format!("&{} {}", Self::mutability_modifier_to_string(mutability), string_id), opt_def_path)
        } else if let Some(def_path) = self.types_dynamic_trait.get(typ) {
            (format!("dyn {}", Self::def_path_to_type_name(def_path, interning)), Some(*def_path))
        } else if let Some(typ) = self.types_tuple.get(typ) {
            if let Some(elements) = self.types_tuple_elements.get(typ) {
                let mut tuple_string = String::from("(");
                for (_, typ) in elements {
                    let (string_id, _) = self.resolve_type(typ, interning);
                    tuple_string.push_str(&string_id);
                    tuple_string.push_str(", ");
                }
                tuple_string.push_str(")");
                (tuple_string, None)
            } else {
                ("()".to_string(), None)
            }
        } else if let Some(param_type) = self.types_param.get(typ) {
            (format!("{}", param_type), None)
        } else if let Some((def_path, item_def_path)) = self.types_projection.get(typ) {
            (interning.def_path_to_string(item_def_path), Some(*def_path))
        } else {
            ("Unknown".to_string(), None)
        }
    }
    fn def_path_to_type_name(def_path: &DefPath, interning: &InterningInfo) -> String {
        let def_path_string = interning.def_path_to_string(&def_path);
        let mut tokens: Vec<&str> = def_path_string.split("::").collect();
        if let Some(string_id) = tokens.pop() {
            let len = string_id.len();
            let mut string_id = string_id.to_string();
            string_id.truncate(len-3);
            string_id
        } else {
            "null".to_string()
        }
    }
    fn mutability_modifier_to_string(mutability: &Mutability) -> String {
        match mutability.to_string().as_ref() {
            "Mutable" => String::from("mut"),
            "Const" => String::from("const"),
            _ => String::from(""),
        }
    }
    fn primitive_to_string(typ: &TyPrimitive) -> String {
        match typ.to_string().as_ref() {
            "Bool" => String::from("bool"),
            "Char" => String::from("char"),
            "Isize" => String::from("isize"),
            "I8" => String::from("i8"),
            "I16" => String::from("i16"),
            "I32" => String::from("i32"),
            "I64" => String::from("i64"),
            "I128" => String::from("i128"),
            "Usize" => String::from("usize"),
            "U8" => String::from("u8"),
            "U16" => String::from("u16"),
            "U32" => String::from("u32"),
            "U64" => String::from("u64"),
            "U128" => String::from("u128"),
            "F32" => String::from("f32"),
            "F64" => String::from("f64"),
            "Str" => String::from("string"),
            "Never" => String::from("!"),
            _ => String::from(""),
        }
    }
}
