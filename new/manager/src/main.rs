// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use corpus_manager;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use structopt::StructOpt;

#[derive(StructOpt)]
#[structopt(
    name = "corpus-manager",
    about = "Manager of the Rust corpus database."
)]
struct CorpusManagerArgs {
    #[structopt(
        parse(from_os_str),
        default_value = "CrateList.json",
        long = "crate-list-path",
        help = "The file specifying crates and their versions."
    )]
    crate_list_path: PathBuf,
    #[structopt(
        parse(from_os_str),
        default_value = "LocalCrateList.json",
        long = "local-crate-list-path",
        help = "The file with the local paths of all downloaded crates."
    )]
    local_archive_index_path: PathBuf,
    #[structopt(
        parse(from_os_str),
        long = "cargo-path",
        help = "The path to the cargo binary."
    )]
    cargo_path: Option<PathBuf>,
    #[structopt(
        parse(from_os_str),
        long = "sccache-path",
        help = "The path to the sccache binary. (Install via cargo install sccache)."
    )]
    sccache_path: Option<PathBuf>,
    #[structopt(
        parse(from_os_str),
        default_value = "compilation-cache",
        long = "sccache-cache-path",
        help = "The directory used to store sccache compilation cache."
    )]
    sccache_cache_path: PathBuf,
    #[structopt(
        parse(from_os_str),
        long = "rustc-path",
        help = "The path to the extractor binary."
    )]
    rustc_path: Option<PathBuf>,
    #[structopt(
        parse(from_os_str),
        default_value = "../workspace",
        long = "workspace",
        help = "The directory in which all crates are compiled."
    )]
    workspace_root: PathBuf,
    #[structopt(
        parse(from_os_str),
        default_value = "../database",
        long = "database",
        help = "The directory in which the database is stored."
    )]
    database_root: PathBuf,
    #[structopt(
        default_value = "900",
        long = "compilation-timeout",
        help = "The compilation timeout in seconds."
    )]
    compilation_timeout: u64,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    #[structopt(name = "init", about = "Initialise the list of crates.")]
    Init {
        #[structopt(help = "How many top crates to download.")]
        top_count: usize,
        #[structopt(help = "Download all crate versions or only the newest one.")]
        all_versions: bool,
    },
    #[structopt(name = "download", about = "Download the list of crates.")]
    Download,
    #[structopt(name = "compile", about = "Compile the list of crates.")]
    Compile,
    #[structopt(
        name = "update-database",
        about = "Scan the compiled crates and update the database."
    )]
    UpdateDatabase,
}

fn main() {
    color_backtrace::install();
    {
        let timestamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
        let info_log_file = format!("log/info-{}.log", timestamp);
        let trace_log_file = format!("log/trace-{}.log", timestamp);
        use simplelog::*;
        fs::create_dir_all("log").unwrap();
        let mut loggers: Vec<Box<dyn SharedLogger>> = vec![
            WriteLogger::new(
                LevelFilter::Info,
                Config::default(),
                fs::File::create(&info_log_file).unwrap(),
            ),
            WriteLogger::new(
                LevelFilter::Trace,
                Config::default(),
                fs::File::create(&trace_log_file).unwrap(),
            ),
        ];
        match TermLogger::new(LevelFilter::Info, Config::default(), TerminalMode::Mixed) {
            Some(logger) => loggers.push(logger as Box<dyn SharedLogger>),
            None => loggers.push(SimpleLogger::new(LevelFilter::Warn, Config::default())),
        }
        CombinedLogger::init(loggers).unwrap();
    }
    let args = CorpusManagerArgs::from_args();
    match args.cmd {
        Command::Init {
            top_count,
            all_versions,
        } => {
            corpus_manager::initialise_with_top(&args.crate_list_path, top_count, all_versions);
        }
        Command::Download => {
            corpus_manager::download(&args.crate_list_path, &args.local_archive_index_path);
        }
        Command::Compile => {
            fs::create_dir_all(&args.sccache_cache_path)
                .expect("Failed to create the SCCACHE directory.");
            let absolute_sccache_cache_path = std::fs::canonicalize(args.sccache_cache_path)
                .expect("Failed to convert the SCCACHE directory path to absolute.");
            corpus_manager::compile(
                &args.local_archive_index_path,
                &args.cargo_path,
                &args.sccache_path,
                &absolute_sccache_cache_path,
                &args.rustc_path,
                &args.workspace_root,
                Duration::from_secs(args.compilation_timeout),
            );
        }
        Command::UpdateDatabase => {
            corpus_manager::update_database(&args.workspace_root, &args.database_root)
        }
    }
}
