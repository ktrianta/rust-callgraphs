// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Library for managing crate sources.

mod compilation;
mod database;
mod sources_list;
mod top_crates;

use self::compilation::CompileManager;
use self::database::DatabaseManager;
use self::sources_list::CratesList;
use log_derive::logfn;
use std::path::Path;
use std::time::Duration;

/// Initialise the list of crates with ``top_count`` most downloaded crates.
#[logfn(Trace)]
pub fn initialise_with_top(sources_list_path: &Path, top_count: usize, all_versions: bool) {
    let crates_list = CratesList::top_crates_by_download_count(top_count, all_versions);
    crates_list.save(sources_list_path);
}

pub fn initialise_with_all(sources_list_path: &Path, all_versions: bool) {
    let crates_list = CratesList::all_crates(all_versions);
    crates_list.save(sources_list_path);
}

/// Compile the downloaded crates.
#[logfn(Trace)]
pub fn compile(
    sources_list_path: &Path,
    workspace: &Path,
    toolchain: String,
    max_log_size: usize,
    memory_limit: Option<usize>,
    timeout: Option<Duration>,
    enable_networking: bool,
    output_json: bool,
) {
    let crates_list = CratesList::load(sources_list_path);
    let manager = CompileManager::new(
        crates_list,
        workspace,
        toolchain,
        max_log_size,
        memory_limit,
        timeout,
        enable_networking,
        output_json,
    );
    manager
        .compile_all()
        .map_err(|e| panic!("Error: {}", e))
        .unwrap();
}

/// Update the database with the new information from the downloaded crates.
#[logfn(Trace)]
pub fn update_database(workspace: &Path, database_root: &Path) {
    let mut manager = DatabaseManager::new(database_root);
    manager.update_database(workspace);
}
