use analysis::analysis::CallGraphAnalysis;
use corpus_database::tables::Tables;
use std::path::{Path, PathBuf};
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "callgraph-analyzer",
    about = "Call-graph analyzer for Rust programs."
)]
struct CMDArgs {
    #[structopt(
        parse(from_os_str),
        default_value = "../../database",
        long = "database",
        help = "The directory in which the database is stored."
    )]
    database_root: PathBuf,
}

fn main() {
    let args = CMDArgs::from_args();
    let database_root = Path::new(&args.database_root);
    let tables = Tables::load_multifile(database_root).unwrap();
    let mut analysis = CallGraphAnalysis::new(&tables);
    // println!("Loaded database");

    let callgraph = analysis.run();
    println!("{}", serde_json::to_string_pretty(&callgraph).unwrap());
}
