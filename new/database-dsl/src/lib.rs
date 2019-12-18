use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

mod ast;
mod generator;
mod parser;

pub fn parse_schema(path: &Path) -> ast::DatabaseSchema {
    let mut file = File::open(path).unwrap();
    let mut content = String::new();
    file.read_to_string(&mut content).unwrap();
    match syn::parse_str(&content) {
        Ok(config) => config,
        Err(err) => panic!("Error: {:?} (at {:?})", err, err.span().start()),
    }
}

pub fn generate_definition(dest_path: &Path, schema: ast::DatabaseSchema) {
    let tokens = generator::generate_tokens(schema);
    let mut file = File::create(dest_path).unwrap();
    file.write(tokens.to_string().as_bytes()).unwrap();
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
