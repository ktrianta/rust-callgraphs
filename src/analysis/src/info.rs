use corpus_database::tables::{InterningTables, Tables};
use corpus_database::types::*;
use std::collections::{HashMap, HashSet};

pub(crate) struct InterningInfo<'a> {
    package_info: Vec<(InternedString, InternedString)>,
    package_info_registry: HashMap<CrateHash, usize>,
    interning_tables: &'a InterningTables,
}

impl<'a> InterningInfo<'a> {
    pub fn new(interning_tables: &'a InterningTables) -> Self {
        let mut package_info = Vec::new();
        let mut package_info_registry = HashMap::new();
        for (_, (pkg, version, _, crate_hash, _)) in interning_tables.builds.iter() {
            let pkg_name_interned_string = interning_tables.package_names[*pkg];
            let pkg_version_interned_string = interning_tables.package_versions[*version];
            if package_info_registry.get(crate_hash).is_none() {
                let id = package_info.len();
                package_info_registry.insert(*crate_hash, id);
                package_info.push((
                    pkg_name_interned_string,
                    pkg_version_interned_string,
                ));
            }
        }
        Self {
            package_info,
            package_info_registry,
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
    pub fn def_path_to_package(&self, def_path: &DefPath) -> Option<(String, String)> {
        let (_, crate_hash, _, _, _) = self.interning_tables.def_paths[*def_path];
        if let Some(index) = self.package_info_registry.get(&crate_hash) {
            let (pkg_name_id, pkg_version_id) = self.package_info[*index];
            let pkg_name = self.interning_tables.strings[pkg_name_id].clone();
            let pkg_version = self.interning_tables.strings[pkg_version_id].clone();
            Some((pkg_name, pkg_version))
        } else {
            None
        }
    }
}

pub(crate) struct FunctionsInfo<'a> {
    functions: HashMap<DefPath, (Module, Visibility, Option<SpanLocation>)>,
    function_to_impl_item: HashMap<DefPath, Item>,
    function_to_trait_item: HashMap<DefPath, Item>,
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
        let mut function_to_impl_item = HashMap::new();
        for (impl_id, function_def_path, _) in tables.relations.trait_impl_items.iter() {
            function_to_impl_item.insert(*function_def_path, *impl_id);
        }
        let mut function_to_trait_item = HashMap::new();
        for (trait_id, function_def_path, _, _) in tables.relations.trait_items.iter() {
            function_to_trait_item.insert(*function_def_path, *trait_id);
        }
        Self {
            functions,
            function_to_impl_item,
            function_to_trait_item,
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
    pub fn is_function_externally_visible(
        &self,
        def_path: &DefPath,
        modules: &ModulesInfo,
        types: &TypeInfo,
    ) -> bool {
        if let Some((module, visibility, _)) = self.functions.get(def_path) {
            if let Some(trait_item) = self.function_to_trait_item.get(def_path) {
                types.is_trait_item_externally_visible(trait_item, modules)
            } else if let Some(impl_item) = self.function_to_impl_item.get(def_path) {
                if types.is_trait_impl(impl_item) {
                    types.is_impl_item_externally_visible(impl_item, modules)
                } else {
                    match visibility {
                        Visibility::Public => {
                            types.is_impl_item_externally_visible(impl_item, modules)
                        }
                        _ => false,
                    }
                }
            } else {
                match visibility {
                    Visibility::Public => modules.is_module_externally_visible(module),
                    _ => false,
                }
            }
        } else {
            // If function definition is missing it is because it is defined in another package,
            // thus it is externally visible.
            true
        }
    }
}

pub(crate) struct ModulesInfo {
    modules: HashMap<Module, (DefPath, Visibility, Module)>,
    module_is_externally_visible: HashMap<Module, bool>,
}

impl ModulesInfo {
    pub fn new(tables: &Tables) -> Self {
        let mut crate_types = HashMap::new();
        for (build, crate_type) in tables.relations.build_crate_types.iter() {
            crate_types.insert(*build, tables.interning_tables.strings[*crate_type].clone());
        }
        // Mapping from root module to module's crate type, e.g., bin, lib, etc.
        let mut root_modules_to_crate_type = HashMap::new();
        for (build, root_module) in tables.relations.root_modules.iter() {
            if let Some(crate_type) = crate_types.get(build) {
                root_modules_to_crate_type.insert(*root_module, crate_type.clone());
            } else {
                // TODO: further investigate why rarely it happends that a build has no crate type.
                // The root modules (associated to these builds) that we have examined include only
                // private items and thus we treat these modules as binary ones.
                root_modules_to_crate_type.insert(*root_module, "bin".to_string());
            }
        }
        let mut modules = HashMap::new();
        for (def_path, parent_module, module, _, visibility, _) in
            tables.relations.submodules.iter()
        {
            modules.insert(*module, (*def_path, *visibility, *parent_module));
        }
        let module_is_externally_visible =
            Self::compute_modules_external_visibility(&root_modules_to_crate_type, &modules);
        Self {
            modules,
            module_is_externally_visible,
        }
    }
    // Returns a mapping that specifies if the module is externally visible or not.
    fn compute_modules_external_visibility(
        root_modules_to_crate_type: &HashMap<Module, String>,
        modules: &HashMap<Module, (DefPath, Visibility, Module)>,
    ) -> HashMap<Module, bool> {
        let mut module_is_externally_visible = HashMap::new();
        // Root modules' visibility depends on the type of the crate, i.e., bin or lib.
        // Binary crates are invisible, while library ones are visible.
        let is_crate_type_externally_visible = |crate_type: &str| match crate_type {
            // Below we list all available crate types.
            "bin" => false,
            "rlib" => true,
            "dylib" => true,
            "staticlib" => true,
            "cdylib" => true,
            "proc-macro" => true,
            // "lib" should not occur as it appears as "rlib".
            "lib" => true,
            // There are no other crate type options (at least currently).
            _ => unreachable!(),
        };
        // Compute root modules external visibility.
        for (module, crate_type) in root_modules_to_crate_type {
            let is_externally_visible = is_crate_type_externally_visible(crate_type);
            module_is_externally_visible.insert(*module, is_externally_visible);
        }
        // Compute submodules external visibility by looping over them.
        for (module, (_, visibility, parent)) in modules {
            if module_is_externally_visible.get(module).is_none() {
                let is_public = Self::compute_submodule_external_visibility(
                    *visibility,
                    *parent,
                    modules,
                    &mut module_is_externally_visible,
                );
                module_is_externally_visible.insert(*module, is_public);
            }
        }
        module_is_externally_visible
    }
    // Returns the external visibility of a module when provided by its visibility and its parent.
    // While computing the module's external visibility, it also does so for all parent modules.
    // External visibility for all modules is cached in module_is_externally_visible mapping.
    fn compute_submodule_external_visibility(
        visibility: Visibility,
        parent: Module,
        modules: &HashMap<Module, (DefPath, Visibility, Module)>,
        module_is_externally_visible: &mut HashMap<Module, bool>,
    ) -> bool {
        let is_externally_visible = |visibility| match visibility {
            Visibility::Public => true,
            _ => false,
        };
        if let Some(is_parent_externally_visible) = module_is_externally_visible.get(&parent) {
            // Externally visibility has already been computed in the past for this parent module.
            // NOTE: The visibility of all parent modules that are root modules should have been
            //       computed prior to calling the compute_submodule_external_visibility function.
            *is_parent_externally_visible && is_externally_visible(visibility)
        } else {
            if let Some((_, parents_visibility, parents_parent)) = modules.get(&parent) {
                let is_parent_externally_visible = Self::compute_submodule_external_visibility(
                    *parents_visibility,
                    *parents_parent,
                    modules,
                    module_is_externally_visible,
                );
                module_is_externally_visible.insert(parent, is_parent_externally_visible);
                is_parent_externally_visible && is_externally_visible(visibility)
            } else {
                // If the parent module does not have a parent, it is a root module. Root modules
                // external visibility should have been computed prior to calling this function and
                // should be cached in module_is_externally_visible mapping.
                panic!()
            }
        }
    }
    pub fn is_module_externally_visible(&self, module: &Module) -> bool {
        if let Some(is_externally_visible) = self.module_is_externally_visible.get(module) {
            *is_externally_visible
        } else {
            // If module definition is missing it is because it is defined in another package, thus
            // it is externally visible.
            true
        }
    }
}

pub(crate) struct TypeInfo {
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
            types_projection,
        }
    }
    fn is_trait_impl(&self, impl_item: &Item) -> bool {
        let impl_def_path = self.item_to_def_path[impl_item];
        match self.impls.get(&impl_def_path) {
            Some((Some(_), _)) => true,
            _ => false,
        }
    }
    pub(crate) fn is_type_externally_visible(&self, typ: &Type, modules: &ModulesInfo) -> bool {
        if let Some(def_path) = self.type_to_adt_def_path.get(typ) {
            self.is_adt_externally_visible(def_path, modules)
        } else if let Some(_) = self.types_primitive.get(typ) {
            true
        } else if let Some(element_type) = self.types_slice.get(typ) {
            self.is_type_externally_visible(element_type, modules)
        } else if let Some(element_type) = self.types_array.get(typ) {
            self.is_type_externally_visible(element_type, modules)
        } else if let Some((typ, _)) = self.types_raw_ptr.get(typ) {
            self.is_type_externally_visible(typ, modules)
        } else if let Some((typ, _)) = self.types_ref.get(typ) {
            self.is_type_externally_visible(typ, modules)
        } else if let Some(def_path) = self.types_dynamic_trait.get(typ) {
            self.is_trait_externally_visible(def_path, modules)
        } else if let Some(typ) = self.types_tuple.get(typ) {
            if let Some(elements) = self.types_tuple_elements.get(typ) {
                let mut is_visible = true;
                for (_, typ) in elements {
                    is_visible &= self.is_type_externally_visible(typ, modules);
                }
                is_visible
            } else {
                true
            }
        } else if let Some(_) = self.types_param.get(typ) {
            // TODO: Investigate further. Conservatively consider these externally visible for now.
            true
        } else if let Some((trait_def_path, _)) = self.types_projection.get(typ) {
            // Associated type is externally visible if trait is.
            self.is_trait_externally_visible(trait_def_path, modules)
        } else {
            true
        }
    }
    fn is_trait_item_externally_visible(&self, trait_item: &Item, modules: &ModulesInfo) -> bool {
        if let Some(trait_def_path) = self.item_to_def_path.get(trait_item) {
            self.is_trait_externally_visible(&trait_def_path, modules)
        } else {
            // If trait definition is missing it is because it is defined in another package, thus
            // it is externally visible.
            true
        }
    }
    fn is_impl_item_externally_visible(&self, impl_item: &Item, modules: &ModulesInfo) -> bool {
        let impl_def_path = self.item_to_def_path[impl_item];
        if let Some((opt_trait_def_path, typ)) = self.impls.get(&impl_def_path) {
            if let Some(trait_def_path) = opt_trait_def_path {
                self.is_trait_externally_visible(&trait_def_path, modules)
                    && self.is_type_externally_visible(typ, modules)
            } else {
                self.is_type_externally_visible(typ, modules)
            }
        } else {
            panic!("Implementation visibility: missing implementation definition.");
        }
    }
    fn is_trait_externally_visible(&self, def_path: &DefPath, modules: &ModulesInfo) -> bool {
        if let Some((_, visibility, module)) = self.traits.get(def_path) {
            match visibility {
                Visibility::Public => modules.is_module_externally_visible(module),
                _ => false,
            }
        } else {
            // If trait definition is missing it is because it is defined in another package, thus
            // it is externally visible.
            true
        }
    }
    fn is_adt_externally_visible(&self, def_path: &DefPath, modules: &ModulesInfo) -> bool {
        if let Some((_, visibility, module)) = self.adts.get(def_path) {
            match visibility {
                Visibility::Public => modules.is_module_externally_visible(module),
                _ => false,
            }
        } else {
            // If adt definition is missing it is because it is defined in another package, thus
            // it is externally visible.
            true
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
    pub(crate) fn resolve_type(
        &self,
        typ: &Type,
        interning: &InterningInfo,
    ) -> (String, Option<DefPath>) {
        if let Some(def_path) = self.type_to_adt_def_path.get(typ) {
            (
                Self::def_path_to_type_name(def_path, interning),
                Some(*def_path),
            )
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
            (
                format!(
                    "&{} {}",
                    Self::mutability_modifier_to_string(mutability),
                    string_id
                ),
                opt_def_path,
            )
        } else if let Some(def_path) = self.types_dynamic_trait.get(typ) {
            (
                format!("dyn {}", Self::def_path_to_type_name(def_path, interning)),
                Some(*def_path),
            )
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
        } else if let Some((trait_def_path, item_def_path)) = self.types_projection.get(typ) {
            (
                interning.def_path_to_string(item_def_path),
                Some(*trait_def_path),
            )
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
            string_id.truncate(len - 3);
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
