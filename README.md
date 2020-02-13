# Rust Call-Graph Generator

## Prerequisites

* Install rust (via rustup).
* Install the rustc dev component for your distribution via `rustup component add rustc-dev-{your_distro}`.
* Install docker.

## How to setup and compile

```
mkdir workspace
cd src
cargo build --all --release
```

## How to run

### Step 0

Create the list of crates to compile (i.e., analyze). There are two alternatives.

* Get the top most used N crates with `cargo run --release init [--all-versions] <N>`.
* Get all available crates on crates.io with `cargo run --release init-all [--all-versions]`.

The above commands create a CrateList.json file which is read by our tool at Step 1.

### Step 1

Compile (sequentially) all crates and their dependencies in CrateList.json with `cargo run --release compile`.

Possible command line options:
* **--workspace &lt;workspace&gt;**
    The directory in which all crates are compiled. [default: ../workspace]
* **--crate-list-path &lt;crate-list-path&gt;**
    The file specifying crates and their versions. [default: CrateList.json]
* **--max-log-size &lt;max-log-size&gt;**
    The maximum log size per build before it gets truncated (in bytes). [default: 5242880]
* **--memory-limit &lt;memory-limit&gt;**
    The memory limit that is set while building a crate. 0 means no limit. [default: 4000000000]

In this step the tool uses the Rustwide library, to assist in the setup and compilation process.

Taken from the Rustwide GitHub repository:

> Rustwide is a library to execute your code on the Rust ecosystem, powering projects like Crater and docs.rs. It features:
> * Linux and Windows support.
> * Sandboxing by default using Docker containers, with the option to restrict network access during builds while still supporting most of the crates.
> * Curated build environment to build a large part of the ecosystem, built from the experience gathered running Crater and docs.rs.

To achieve these Rustwide **creates a Docker container** from the rustops/crates-build-env image and compiles the
crates inside it.

In the Rustwide workspace directory (default: ../workspace) exist the following directories:
* **builds**
    contains the compiled code of each crate.
* **cache**
    contains the crates downloaded by crates.io.
* **cargo-home and rustup-home**
    contain the rust binaries and toolchains installed by Rustwide during setup.
* **rust-corpus**
    contains one directory per compiled packaged with all the extracted information stored in binary files.

### Step 2

Merge all binary files from the `<workspace>/rust-corpus` directory with `cargo run --release update-database`.

Possible command line options:
* **--database &lt;database-root&gt;**
    The directory in which the database is stored. [default: ../database]

The merged files are stored in the <database-root> directory. If we subsequently compile more packages we can run again
the `update-database` command to add the extracted knowledge into the database.

**Note that the database is basically binary files and the update process is not protected by a lock**.

### Step 3

Run the analysis on the data stored in the "database", basically all the compiled packages and their dependencies.

The analysis code can be found under `src/analysis`. Run with `cargo run --release [-- --database <database-root>] > callgraph.json`.

Possible command line options:
* **--database &lt;database-root&gt;**
    The directory in which the database is stored. [default: ../../database]

### An example run with the top 10 crates on crates.io

```
cargo run --release init 10
cargo run --release compile
cargo run --release update-database
cd analysis
cargo run --release > callgraph.json
```

## Available command line arguments

```
cd src
cargo run
```
