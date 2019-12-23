use syn;

/// A custom ID (most likely some ID taken directly from the Rust compiler).
pub struct CustomId {
    pub name: syn::Ident,
    pub typ: syn::Type,
    pub items: Vec<syn::Item>,
}

/// A constant of the incremental ID.
pub struct Constant {
    pub name: syn::Ident,
    /// **NOTE:** The constant value must be unique and from the range
    /// `0..IncrementalId.constants.len()`.
    pub value: syn::Lit,
}

impl Constant {
    pub fn get_getter_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("get_{}", self.name).to_lowercase(),
            self.name.span(),
        )
    }
}

/// An identifier that is incremented each time an object is created.
pub struct IncrementalId {
    pub name: syn::Ident,
    pub typ: syn::Type,
    /// Some IDs have special values.
    pub constants: Vec<Constant>,
}

impl IncrementalId {
    pub fn get_field_name(&self) -> syn::Ident {
        syn::Ident::new(&format!("{}s", self.name).to_lowercase(), self.name.span())
    }
    pub fn get_generator_fn_name(&self) -> syn::Ident {
        syn::Ident::new(
            &format!("get_fresh_{}", self.name).to_lowercase(),
            self.name.span(),
        )
    }
    pub fn get_default_value(&self) -> syn::LitInt {
        syn::LitInt::new(&self.constants.len().to_string(), self.name.span())
    }
}

/// An identifier used as a key for an interning table.
pub struct InternedId {
    pub name: syn::Ident,
    pub typ: syn::Type,
}

/// An interning table for a specific type.
///
/// Note: the implementation assumes that if values are equal (by the definition of `==`),
/// then the generates keys should also be the same.
pub struct InterningTable {
    pub name: syn::Ident,
    pub key: InternedId,
    pub value: syn::Type,
}

impl InterningTable {
    pub fn get_registration_function_name(&self) -> syn::Ident {
        let mut name = String::from("register_");
        for c in self.name.to_string().chars() {
            if c.is_uppercase() {
                name.push('_');
                name.extend(c.to_lowercase());
            } else {
                name.push(c);
            }
        }

        syn::Ident::new(&name, self.name.span())
    }
    pub fn get_key_type(&self) -> syn::Type {
        syn::Type::Path(syn::TypePath {
            qself: None,
            path: self.key.name.clone().into(),
        })
    }
}

/// A definition of an enum.
pub struct Enum {
    pub item: syn::ItemEnum,
    /// A default variant of the enum.
    pub default: syn::Ident,
}

/// A relation parameter with types.
pub struct RelationParameter {
    pub name: syn::Ident,
    pub typ: syn::Type,
}

/// A Datalog relation.
pub struct Relation {
    pub name: syn::Ident,
    pub parameters: Vec<RelationParameter>,
}

/// Configuration of all tables.
#[derive(Default)]
pub struct DatabaseSchema {
    pub custom_ids: Vec<CustomId>,
    pub incremental_ids: Vec<IncrementalId>,
    pub enums: Vec<Enum>,
    pub interning_tables: Vec<InterningTable>,
    pub relations: Vec<Relation>,
}
