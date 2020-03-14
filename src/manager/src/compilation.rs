//! Module responsible for compiling crates.

use super::sources_list::Crate as CrateInfo;
use crate::sources_list::CratesList;
use log::LevelFilter;
use log::{debug, error, info};
use log_derive::logfn;
use rustwide::logging::{self, LogStorage};
use rustwide::{cmd::SandboxBuilder, Crate, Toolchain, Workspace, WorkspaceBuilder};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct CompileManager {
    /// The list of crates we want to compile.
    crates_list: CratesList,
    /// The rustwide workspace.
    workspace: PathBuf,
    /// The Rust toolchain to use for building.
    toolchain: String,
    /// Maximum log size for a build before it gets truncated.
    max_log_size: usize,
    /// The memory limit that is set while building a crate.
    memory_limit: Option<usize>,
    /// The timeout for the build.
    timeout: Option<Duration>,
    /// Should the network be enabled while building a crate?
    enable_networking: bool,
    /// Should the whole compilation process stop on crate compilation error?
    stop_on_error: bool,
    /// Should the extractor output also json, or only bincode?
    output_json: bool,
}

impl CompileManager {
    pub fn new(
        crates_list: CratesList,
        workspace: &Path,
        toolchain: String,
        max_log_size: usize,
        memory_limit: Option<usize>,
        timeout: Option<Duration>,
        enable_networking: bool,
        stop_on_error: bool,
        output_json: bool,
    ) -> Self {
        Self {
            crates_list: crates_list,
            workspace: workspace
                .canonicalize()
                .expect("Failed to convert the workspace path to absolute."),
            toolchain: toolchain,
            max_log_size: max_log_size,
            memory_limit: memory_limit,
            timeout: timeout,
            enable_networking: enable_networking,
            stop_on_error: stop_on_error,
            output_json: output_json,
        }
    }
    #[logfn(Trace)]
    pub fn compile_all(&self) -> Result<(), Box<dyn std::error::Error>> {
        let workspace_builder = WorkspaceBuilder::new(&self.workspace, "rust-corpus")
            .fetch_registry_index_during_builds(false)
            .running_inside_docker(std::env::var("RUNNING_INSIDE_DOCKER").is_ok());
        let workspace = workspace_builder.init()?;
        let toolchain = Toolchain::dist(&self.toolchain);
        toolchain.install(&workspace)?;
        toolchain.add_component(&workspace, "rustc-dev")?;
        self.copy_extractor()?;
        for krate in self.crates_list.iter() {
            let compiler = CrateCompiler::new(
                &toolchain,
                &workspace,
                self.max_log_size,
                self.memory_limit,
                self.timeout,
                self.enable_networking,
                self.output_json,
                self.workspace.join("rust-corpus"),
            );
            match compiler.build(krate) {
                Ok(_) => info!("Compilation succeeded."),
                Err(error) => {
                    error!("Compilation failed: {}", error);
                    if self.stop_on_error {
                        return Err(error);
                    }
                }
            }
        }
        Ok(())
    }
    /// Copies extractor to the workspace.
    fn copy_extractor(&self) -> Result<(), Box<dyn std::error::Error>> {
        let out_dir: PathBuf = env!("OUT_DIR").into();
        let rustc_path = out_dir.join("../../../rustc").canonicalize()?;
        let dest_path = self.workspace.join("cargo-home/rustc");
        if let Err(error) = std::fs::copy(&rustc_path, &dest_path) {
            debug!(
                "couldn't copy '{}' to '{}': {}",
                rustc_path.display(),
                dest_path.display(),
                error
            );
        };
        Ok(())
    }
}

struct CrateCompiler<'a> {
    toolchain: &'a Toolchain,
    workspace: &'a Workspace,
    max_log_size: usize,
    memory_limit: Option<usize>,
    timeout: Option<Duration>,
    enable_networking: bool,
    output_json: bool,
    extracted_files_path: PathBuf,
}

impl<'a> CrateCompiler<'a> {
    fn new(
        toolchain: &'a Toolchain,
        workspace: &'a Workspace,
        max_log_size: usize,
        memory_limit: Option<usize>,
        timeout: Option<Duration>,
        enable_networking: bool,
        output_json: bool,
        extracted_files_path: PathBuf,
    ) -> Self {
        Self {
            toolchain,
            workspace,
            max_log_size,
            memory_limit,
            timeout,
            enable_networking,
            extracted_files_path,
            output_json,
        }
    }
    fn build(&self, krate_info: &'a CrateInfo) -> Result<(), Box<dyn std::error::Error>> {
        let crate_name_info = format!("{}-{}", krate_info.name(), krate_info.version());
        let crate_extracted_files = self.extracted_files_path.join(&crate_name_info);
        if crate_extracted_files.exists() {
            info!("Already compiled: {}", crate_extracted_files.display());
            return Ok(());
        }
        let krate = Crate::crates_io(krate_info.name(), krate_info.version());
        krate.fetch(self.workspace)?;
        let sandbox = SandboxBuilder::new()
            .memory_limit(self.memory_limit)
            .enable_networking(self.enable_networking);
        let mut build_dir = self.workspace.build_dir(&crate_name_info);
        build_dir.purge()?;
        let toolchain = self.toolchain.as_dist().unwrap().name();
        let sysroot = format!(
            "/opt/rustwide/rustup-home/toolchains/{}-x86_64-unknown-linux-gnu",
            toolchain
        );
        std::fs::create_dir_all(&crate_extracted_files)?;
        build_dir
            .build(self.toolchain, &krate, sandbox)
            .run(|build| {
                let mut storage = LogStorage::new(LevelFilter::Info);
                storage.set_max_size(self.max_log_size);
                let result = logging::capture(&storage, || {
                    let mut builder = build
                        .cargo()
                        .timeout(self.timeout)
                        .args(&["build"])
                        .env("RUST_BACKTRACE", "1")
                        .env("CARGO_INCREMENTAL", "0")
                        .env("SYSROOT", sysroot)
                        .env("RUSTC", "/opt/rustwide/cargo-home/rustc");
                    if self.output_json {
                        builder = builder.env("CORPUS_OUTPUT_JSON", "true");
                    }
                    builder.run()
                });
                let target_dir = build.host_target_dir().join("rust-corpus");
                std::fs::create_dir_all(&target_dir)?;
                if result.is_ok() {
                    let success_marker = target_dir.join("success");
                    std::fs::write(success_marker, format!("{:?}", chrono::offset::Utc::now()))?;
                }
                // Extract log data.
                let build_logs = target_dir.join("logs");
                std::fs::write(build_logs, storage.to_string())?;
                // Extract bincode files.
                for entry in walkdir::WalkDir::new(target_dir) {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_file() {
                        let file_name = path.file_name().unwrap();
                        std::fs::rename(path, crate_extracted_files.join(file_name))?;
                    }
                }
                // Extract lockfile.
                let lockfile = build.host_source_dir().join("Cargo.lock");
                std::fs::rename(lockfile, crate_extracted_files.join("Cargo.lock"))?;
                result
            })?;
        Ok(())
    }
}
