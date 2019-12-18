use crate::ast;
use std::mem;
use syn::parse::{Parse, ParseStream};
use syn::{self, spanned::Spanned};
use syn::{punctuated::Punctuated, Token};

mod kw {
    syn::custom_keyword!(custom_id);
    syn::custom_keyword!(inc_id);
    syn::custom_keyword!(intern);
    syn::custom_keyword!(relation);
}

impl Parse for ast::CustomId {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::custom_id>()?;
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        let content;
        syn::braced!(content in input);
        let mut items = Vec::new();
        while !content.is_empty() {
            items.push(content.parse()?);
        }
        Ok(Self { name, typ, items })
    }
}

impl Parse for ast::Constant {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let _attrs = input.call(syn::Attribute::parse_outer)?;
        Ok(Self {
            name: input.parse()?,
            value: {
                input.parse::<Token![=]>()?;
                input.parse()?
            }
        })
    }
}

impl Parse for ast::IncrementalId {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::inc_id>()?;
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        let content;
        syn::braced!(content in input);
        let punctuated: Punctuated<_, Token![,]> =
            content.parse_terminated(ast::Constant::parse)?;
        let constants = punctuated
            .into_pairs()
            .map(|pair| pair.into_value())
            .collect();
        Ok(Self {
            name,
            typ,
            constants,
        })
    }
}

impl Parse for ast::InternedId {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![<]>()?;
        let typ = input.parse()?;
        input.parse::<Token![>]>()?;
        Ok(Self { name, typ })
    }
}

impl Parse for ast::InterningTable {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::intern>()?;
        let name = input.parse()?;
        input.parse::<Token![<]>()?;
        let value = input.parse()?;
        input.parse::<Token![as]>()?;
        let key = input.parse()?;
        input.parse::<Token![>]>()?;
        input.parse::<Token![;]>()?;
        Ok(Self { name, key, value })
    }
}

impl Parse for ast::Enum {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut item: syn::ItemEnum = input.parse()?;
        let mut default = None;
        for variant in &mut item.variants {
            let mut new_attrs = Vec::new();
            for attr in mem::replace(&mut variant.attrs, Vec::new()) {
                if attr.path.is_ident("default") && attr.tokens.is_empty() {
                    default = Some(variant.ident.clone());
                }
            }
            mem::swap(&mut variant.attrs, &mut new_attrs)
        }
        let default =
            default.ok_or_else(|| syn::Error::new(item.span(), "missing #[default] variant"))?;
        Ok(Self { item, default })
    }
}

impl Parse for ast::RelationParameter {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name = input.parse()?;
        input.parse::<Token![:]>()?;
        let typ = input.parse()?;
        Ok(Self { name, typ })
    }
}

impl Parse for ast::Relation {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        input.parse::<kw::relation>()?;
        let name = input.parse()?;
        let content;
        syn::parenthesized!(content in input);
        let punctuated: Punctuated<_, Token![,]> =
            content.parse_terminated(ast::RelationParameter::parse)?;
        let parameters = punctuated
            .into_pairs()
            .map(|pair| pair.into_value())
            .collect();
        input.parse::<Token![;]>()?;
        Ok(Self { name, parameters })
    }
}

impl Parse for ast::DatabaseSchema {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut schema = ast::DatabaseSchema::default();
        while !input.is_empty() {
            let _attrs = input.call(syn::Attribute::parse_outer)?;
            let lookahead = input.lookahead1();
            if lookahead.peek(kw::custom_id) {
                let custom_id: ast::CustomId = input.parse()?;
                // inc_id.comments = _attrs; TODO
                schema.custom_ids.push(custom_id);
            } else if lookahead.peek(kw::inc_id) {
                let inc_id: ast::IncrementalId = input.parse()?;
                // inc_id.comments = _attrs; TODO
                schema.incremental_ids.push(inc_id);
            } else if lookahead.peek(kw::intern) {
                let intern_table: ast::InterningTable = input.parse()?;
                schema.interning_tables.push(intern_table);
            } else if lookahead.peek(Token![enum]) {
                let item: ast::Enum = input.parse()?;
                schema.enums.push(item);
            } else if lookahead.peek(kw::relation) {
                let relation: ast::Relation = input.parse()?;
                schema.relations.push(relation);
            } else {
                return Err(lookahead.error());
            }
        }
        Ok(schema)
    }
}
