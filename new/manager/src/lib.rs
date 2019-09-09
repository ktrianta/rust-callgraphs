// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Library for managing crate sources.

mod compilation;
mod database;
mod local_archive;
mod sources_list;
mod top_crates;

use self::compilation::CompileManager;
use self::database::DatabaseManager;
use self::local_archive::LocalArchive;
use self::sources_list::CratesList;
use log_derive::logfn;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Initialise the list of crates with ``top_count`` most downloaded crates.
#[logfn(Trace)]
pub fn initialise_with_top(sources_list_path: &Path, top_count: usize, all_versions: bool) {
    let crates_list = CratesList::top_crates_by_download_count(top_count, all_versions);
    crates_list.save(sources_list_path);
}

/// Download the crates.
#[logfn(Trace)]
pub fn download(sources_list_path: &Path, local_archive_index_path: &Path) {
    let crates_list = CratesList::load(sources_list_path);
    let local_archive = LocalArchive::download(&crates_list);
    local_archive.save(local_archive_index_path);
}

/// Compile the downloaded crates.
#[logfn(Trace)]
pub fn compile(
    local_archive_index_path: &Path,
    cargo_path: &Option<PathBuf>,
    sccache_path: &Option<PathBuf>,
    sccache_cache_path: &Path,
    rustc_path: &Option<PathBuf>,
    workspace_root: &Path,
    compilation_timeout: Duration,
) {
    let local_archive = LocalArchive::load(local_archive_index_path);
    let manager = CompileManager::new(
        local_archive,
        cargo_path,
        sccache_path,
        sccache_cache_path,
        rustc_path,
        workspace_root,
        compilation_timeout,
    );
    manager.compile_all();
}

/// Update the database with the new information from the downloaded crates.
#[logfn(Trace)]
pub fn update_database(workspace_root: &Path, database_root: &Path) {
    let mut manager = DatabaseManager::new(database_root);
    manager.update_database(workspace_root);
}
