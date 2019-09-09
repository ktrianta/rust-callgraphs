// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Module for managing lists of crate sources.

use cargo::core::{Dependency, PackageId, Source, SourceId};
use cargo::sources::RegistrySource;
use cargo::util::Config;
use log_derive::{logfn, logfn_inputs};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A create on crates.io.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Package {
    name: String,
    version: String,
}

impl Package {
    pub fn to_package_id(&self, source_id: SourceId) -> PackageId {
        PackageId::new(&self.name, &self.version, source_id).unwrap()
    }
}

/// A crate source: either crates.io name or a repository URL.
#[derive(Debug, Deserialize, Serialize)]
pub enum Crate {
    Package(Package),
}

impl Crate {
    pub fn name(&self) -> &str {
        match self {
            Crate::Package(Package { ref name, .. }) => name,
        }
    }
    pub fn version(&self) -> &str {
        match self {
            Crate::Package(Package { ref version, .. }) => version,
        }
    }
    /// Create a path inside the ``root`` directory that uniquely identifies this crate.
    pub fn work_path(&self, root: &Path) -> PathBuf {
        let name = self.name();
        let prefix = name.chars().chain("___".chars()).take(3);
        let mut work_path = root.to_path_buf();
        for c in prefix {
            work_path.push(c.to_string());
        }
        work_path.push(name);
        work_path.push(self.version());
        work_path
    }
}

/// A list of sources from where to download creates.
#[derive(Debug, Deserialize, Serialize)]
pub struct CratesList {
    creation_date: SystemTime,
    crates: Vec<Crate>,
}

impl CratesList {
    /// Create a list of top ``count`` crates.
    ///
    /// `all_versions` – should get all versions or only the newest one?
    #[logfn(Trace)]
    pub fn top_crates_by_download_count(count: usize, all_versions: bool) -> Self {
        let config = Config::default().expect("Unable to create default Cargo config");
        let _lock = config.acquire_package_cache_lock();
        let crates_io = SourceId::crates_io(&config).expect("Unable to create crates.io source ID");
        let mut source = RegistrySource::remote(crates_io, &HashSet::new(), &config);
        source.update().expect("Unable to update registry");
        let creation_date = SystemTime::now();
        let mut crates = Vec::new();
        for crate_name in super::top_crates::top_crates_by_download_count(count) {
            let query = Dependency::new_override(&crate_name, crates_io);
            let summaries = source.query_vec(&query).unwrap_or_else(|err| {
                panic!("Querying for {} failed: {}", crate_name, err);
            });
            if all_versions {
                for summary in summaries {
                    let package = Package {
                        name: crate_name.clone(),
                        version: summary.version().to_string(),
                    };
                    crates.push(Crate::Package(package));
                }
            } else {
                let maybe_summary = summaries
                    .into_iter()
                    .max_by_key(|summary| summary.version().clone());
                if let Some(summary) = maybe_summary {
                    let package = Package {
                        name: crate_name.clone(),
                        version: summary.version().to_string(),
                    };
                    crates.push(Crate::Package(package));
                }
            }
        }
        Self {
            creation_date: creation_date,
            crates: crates,
        }
    }

    /// Save the list into a file.
    #[logfn_inputs(Trace)]
    pub fn save(&self, path: &std::path::Path) {
        let mut file =
            File::create(path).unwrap_or_else(|e| panic!("Unable to create {:?}: {}", path, e));
        serde_json::to_writer_pretty(&mut file, self)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    }

    /// Load the list from a file.
    pub fn load(path: &std::path::Path) -> Self {
        let file =
            File::open(path).unwrap_or_else(|e| panic!("Failed to load from {:?}: {}", path, e));
        serde_json::from_reader(file).unwrap_or_else(|e| panic!("Invalid JSON {:?}: {}", path, e))
    }

    pub fn iter_packages<'a>(&'a self) -> impl Iterator<Item = &'a Package> {
        self.crates.iter().flat_map(|c| match c {
            Crate::Package(package) => Some(package),
        })
    }
}
