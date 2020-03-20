use crate::ast;
use log::debug;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};

pub(crate) fn generate_tokens(schema: ast::DatabaseSchema) -> TokenStream {
    let types = generate_types(&schema);
    let tables = generate_interning_tables(&schema);
    let relations = generate_relations(&schema);
    let (counters, counter_functions) = generate_counters(&schema);
    let registration_functions = generate_registration_functions(&schema);
    let load_save_functions = generate_load_save_functions(&schema);
    let loader_functions = generate_loader_functions(&schema);
    let merge_functions = generate_merge_functions(&schema);
    quote! {
        pub mod types {
            use serde_derive::{Deserialize, Serialize};
            #types
        }
        pub mod tables {
            use std::path::{Path, PathBuf};
            use std::collections::HashMap;
            use failure::Error;
            use serde_derive::{Deserialize, Serialize};
            use super::types::*;
            #tables
            #relations
            #counters

            #[derive(Default, Deserialize, Serialize)]
            pub struct Tables {
                /// Relations between Rust program elements.
                pub relations: Relations,
                /// Counters used for generating ids.
                pub counters: Counters,
                /// Interning tables that link typed ids to untyped interning ids.
                pub interning_tables: InterningTables,
            }

            impl Tables {
                #registration_functions
            }

            impl Tables {
                #counter_functions
            }

            impl Tables {
                #merge_functions
            }

            #load_save_functions

            pub struct Loader {
                pub(crate) database_root: PathBuf,
            }

            impl Loader {
                pub fn new(database_root: PathBuf) -> Self {
                    Self { database_root }
                }
                #loader_functions
            }
        }
    }
}

fn generate_types(schema: &ast::DatabaseSchema) -> TokenStream {
    let id_types = generate_id_types(schema);
    let enum_types = generate_enum_types(schema);
    quote! {
        #id_types
        #enum_types
    }
}

fn generate_id_types(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut tokens = TokenStream::new();
    for ast::CustomId {
        ref name,
        ref typ,
        items,
    } in &schema.custom_ids
    {
        tokens.extend(generate_id_decl(name, typ));
        for item in items {
            tokens.extend(quote! { #item });
        }
    }
    for ast::IncrementalId {
        ref name, ref typ, ..
    } in &schema.incremental_ids
    {
        tokens.extend(generate_id_decl(name, typ));
    }
    for ast::InterningTable {
        key: ast::InternedId { ref name, ref typ },
        ..
    } in &schema.interning_tables
    {
        tokens.extend(generate_id_decl(name, typ));
    }
    tokens
}

fn is_numeric_type(typ: &syn::Type) -> bool {
    if let syn::Type::Path(syn::TypePath {
        qself: None,
        ref path,
    }) = typ
    {
        path.is_ident("u8")
            || path.is_ident("u16")
            || path.is_ident("u32")
            || path.is_ident("u64")
            || path.is_ident("usize")
    } else {
        false
    }
}

fn generate_id_decl(name: &syn::Ident, typ: &syn::Type) -> TokenStream {
    let mut tokens = quote! {
        #[derive(
            Debug, Eq, PartialEq, Hash, Clone, Copy,
            Deserialize, Serialize, PartialOrd, Ord, Default
        )]
        pub struct #name(pub(super) #typ);
    };
    if is_numeric_type(typ) {
        tokens.extend(quote! {
            impl From<#typ> for #name {
                fn from(value: #typ) -> Self {
                    Self(value)
                }
            }

            impl From<usize> for #name {
                fn from(value: usize) -> Self {
                    Self(value as #typ)
                }
            }

            impl Into<usize> for #name {
                fn into(self) -> usize {
                    self.0 as usize
                }
            }

            impl #name {
                /// Shift the id by given `offset`.
                pub fn shift(&self, offset: #typ) -> Self {
                    Self(self.0.checked_add(offset).expect("Overflow!"))
                }
                /// Get the underlying index.
                pub fn index(&self) -> usize {
                    self.0 as usize
                }
            }
        });
    }
    tokens
}

fn generate_enum_types(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut tokens = TokenStream::new();
    for ast::Enum {
        ref item,
        ref default,
    } in &schema.enums
    {
        let enum_name = item.ident.clone();
        let enum_tokens = quote! {

            #[derive(Debug, Eq, PartialEq, Hash, Clone, Copy, Deserialize, Serialize, PartialOrd, Ord)]
            pub #item

            impl Default for #enum_name {
                fn default() -> Self {
                    #enum_name::#default
                }
            }

            impl std::fmt::Display for #enum_name {
                fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    write!(f, "{:?}", self)
                }
            }
        };
        tokens.extend(enum_tokens);
    }
    tokens
}

fn generate_interning_tables(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut fields = TokenStream::new();
    let mut conversions = TokenStream::new();
    let mut longest = 2;
    for ast::InterningTable {
        ref name,
        ref key,
        ref value,
    } in &schema.interning_tables
    {
        let key_type = &key.name;
        let field = quote! {
            pub #name: InterningTable<#key_type, #value>,
        };
        fields.extend(field);
        if let syn::Type::Tuple(syn::TypeTuple { elems, .. }) = value {
            if longest < elems.len() {
                longest = elems.len();
            }
        }
    }
    let mut args = TokenStream::new();
    let mut type_args = TokenStream::new();
    let mut type_constraints = TokenStream::new();
    for i in 0..longest {
        let arg = syn::Ident::new(&format!("v{}", i), Span::call_site());
        let type_arg = syn::Ident::new(&format!("V{}", i), Span::call_site());
        args.extend(quote! {#arg,});
        type_args.extend(quote! {#type_arg,});
        type_constraints.extend(quote! {
            #type_arg: crate::data_structures::InterningTableValue,
        });
        conversions.extend(quote! {
            impl<K, #type_args> Into<Vec<(K, #type_args)>> for InterningTable<K, (#type_args)>
                where
                    K: crate::data_structures::InterningTableKey,
                    #type_constraints
            {
                fn into(self) -> Vec<(K, #type_args)> {
                    self.contents.into_iter().enumerate().map(|(i, (#args))| {
                        (i.into(), #args)
                    }).collect()
                }
            }

        });
    }
    quote! {
        use crate::data_structures::InterningTable;
        #[derive(Default, Deserialize, Serialize)]
        /// Interning tables.
        pub struct InterningTables {
            #fields
        }
        #conversions
    }
}

fn generate_relations(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut fields = TokenStream::new();
    for ast::Relation {
        ref name,
        ref parameters,
    } in &schema.relations
    {
        let mut parameter_tokens = TokenStream::new();
        for ast::RelationParameter { typ, .. } in parameters {
            parameter_tokens.extend(quote! {#typ,});
        }
        fields.extend(quote! {
            pub #name: Relation<(#parameter_tokens)>,
        });
    }
    quote! {
        use crate::data_structures::Relation;
        #[derive(Default, Deserialize, Serialize)]
        /// Relations between various entities of the Rust program.
        pub struct Relations {
            #fields
        }
    }
}

fn generate_counters(schema: &ast::DatabaseSchema) -> (TokenStream, TokenStream) {
    let mut fields = TokenStream::new();
    let mut getter_functions = TokenStream::new();
    let mut counter_functions = TokenStream::new();
    let mut default_impls = TokenStream::new();
    for id in &schema.incremental_ids {
        let ast::IncrementalId {
            ref name,
            ref typ,
            ref constants,
        } = id;
        let field_name = id.get_field_name();
        fields.extend(quote! {
            pub(crate) #field_name: #typ,
        });
        let get_fresh_name = id.get_generator_fn_name();
        getter_functions.extend(quote! {
            fn #get_fresh_name(&mut self) -> #name {
                let value = self.#field_name.into();
                self.#field_name += 1;
                value
            }
        });
        counter_functions.extend(quote! {
            pub fn #get_fresh_name(&mut self) -> #name {
                self.counters.#get_fresh_name()
            }
        });
        for constant in constants {
            let get_constant_name = constant.get_getter_name();
            let value = &constant.value;
            getter_functions.extend(quote! {
                fn #get_constant_name(&mut self) -> #name {
                    #value.into()
                }
            });
            counter_functions.extend(quote! {
                pub fn #get_constant_name(&mut self) -> #name {
                    self.counters.#get_constant_name()
                }
            });
        }
        let default_value = id.get_default_value();
        default_impls.extend(quote! {
           #field_name: #default_value,
        });
    }
    let counters = quote! {
        #[derive(Deserialize, Serialize)]
        /// Counters for generating unique identifiers.
        pub struct Counters {
            #fields
        }
        impl Counters {
            #getter_functions
        }
        impl Default for Counters {
            fn default() -> Self {
                Self {
                    #default_impls
                }
            }
        }
    };
    (counters, counter_functions)
}

fn generate_registration_functions(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut functions = TokenStream::new();
    for table in &schema.interning_tables {
        if let syn::Type::Tuple(ref tuple) = table.value {
            let function = generate_intern_tuple_registration(tuple, table, schema);
            functions.extend(function);
        } else {
            let function = generate_intern_value_registration(table, schema);
            functions.extend(function);
        };
    }
    for relation in &schema.relations {
        let function = generate_relation_registration(&relation, schema);
        functions.extend(function);
    }
    functions
}

#[derive(Debug, Clone)]
struct NameGenerator {
    name: String,
    counter: u32,
}

impl NameGenerator {
    fn new(name: String) -> Self {
        Self {
            name: name,
            counter: 0,
        }
    }
    fn inc(&mut self) {
        self.counter += 1;
    }
    fn get_ident(&self) -> syn::Ident {
        syn::parse_str(&format!("{}_{}", self.name, self.counter)).unwrap()
    }
}

/// Find a type that is not an interning key and an interning path
/// from it to the target type.
fn generate_interning_type(
    target_type: &syn::Type,
    schema: &ast::DatabaseSchema,
    name_generator: &mut NameGenerator,
) -> (syn::Ident, syn::Type, TokenStream) {
    let var_name = name_generator.get_ident();
    let mut found_type = None;
    let mut tokens = TokenStream::new();
    for table in &schema.interning_tables {
        if &table.get_key_type() == target_type {
            assert!(found_type.is_none(), "Ambigous interning tables");
            if let syn::Type::Tuple(_) = table.value {
                continue;
            }
            name_generator.inc();
            // let new_target_type = syn::Type::Path(syn::TypePath { qself: None, path: table.key.name.clone().into() });
            let (new_name, new_found_type, prefix) =
                generate_interning_type(&table.value, schema, name_generator);
            found_type = Some(new_found_type);
            tokens.extend(prefix);
            let table_name = &table.name;
            tokens.extend(quote! {
                let #var_name = self.interning_tables.#table_name.intern(#new_name);
            });
        }
    }
    if let Some(final_type) = found_type {
        (var_name, final_type, tokens)
    } else {
        (var_name, target_type.clone(), tokens)
    }
}

fn generate_intern_tuple_registration(
    values: &syn::TypeTuple,
    table: &ast::InterningTable,
    schema: &ast::DatabaseSchema,
) -> TokenStream {
    let registration_function_name = table.get_registration_function_name();
    let key_type = table.get_key_type();
    let mut interning_tokens = TokenStream::new();
    let mut param_tokens = TokenStream::new();
    let mut arg_tokens = TokenStream::new();

    for value_type in values.elems.pairs().map(|pair| pair.into_value()) {
        let template = format!("value_{}", value_type.to_token_stream()).to_lowercase();
        let mut name_generator = NameGenerator::new(template);
        let (final_name, param_type, tokens) =
            generate_interning_type(&value_type, schema, &mut name_generator);
        let param_name = name_generator.get_ident();
        param_tokens.extend(quote! {
            #param_name: #param_type,
        });
        interning_tokens.extend(tokens);
        arg_tokens.extend(quote! {
            #final_name,
        });
    }
    let table_name = &table.name;
    quote! {
        pub fn #registration_function_name(&mut self, #param_tokens) -> #key_type {
            #interning_tokens
            self.interning_tables.#table_name.intern((#arg_tokens))
        }
    }
}

fn generate_intern_value_registration(
    table: &ast::InterningTable,
    schema: &ast::DatabaseSchema,
) -> TokenStream {
    let registration_function_name = table.get_registration_function_name();
    let key_type = table.get_key_type();

    let mut name_generator = NameGenerator::new(String::from("value"));
    let (final_name, param_type, tokens) =
        generate_interning_type(&key_type, schema, &mut name_generator);
    let param_name = name_generator.get_ident();
    quote! {
        pub fn #registration_function_name(&mut self, #param_name: #param_type) -> #key_type {
            #tokens
            #final_name
        }
    }
}

fn generate_relation_registration(
    relation: &ast::Relation,
    schema: &ast::DatabaseSchema,
) -> TokenStream {
    let registration_function_name = relation.get_registration_function_name();
    let mut param_tokens = TokenStream::new();
    let mut return_tokens = TokenStream::new();
    let mut return_type_tokens = TokenStream::new();
    let mut interning_tokens = TokenStream::new();
    let mut arg_tokens = TokenStream::new();
    for ast::RelationParameter {
        name,
        typ,
        is_autogenerated,
    } in &relation.parameters
    {
        if *is_autogenerated {
            let id = schema
                .find_incremental_id(typ)
                .expect("Only incremental IDs can be marked with `auto`.");
            let generator_fn_name = id.get_generator_fn_name();
            interning_tokens.extend(quote! {
                let #name = self.counters.#generator_fn_name();
            });
            return_type_tokens.extend(quote! {#typ,});
            return_tokens.extend(quote! {#name,});
            arg_tokens.extend(quote! {#name,});
        } else {
            let mut name_generator = NameGenerator::new(name.to_string());
            let (final_name, param_type, tokens) =
                generate_interning_type(&typ, schema, &mut name_generator);
            let param_name = name_generator.get_ident();
            param_tokens.extend(quote! {
                #param_name: #param_type,
            });
            interning_tokens.extend(tokens);
            arg_tokens.extend(quote! {#final_name,});
        }
    }
    let table_name = &relation.name;
    quote! {
        pub fn #registration_function_name(&mut self, #param_tokens) -> (#return_type_tokens) {
            #interning_tokens
            self.relations.#table_name.insert((#arg_tokens));
            (#return_tokens)
        }
    }
}

use std::collections::HashMap;

fn generate_merge_functions(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut counter = 0;
    let mut get_fresh_name = || -> syn::Ident {
        counter += 1;
        syn::parse_str(&format!("tmp_{}", counter)).unwrap()
    };
    let mut interning_remap = HashMap::new();
    let mut tokens = TokenStream::new();
    // let mut tuple_arguments = TokenStream::new();
    let mut tuple_iterning_tables = Vec::new();
    for table in &schema.interning_tables {
        let name = &table.name;
        println!("{:?}", table);
        if let syn::Type::Tuple(ref values) = table.value {
            tuple_iterning_tables.push((table, values));
        } else {
            let arg_remap = if let Some(map) = interning_remap.get(&table.value) {
                quote! {
                    let new_value = #map[&value];
                }
            } else {
                quote! {
                    let new_value = value;
                }
            };
            tokens.extend(quote! {
                let #name: HashMap<_, _> = other
                   .interning_tables
                   .#name
                   .into_iter()
                   .map(|(key, value)| {
                       #arg_remap
                       let new_key = self.interning_tables.#name.intern(new_value);
                       (key, new_key)
                   })
                   .collect();
            });
            interning_remap.insert(table.get_key_type(), name);
        }
    }
    for (table, values) in tuple_iterning_tables {
        let name = &table.name;
        let mut args = TokenStream::new();
        let mut params = TokenStream::new();
        let mut arg_remap = TokenStream::new();
        for value_type in values.elems.pairs().map(|pair| pair.into_value()) {
            let param = get_fresh_name();
            let arg = get_fresh_name();
            if let Some(map) = interning_remap.get(value_type) {
                arg_remap.extend(quote! {
                    let #arg = #map[&#param];
                });
            } else {
                debug!("Not an interned type: {:?}", value_type);
                assert!(schema.get_type_kind(value_type).is_custom_id());
                arg_remap.extend(quote! {
                    let #arg = #param;
                });
            }
            args.extend(quote! {#arg,});
            params.extend(quote! {#param,})
        }
        tokens.extend(quote! {
            let #name: HashMap<_, _> = other
                .interning_tables
                .#name
                .into_iter()
                .map(|(key, (#params))| {
                    #arg_remap
                    let new_key = self.interning_tables.#name.intern((#args));
                    (key, new_key)
                })
                .collect();
        });
    }
    for relation in &schema.relations {
        let name = &relation.name;
        let mut params = TokenStream::new();
        let mut params_remap = TokenStream::new();
        let mut new_params = TokenStream::new();
        for param in &relation.parameters {
            let param_name = &param.name;
            params.extend(quote! { #param_name, });
            let new_name = get_fresh_name();
            new_params.extend(quote! { #new_name, });
            match schema.get_type_kind(&param.typ) {
                ast::TypeKind::CustomId | ast::TypeKind::Enum | ast::TypeKind::RustType => {
                    params_remap.extend(quote! {
                        let #new_name = *#param_name;
                    });
                }
                ast::TypeKind::IncrementalId(id) => {
                    let counter_name = id.get_field_name();
                    let constant_count = id.constants.len();
                    let typ = &id.typ;
                    let shift = quote! {
                        #param_name.shift(
                            self.counters.#counter_name-(#constant_count as #typ)
                        )
                    };
                    if constant_count > 0 {
                        params_remap.extend(quote! {
                            let #new_name = if #param_name.index() >= #constant_count {
                                #shift
                            } else {
                                *#param_name
                            };
                        });
                    } else {
                        params_remap.extend(quote! {
                            let #new_name = #shift;
                        });
                    }
                }
                ast::TypeKind::InternedId(table) => {
                    let map = &table.name;
                    params_remap.extend(quote! {
                        let #new_name = #map[#param_name];
                    });
                }
            }
        }
        tokens.extend(quote! {
            for (#params) in other.relations.#name.iter() {
                #params_remap
                self.relations
                    .#name
                    .insert((#new_params));
            }
        });
    }
    for id in &schema.incremental_ids {
        let field_name = id.get_field_name();
        let constant_count = id.constants.len();
        let typ = &id.typ;
        tokens.extend(quote! {
            self.counters.#field_name +=
                other.counters.#field_name - (#constant_count as #typ);
        });
    }
    quote! {
        pub fn merge(&mut self, other: super::tables::Tables) {
            #tokens
        }
    }
}

fn generate_load_save_functions(schema: &ast::DatabaseSchema) -> TokenStream {
    let load_multifile_relations = load_multifile_relations_function(schema);
    let load_counters = load_counters_function();
    let load_interning_tables = load_multifle_interning_function(schema);
    let store_multifile_relations = store_multifile_relations_function(schema);
    let store_counters = store_counters_function();
    let store_interning_tables = store_multifle_interning_function(schema);
    quote! {
        impl Tables {
            pub fn load_multifile(
                database_root: &Path
            ) -> Result<Tables, Error> {
                let relations = load_multifile_relations(&database_root.join("relations"))?;
                let counters = load_counters(&database_root.join("counters.bincode"))?;
                let interning_tables = load_interning_tables(&database_root.join("interning"))?;
                Ok(Tables {
                    relations,
                    counters,
                    interning_tables,
                })
            }
            pub fn load_single_file(
                tables_file: &Path
            ) -> Result<Tables, Error> {
                crate::storage::load(tables_file)
            }
            pub fn store_multifile(&self, database_root: &Path) -> Result<(), Error> {
                let relations_path = database_root.join("relations");
                std::fs::create_dir_all(&relations_path)?;
                store_multifile_relations(&self.relations, &relations_path);
                let counters_path = database_root.join("counters.bincode");
                store_counters(&self.counters, &counters_path);
                let interning_tables_path = &database_root.join("interning");
                std::fs::create_dir_all(&interning_tables_path)?;
                store_multifile_interning_tables(
                    &self.interning_tables,
                    &interning_tables_path
                );
                Ok(())
            }
        }
        #load_multifile_relations
        #load_counters
        #load_interning_tables
        #store_multifile_relations
        #store_counters
        #store_interning_tables
    }
}

fn generate_loader_functions(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut tokens = TokenStream::new();
    for ast::Relation {
        ref name,
        ref parameters,
        ..
    } in &schema.relations
    {
        let file_name = format!("relations/{}.bincode", name);
        let load_fn_name = syn::Ident::new(&format!("load_{}", name), Span::call_site());
        let store_fn_name = syn::Ident::new(&format!("store_{}", name), Span::call_site());
        let mut types = TokenStream::new();
        for ast::RelationParameter { typ, .. } in parameters {
            types.extend(quote! {#typ,});
        }
        tokens.extend(quote! {
            pub fn #load_fn_name(&self) -> Vec<(#types)> {
                let relation: Relation<(#types)> = crate::storage::load(
                    &self.database_root.join(#file_name)
                ).unwrap();
                relation.into()
            }
            pub fn #store_fn_name(&self, facts: Vec<(#types)>) {
                let relation: Relation<(#types)> = facts.into();
                crate::storage::save(
                    &relation,
                    &self.database_root.join(#file_name)
                );
            }
        });
    }
    for ast::InterningTable {
        ref name,
        ref key,
        ref value,
    } in &schema.interning_tables
    {
        let file_name = format!("interning/{}.bincode", name);
        let fn_name = syn::Ident::new(&format!("load_{}", name), Span::call_site());
        let fn_name_as_vec = syn::Ident::new(&format!("load_{}_as_vec", name), Span::call_site());
        let key_type = &key.name;
        let mut types = TokenStream::new();
        types.extend(quote! {#key_type,});
        match value {
            syn::Type::Tuple(syn::TypeTuple { elems, .. }) => {
                for elem in elems {
                    types.extend(quote! {#elem,});
                }
            }
            _ => {
                types.extend(quote! {#value,});
            }
        }
        tokens.extend(quote! {
            pub fn #fn_name(&self) -> InterningTable<#key_type, #value> {
                crate::storage::load(
                    &self.database_root.join(#file_name)
                ).unwrap()
            }
            pub fn #fn_name_as_vec(&self) -> Vec<(#types)> {
                let table: InterningTable<#key_type, #value> = crate::storage::load(
                    &self.database_root.join(#file_name)
                ).unwrap();
                table.into()
            }
        });
    }
    tokens
}

fn load_multifile_relations_function(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut load_fields = TokenStream::new();
    for ast::Relation { ref name, .. } in &schema.relations {
        let file_name = format!("{}.bincode", name);
        load_fields.extend(quote! {
            #name: crate::storage::load(&path.join(#file_name))?,
        });
    }
    quote! {
        fn load_multifile_relations(path: &Path) -> Result<Relations, Error> {
            Ok(Relations {
                #load_fields
            })
        }
    }
}

fn store_multifile_relations_function(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut store_fields = TokenStream::new();
    for ast::Relation { ref name, .. } in &schema.relations {
        let file_name = format!("{}.bincode", name);
        store_fields.extend(quote! {
            crate::storage::save(&relations.#name, &path.join(#file_name));
        });
    }
    quote! {
        fn store_multifile_relations(
            relations: &Relations,
            path: &Path
        ) {
            #store_fields
        }
    }
}

fn load_counters_function() -> TokenStream {
    quote! {
        fn load_counters(path: &Path) -> Result<Counters, Error> {
            crate::storage::load(&path)
        }
    }
}

fn store_counters_function() -> TokenStream {
    quote! {
        fn store_counters(counters: &Counters, path: &Path) {
            crate::storage::save(counters, &path);
        }
    }
}

fn load_multifle_interning_function(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut load_fields = TokenStream::new();
    for ast::InterningTable { ref name, .. } in &schema.interning_tables {
        let file_name = format!("{}.bincode", name);
        load_fields.extend(quote! {
            #name: crate::storage::load(&path.join(#file_name))?,
        });
    }
    quote! {
        fn load_interning_tables(path: &Path) -> Result<InterningTables, Error> {
            Ok(InterningTables {
                #load_fields
            })
        }
    }
}

fn store_multifle_interning_function(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut store_fields = TokenStream::new();
    for ast::InterningTable { ref name, .. } in &schema.interning_tables {
        let file_name = format!("{}.bincode", name);
        store_fields.extend(quote! {
            crate::storage::save(&interning_tables.#name, &path.join(#file_name));
        });
    }
    quote! {
        fn store_multifile_interning_tables(
            interning_tables: &InterningTables,
            path: &Path
        ) {
            #store_fields
        }
    }
}
