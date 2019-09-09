// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Local copies of all crates.

use super::sources_list::{Crate, CratesList};
use cargo::core::{registry::PackageRegistry, SourceId};
use cargo::util::Config;
use log::{error, info};
use log_derive::logfn;
use serde::{Deserialize, Serialize};

use std::fs::File;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize)]
struct DownloadedCrate {
    krate: Crate,
    local_path: PathBuf,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalArchive {
    downloaded_crates: Vec<DownloadedCrate>,
}

impl LocalArchive {
    pub fn load(path: &Path) -> Self {
        let file =
            File::open(path).unwrap_or_else(|e| panic!("Failed to load from {:?}: {}", path, e));
        serde_json::from_reader(file).unwrap_or_else(|e| panic!("Invalid JSON {:?}: {}", path, e))
    }
    pub fn save(&self, path: &Path) {
        let mut file =
            File::create(path).unwrap_or_else(|e| panic!("Unable to create {:?}: {}", path, e));
        serde_json::to_writer_pretty(&mut file, self)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    }
    #[logfn(Trace)]
    pub fn download(crate_list: &CratesList) -> Self {
        let mut archive = Self {
            downloaded_crates: Vec::new(),
        };
        archive.download_from_crates_io(crate_list);
        archive
    }
    #[logfn(Trace)]
    fn download_from_crates_io(&mut self, crate_list: &CratesList) {
        let config = Config::default().expect("Unable to create default Cargo config");
        let _lock = config.acquire_package_cache_lock();
        let crates_io: SourceId =
            SourceId::crates_io(&config).expect("Unable to create crates.io source ID");
        let package_ids: Vec<_> = crate_list
            .iter_packages()
            .map(|package| package.to_package_id(crates_io))
            .collect();
        let mut registry = PackageRegistry::new(&config).unwrap();
        let sources = vec![crates_io];
        registry.add_sources(sources).unwrap();
        let set = registry.get(&package_ids).unwrap();
        for (package, package_id) in crate_list.iter_packages().zip(&package_ids) {
            match set.get_one(package_id.clone()) {
                Ok(cargo_package) => {
                    info!("Ready: {:?} at {:?}", package_id, cargo_package.root());
                    let downloaded_crate = DownloadedCrate {
                        krate: Crate::Package(package.clone()),
                        local_path: cargo_package.root().to_path_buf(),
                    };
                    self.downloaded_crates.push(downloaded_crate);
                }
                Err(error) => {
                    error!("Failed to download: {}", error);
                }
            }
        }
    }
    pub fn iter<'a>(&'a self) -> impl Iterator<Item = (&'a Crate, &'a Path)> {
        self.downloaded_crates
            .iter()
            .map(|DownloadedCrate { krate, local_path }| (krate, local_path.as_path()))
    }
}
