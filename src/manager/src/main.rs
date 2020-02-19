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
        default_value = "../workspace",
        long = "workspace",
        help = "The directory in which all crates are compiled."
    )]
    workspace: PathBuf,
    #[structopt(
        default_value = "4000000000",   // 4 GB
        long = "memory-limit",
        help = "The memory limit that is set while building a crate. 0 means no limit."
    )]
    memory_limit: usize,
    #[structopt(
        long = "enable-networking",
        help = "Should the network be enabled while building a crate?"
    )]
    enable_networking: bool,
    #[structopt(
        long = "stop-on-error",
        help = "On crate compilation error, stop and ignore the remaining to be compiled crates."
    )]
    stop_on_error: bool,
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
        help = "The compilation timeout in seconds. 0 means no timeout."
    )]
    compilation_timeout: u64,
    #[structopt(
        default_value = "5242880",   // 5 MB
        long = "max-log-size",
        help = "The maximum log size per build before it gets truncated (in bytes)."
    )]
    max_log_size: usize,
    #[structopt(subcommand)]
    cmd: Command,
}

#[derive(StructOpt)]
enum Command {
    #[structopt(name = "init", about = "Initialise the list of crates.")]
    Init {
        #[structopt(help = "How many top crates to download.")]
        top_count: usize,
        #[structopt(long, help = "Download all crate versions or only the newest one.")]
        all_versions: bool,
    },
    #[structopt(
        name = "init-all",
        about = "Initialise the list of crates with all crates."
    )]
    InitAll {
        #[structopt(long, help = "Download all crate versions or only the newest one.")]
        all_versions: bool,
    },
    #[structopt(name = "compile", about = "Compile the list of crates.")]
    Compile {
        #[structopt(long, help = "Should the extractor output also json, or only bincode?")]
        output_json: bool,
    },
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
        let logger = CombinedLogger::new(loggers);
        rustwide::logging::init_with(*logger);
    }
    let args = CorpusManagerArgs::from_args();
    match args.cmd {
        Command::Init {
            top_count,
            all_versions,
        } => {
            corpus_manager::initialise_with_top(&args.crate_list_path, top_count, all_versions);
        }
        Command::InitAll { all_versions } => {
            corpus_manager::initialise_with_all(&args.crate_list_path, all_versions);
        }
        Command::Compile { output_json } => {
            let toolchain = {
                use std::io::Read;
                let mut file = std::fs::File::open("rust-toolchain")
                    .expect("Failed to open file “rust-toolchain”.");
                let mut contents = String::new();
                file.read_to_string(&mut contents)
                    .expect("Failed to read “rust-toolchain”.");
                contents.trim().to_string()
            };
            let memory_limit = if args.memory_limit == 0 {
                None
            } else {
                Some(args.memory_limit)
            };
            let timeout = if args.compilation_timeout == 0 {
                None
            } else {
                Some(Duration::from_secs(args.compilation_timeout))
            };
            corpus_manager::compile(
                &args.crate_list_path,
                &args.workspace,
                toolchain,
                args.max_log_size,
                memory_limit,
                timeout,
                args.enable_networking,
                args.stop_on_error,
                output_json,
            );
        }
        Command::UpdateDatabase => {
            corpus_manager::update_database(&args.workspace, &args.database_root)
        }
    }
}
