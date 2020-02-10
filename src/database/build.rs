use corpus_database_dsl::{generate_definition, parse_schema};
use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/schema.dl");

    let definition = parse_schema(Path::new("src/schema.dl"));
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("schema.rs");
    generate_definition(&dest_path, definition);
}
