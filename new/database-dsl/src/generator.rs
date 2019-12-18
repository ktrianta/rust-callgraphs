use crate::ast;
use proc_macro2::TokenStream;
use quote::quote;

pub(crate) fn generate_tokens(schema: ast::DatabaseSchema) -> TokenStream {
    let types = generate_types(&schema);
    let tables = generate_interning_tables(&schema);
    let relations = generate_relations(&schema);
    quote! {
        pub mod types {
            use serde_derive::{Deserialize, Serialize};
            #types
        }
        pub mod tables {
            #tables
            #relations
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
        use super::types::*;
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
        use super::types::ScopeSafety;
        pub struct Relations {
            #fields
        }
    }
}
