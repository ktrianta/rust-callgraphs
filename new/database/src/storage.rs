// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Helper functions for serializing and desiarilizing.

use crate::tables::Tables;
use failure::{Error, Fail};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
enum LoadError {
    OpenFileError { path: PathBuf, error: String },
    InvalidBincode { path: PathBuf, error: String },
    InvalidJson { path: PathBuf, error: String },
}

impl Fail for LoadError {}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            LoadError::OpenFileError { path, error } => {
                write!(f, "Failed to open file {:?}: {}", path, error)
            }
            LoadError::InvalidBincode { path, error } => {
                write!(f, "Invalid bincode {:?}: {}", path, error)
            }
            LoadError::InvalidJson { path, error } => {
                write!(f, "Invalid JSON {:?}: {}", path, error)
            }
        }
    }
}

type LoadResult<T> = Result<T, Error>;

pub fn load<T>(path: &Path) -> LoadResult<T>
where
    for<'de> T: Deserialize<'de>,
{
    let extension = path.extension().unwrap();
    let file = std::fs::File::open(path).map_err(|e| LoadError::OpenFileError {
        path: path.to_path_buf(),
        error: e.to_string(),
    })?;
    if extension == "bincode" {
        bincode::deserialize_from(file).map_err(|e| {
            LoadError::InvalidBincode {
                path: path.to_path_buf(),
                error: e.to_string(),
            }
            .into()
        })
    } else if extension == "json" {
        serde_json::from_reader(file).map_err(|e| {
            LoadError::InvalidJson {
                path: path.to_path_buf(),
                error: e.to_string(),
            }
            .into()
        })
    } else {
        unreachable!("Unknown extension: {:?}", extension);
    }
}

pub fn load_or_default<T>(path: &Path) -> LoadResult<T>
where
    for<'de> T: Deserialize<'de> + Default,
{
    if path.exists() {
        load(path)
    } else {
        Ok(T::default())
    }
}

pub fn save<T>(object: &T, path: &Path)
where
    T: Serialize,
{
    let extension = path.extension().unwrap();
    let mut file = std::fs::File::create(&path)
        .unwrap_or_else(|e| panic!("Unable to create {:?}: {}", path, e));
    if extension == "bincode" {
        bincode::serialize_into(file, object)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    } else if extension == "json" {
        serde_json::to_writer_pretty(&mut file, object)
            .unwrap_or_else(|e| panic!("Unable to write {:?}: {}", path, e));
    } else {
        unreachable!("Unknown extension: {:?}", extension);
    }
}

impl Tables {
    /// ``path`` – the path **without** the extension.
    pub fn save_json(&self, mut path: std::path::PathBuf) {
        path.set_extension("json");
        save(&self, &path);
    }
    /// ``path`` – the path the **without** extension.
    pub fn save_bincode(&self, mut path: std::path::PathBuf) {
        path.set_extension("bincode");
        save(&self, &path);
    }
    /// ``path`` – the path **with** the extension.
    pub fn load(path: &std::path::Path) -> Result<Self, Error> {
        load(path)
    }
}
