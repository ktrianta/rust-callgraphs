// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

//! Module responsible for compiling crates.

use super::local_archive::LocalArchive;
use super::sources_list::Crate;
use failure::{Error, Fail};
use log::{debug, error, info};
use log_derive::logfn;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
use std::{ffi, fmt, fs, io};
use tokio::prelude::{Future, FutureExt};
use tokio::runtime::current_thread::block_on_all;
use tokio_process::CommandExt;

pub struct CompileManager {
    /// The list of downloaded crates we want to compile.
    local_archive: LocalArchive,
    /// Path to the cargo binary.
    cargo_path: PathBuf,
    /// Path to the sccache binary.
    sccache_path: PathBuf,
    /// Directory used by sccache for caching.
    sccache_cache_path: PathBuf,
    /// Path to the compiler binary.
    rustc_path: PathBuf,
    /// The root of the workspace.
    workspace_root: PathBuf,
    /// The maximum time in seconds for the compilation.
    compilation_timeout: Duration,
    /// Rustc sysroot.
    sysroot: PathBuf,
}

#[derive(Debug)]
enum CompileError {
    RunError {
        cmd: String,
        args: Vec<ffi::OsString>,
        status: std::process::ExitStatus,
    },
    ResolveFailureError {
        name: String,
        status: std::process::ExitStatus,
    },
    Timeout(u64),
    UnknownSysrootError {
        status: std::process::ExitStatus,
    },
}

impl Fail for CompileError {}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        match self {
            CompileError::RunError { cmd, args, status } => {
                write!(f, "Command “{}", cmd)?;
                for arg in args {
                    write!(f, " {:?}", arg)?;
                }
                write!(f, "” failed with {}", status)
            }
            CompileError::ResolveFailureError { name, status } => {
                write!(f, "Failed to locate {}: {}", name, status)
            }
            CompileError::Timeout(timeout) => {
                write!(f, "Command did not complete in {} seconds", timeout)
            }
            CompileError::UnknownSysrootError { status } => {
                write!(f, "Failed to detect Rust sysroot: {}", status)
            }
        }
    }
}

type CompilationResult = Result<(), Error>;

impl CompileManager {
    pub fn new(
        local_archive: LocalArchive,
        cargo_path: &Option<PathBuf>,
        sccache_path: &Option<PathBuf>,
        sccache_cache_path: &Path,
        rustc_path: &Option<PathBuf>,
        workspace_root: &Path,
        compilation_timeout: Duration,
    ) -> Self {
        let resolved_cargo_path = match cargo_path {
            Some(path) => path.clone(),
            None => resolve_binary("cargo").unwrap(),
        };
        let resolved_sccache_path = match sccache_path {
            Some(path) => path.clone(),
            None => resolve_binary("sccache").unwrap(),
        };
        let out_dir: PathBuf = env!("OUT_DIR").into();
        debug!("corpus-manager out dir: {:?}", out_dir);
        let target_dir = out_dir.join("../../..").canonicalize().unwrap();
        debug!("target_dir: {:?}", target_dir);
        let resolved_rustc_path = match rustc_path {
            Some(path) => path.clone(),
            None => target_dir.join("rustc"),
        };

        let sysroot = current_sysroot().unwrap();

        fs::create_dir_all(workspace_root).expect("Failed to create workspace root directory.");

        Self {
            local_archive: local_archive,
            cargo_path: resolved_cargo_path,
            sccache_path: resolved_sccache_path,
            workspace_root: workspace_root
                .canonicalize()
                .expect("Failed to convert the workspace root directory path to absolute."),
            sccache_cache_path: sccache_cache_path.to_path_buf(),
            rustc_path: resolved_rustc_path,
            compilation_timeout: compilation_timeout,
            sysroot: sysroot,
        }
    }
    #[logfn(Trace)]
    pub fn compile_all(&self) {
        for (krate, local_path) in self.local_archive.iter() {
            match self.compile_crate(krate, local_path) {
                Ok(_) => info!("Compilation succeeded."),
                Err(error) => error!("Compilation failed: {}", error),
            }
        }
    }
    fn compile_crate<'a>(&self, krate: &'a Crate, local_path: &'a Path) -> CompilationResult {
        let compiler = CrateCompiler::new(
            krate,
            local_path,
            &self.cargo_path,
            &self.sccache_path,
            &self.sccache_cache_path,
            &self.rustc_path,
            &self.workspace_root,
            self.compilation_timeout,
            &self.sysroot,
        );
        compiler.prepare_workspace()?;
        compiler.compile()?;
        // compiler.save_info();
        // self.add_report(compiler.get_report());
        Ok(())
    }
}

#[derive(Debug)]
struct CrateCompiler<'a> {
    krate: &'a Crate,
    local_path: &'a Path,
    source_path: PathBuf,
    cargo_path: &'a Path,
    sccache_path: &'a Path,
    sccache_cache_path: &'a Path,
    rustc_path: &'a Path,
    workspace: PathBuf,
    stderr_path: PathBuf,
    stdout_path: PathBuf,
    compilation_timeout: Duration,
    sysroot: &'a Path,
}

impl<'a> CrateCompiler<'a> {
    #[logfn(Trace)]
    fn new(
        krate: &'a Crate,
        local_path: &'a Path,
        cargo_path: &'a Path,
        sccache_path: &'a Path,
        sccache_cache_path: &'a Path,
        rustc_path: &'a Path,
        workspace_root: &Path,
        compilation_timeout: Duration,
        sysroot: &'a Path,
    ) -> Self {
        let workspace = krate.work_path(workspace_root);
        let source_path = workspace.join("source");
        info!(
            "Compiling {} local={:?} workspace={:?}",
            krate.name(),
            local_path,
            workspace
        );
        let stderr_path = workspace.join("stderr.log");
        let stdout_path = workspace.join("stdout.log");
        Self {
            krate,
            local_path,
            source_path,
            cargo_path,
            sccache_path,
            sccache_cache_path,
            rustc_path,
            workspace,
            stderr_path,
            stdout_path,
            compilation_timeout,
            sysroot,
        }
    }
    /// Prepares the workspace for compilation.
    ///
    /// 1.  Ensures the workspace directory exists and is empty.
    /// 2.  Copies the source code into the workspace and compiles it.
    #[logfn(Trace)]
    fn prepare_workspace(&self) -> CompilationResult {
        match fs::remove_dir_all(&self.workspace) {
            Ok(_) => {}
            Err(error) => {
                if error.kind() != io::ErrorKind::NotFound {
                    return Err(error.into());
                }
            }
        };
        fs::create_dir_all(&self.workspace)?;

        run_command(
            "cp",
            &[
                ffi::OsStr::new("-r"),
                self.local_path.as_os_str(),
                self.source_path.as_os_str(),
            ],
        )?;
        Ok(())
    }
    /// Compile the crate with a given compiler.
    #[logfn(Trace)]
    fn compile(&self) -> CompilationResult {
        let stdout_file = fs::File::create(&self.stdout_path)?;
        let stderr_file = fs::File::create(&self.stderr_path)?;
        let path_env = "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin";
        let library_path = self.sysroot.join("lib");
        let data_path = self.workspace.join("rust_corpus_data");
        fs::create_dir_all(&data_path)?;
        let child = Command::new(self.cargo_path)
            .current_dir(&self.source_path)
            .env_clear()
            .stdin(Stdio::null())
            .stdout(stdout_file)
            .stderr(stderr_file)
            .env("RUST_BACKTRACE", "1")
            .env("SCCACHE_DIR", &self.sccache_cache_path)
            .env("RUSTC_WRAPPER", &self.sccache_path)
            .env("RUSTC", &self.rustc_path)
            .env("SYSROOT", &self.sysroot)
            .env("LD_LIBRARY_PATH", library_path)
            .env("PATH", path_env)
            .env("RUST_CORPUS_DATA_PATH", data_path)
            .env("RUST_CORPUS_DISABLE_LINTS", "true")
            .args(&["build", "--verbose"])
            .spawn_async()?;
        let child_id = child.id();

        let compilation_timeout = self.compilation_timeout;
        let child = child
            .timeout(compilation_timeout)
            .map_err(move |err| -> Error {
                if err.is_elapsed() {
                    use nix::{
                        sys::signal::{kill, Signal},
                        unistd::Pid,
                    };
                    match kill(Pid::from_raw(child_id as i32), Signal::SIGKILL) {
                        Ok(()) => {
                            let error = CompileError::Timeout(compilation_timeout.as_secs());
                            error.into()
                        }
                        Err(error) => error.into(),
                    }
                } else {
                    err.into()
                }
            });
        let status = block_on_all(child)?;
        if status.success() {
            Ok(())
        } else {
            let error = CompileError::RunError {
                cmd: self.cargo_path.to_str().map(|s| s.to_string()).unwrap(),
                args: Vec::new(),
                status: status,
            };
            Err(Error::from(error))
        }
    }
}

#[logfn(Trace)]
fn run_command<S>(cmd: &str, args: &[S]) -> CompilationResult
where
    S: AsRef<std::ffi::OsStr>,
{
    let status = Command::new(cmd).env_clear().args(args).status()?;
    if status.success() {
        Ok(())
    } else {
        let error = CompileError::RunError {
            cmd: cmd.to_owned(),
            args: args.iter().map(|arg| arg.as_ref().to_os_string()).collect(),
            status: status,
        };
        Err(error.into())
    }
}

#[logfn(Trace)]
fn resolve_binary(name: &str) -> Result<PathBuf, Error> {
    let cmd_output = Command::new("which").arg(name).output()?;
    if cmd_output.status.success() {
        let mut output_str = String::from_utf8(cmd_output.stdout)?;
        output_str.pop();
        Ok(output_str.into())
    } else {
        let error = CompileError::ResolveFailureError {
            name: name.to_string(),
            status: cmd_output.status,
        };
        Err(error.into())
    }
}

#[logfn(Trace)]
fn current_sysroot() -> Result<PathBuf, Error> {
    let cmd_output = Command::new("rustc")
        .args(&["--print", "sysroot"])
        .output()?;
    if cmd_output.status.success() {
        let mut output_str = String::from_utf8(cmd_output.stdout)?;
        output_str.pop();
        Ok(output_str.into())
    } else {
        let error = CompileError::UnknownSysrootError {
            status: cmd_output.status,
        };
        Err(error.into())
    }
}
