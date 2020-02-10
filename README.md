# Rust Call-Graph Generator

## Prerequisites

* Install rust (via rustup).
* Install the rustc dev component for your distribution via `rustup component add rustc-dev-{your_distro}`.
* Install docker.

## How to run

```
mkdir workspace
cd src
cargo build --all
cargo run init 5  # initialize with top 5 crates
cargo run compile
cargo run update-database
cd analysis
cargo run > callgraph.json
```

## Available command line arguments

```
cd src
cargo run
```
