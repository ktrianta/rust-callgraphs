// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

#![feature(rustc_private)]

extern crate rustc;
extern crate rustc_data_structures;
extern crate rustc_interface;
extern crate rustc_metadata;
extern crate syntax;

mod converters;
mod hir_visitor;
mod mir_visitor;
mod mirai_utils;
mod table_filler;

use rustc::hir::intravisit::walk_crate;
use rustc::ty::TyCtxt;
use rustc_interface::interface::Compiler;
use std::path::PathBuf;

pub fn analyse(compiler: &Compiler, tcx: TyCtxt) {
    let name = compiler.crate_name().unwrap().peek().clone();
    let hash = tcx.crate_hash(rustc::hir::def_id::LOCAL_CRATE);
    let cargo_pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let cargo_pkg_name = std::env::var("CARGO_PKG_NAME").unwrap();

    // TODO:
    // let _parsed_crate = compiler.parse().unwrap().peek();
    // let _expanded_crate = compiler.expansion().unwrap().peek();

    let hir_map = &tcx.hir();
    let krate = hir_map.krate();

    let file_name = format!("{}_{}", name, hash.to_string());
    let mut hir_visitor = hir_visitor::HirVisitor::new(
        name,
        hash.as_u64(),
        cargo_pkg_name,
        cargo_pkg_version,
        hir_map,
        tcx,
    );

    walk_crate(&mut hir_visitor, krate);

    let tables = hir_visitor.tables();
    let mut path: PathBuf = std::env::var("RUST_CORPUS_DATA_PATH").unwrap().into();
    path.push(file_name);

    tables.save_json(path.clone());
    tables.save_bincode(path);
}
