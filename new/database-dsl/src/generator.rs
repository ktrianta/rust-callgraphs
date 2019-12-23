use crate::ast;
use proc_macro2::TokenStream;
use quote::{quote, ToTokens};

pub(crate) fn generate_tokens(schema: ast::DatabaseSchema) -> TokenStream {
    let types = generate_types(&schema);
    let tables = generate_interning_tables(&schema);
    let relations = generate_relations(&schema);
    let counters = generate_counters(&schema);
    let registration_functions = generate_registration_functions(&schema);
    quote! {
        pub mod types {
            use serde_derive::{Deserialize, Serialize};
            #types
        }
        pub mod tables {
            use serde_derive::{Deserialize, Serialize};
            use super::types::*;
            #tables
            #relations
            #counters

            #[derive(Default, Deserialize, Serialize)]
            pub struct Tables {
                /// Relations between Rust program elements.
                pub(crate) relations: Relations,
                /// Counters used for generating ids.
                pub(crate) counters: Counters,
                /// Interning tables that link typed ids to untyped interning ids.
                pub(crate) interning_tables: InterningTables,
            }

            impl Tables {
                #registration_functions
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
        ref name, ref typ, ..
    } in &schema.custom_ids
    {
        tokens.extend(generate_id_decl(name, typ));
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
        };
        tokens.extend(enum_tokens);
    }
    tokens
}

fn generate_interning_tables(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut fields = TokenStream::new();
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
    }
    quote! {
        use crate::data_structures::InterningTable;
        #[derive(Default, Deserialize, Serialize)]
        /// Interning tables.
        pub struct InterningTables {
            #fields
        }
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

fn generate_counters(schema: &ast::DatabaseSchema) -> TokenStream {
    let mut fields = TokenStream::new();
    let mut getter_functions = TokenStream::new();
    let mut default_impls = TokenStream::new();
    for id in &schema.incremental_ids {
        let ast::IncrementalId {
            ref typ,
            ref constants,
            ..
        } = id;
        let field_name = id.get_field_name();
        fields.extend(quote! {
            #field_name: #typ,
        });
        let get_fresh_name = id.get_generator_fn_name();
        getter_functions.extend(quote! {
            fn #get_fresh_name(&mut self) -> #typ {
                let value = self.#field_name.into();
                self.#field_name += 1;
                value
            }
        });
        for constant in constants {
            let get_constant_name = constant.get_getter_name();
            let value = &constant.value;
            getter_functions.extend(quote! {
                fn #get_constant_name(&mut self) -> #typ {
                    #value
                }
            });
        }
        let default_value = id.get_default_value();
        default_impls.extend(quote! {
           #field_name: #default_value,
        });
    }
    quote! {
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
    }
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
    eprintln!("target_type: {}", target_type.to_token_stream());
    let var_name = name_generator.get_ident();
    let mut found_type = None;
    let mut tokens = TokenStream::new();
    for table in &schema.interning_tables {
        // let new_target_type = ;
        if &table.get_key_type() == target_type {
            assert!(found_type.is_none(), "Ambigous interning tables");
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
