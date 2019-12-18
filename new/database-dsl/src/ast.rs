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
}

/// An identifier that is incremented each time an object is created.
pub struct IncrementalId {
    pub name: syn::Ident,
    pub typ: syn::Type,
    /// Some IDs have special values.
    pub constants: Vec<Constant>,
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
