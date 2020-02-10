// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Module responsible for managing the database.

use corpus_database::tables;
use failure::Error;
use log::{debug, error, info, trace};
use log_derive::logfn;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{ffi, fs, io};
use walkdir;

pub struct DatabaseManager {
    loaded_crates_path: PathBuf,
    loaded_crates: HashSet<PathBuf>,
    database_root: PathBuf,
    database: tables::Tables,
}

impl DatabaseManager {
    pub fn new(database_root: &Path) -> Self {
        let database_root = database_root.to_path_buf();
        let loaded_crates_path = database_root.join("loaded_crates.json");
        let loaded_crates = if database_root.exists() {
            // The database already contains some crates.
            let file = fs::File::open(&loaded_crates_path).unwrap_or_else(|e| {
                panic!(
                    "The database state is corrupted. \
                     Failed to read the list of loaded crates {:?}: {}",
                    loaded_crates_path, e
                )
            });
            serde_json::from_reader(file).unwrap_or_else(|e| {
                panic!(
                    "The database state is corrupted. The crates list is invalid JSON {:?}: {}",
                    loaded_crates_path, e
                )
            })
        } else {
            fs::create_dir_all(&database_root)
                .expect("Failed to create the directory for the database");
            HashSet::new()
        };
        let database = tables::Tables::load_multifile_or_default(&database_root).unwrap();
        Self {
            loaded_crates_path,
            loaded_crates,
            database_root,
            database,
        }
    }
    #[logfn(Trace)]
    pub fn update_database(&mut self, workspace_root: &Path) {
        let crates = self.scan_crates(&workspace_root.join("rust-corpus"));
        let mut counter = 0;
        for path in crates {
            trace!("Checking crate: {:?}", path);
            if self.loaded_crates.contains(&path) {
                debug!("Crate already loaded: {:?}", path);
            } else {
                info!("Loading crate ({}): {:?}", counter, path);
                counter += 1;
                match self.load_crate(path) {
                    Ok(()) => {}
                    Err(e) => error!("  Error occurred: {}", e),
                };
            }
        }
        info!("Successfully loaded {} crates", counter);
        // Delete the loaded crates file so that if we crash, we know that we are
        // in a corrupted state.
        match fs::remove_file(&self.loaded_crates_path) {
            Ok(_) => {}
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    panic!("Failed to remove the loaded crates file.")
                }
            }
        }
        self.database.store_multifile(&self.database_root).unwrap();
        let mut file = fs::File::create(&self.loaded_crates_path)
            .unwrap_or_else(|e| panic!("Unable to create {:?}: {}", self.loaded_crates_path, e));
        serde_json::to_writer_pretty(&mut file, &self.loaded_crates)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", self.loaded_crates_path, e));
    }
    fn scan_crates(&self, workspace_root: &Path) -> impl Iterator<Item = PathBuf> {
        walkdir::WalkDir::new(workspace_root.canonicalize().unwrap())
            .into_iter()
            .filter_entry(|entry| entry.file_name() != "source")
            .map(|entry| entry.unwrap().into_path())
            .filter(|path| path.extension() == Some(ffi::OsStr::new("bincode")))
    }
    #[logfn(Trace)]
    fn load_crate(&mut self, crate_path: PathBuf) -> Result<(), Error> {
        let crate_tables = tables::Tables::load(&crate_path)?;
        self.database.merge(crate_tables);
        self.loaded_crates.insert(crate_path);
        Ok(())
    }
}
