# Rust Call-Graph Generator

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
