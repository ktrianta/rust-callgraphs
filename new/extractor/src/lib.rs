// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

#![feature(rustc_private)]
#![feature(box_patterns)]
#![feature(slice_patterns)]
#![feature(bool_to_option)]

extern crate rustc;
extern crate rustc_data_structures;
extern crate rustc_error_codes;
extern crate rustc_interface;
extern crate rustc_metadata;
extern crate rustc_mir;
extern crate syntax;

mod check_unsafety;
mod converters;
mod hir_visitor;
mod mir_visitor;
mod mirai_utils;
mod table_filler;

use rustc::hir::def_id::DefId;
use rustc::hir::intravisit::walk_crate;
use rustc::session::Session;
use rustc::ty::query::Providers;
use rustc::ty::TyCtxt;
use rustc_interface::interface::Compiler;
use rustc_interface::Queries;
use std::path::PathBuf;

fn analyse_with_tcx(name: String, tcx: TyCtxt) {
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

pub fn analyse<'tcx>(compiler: &Compiler, queries: &'tcx Queries<'tcx>) {
    let name = queries.crate_name().unwrap().peek().clone();

    queries.global_ctxt().unwrap().peek_mut().enter(move |tcx| {
        analyse_with_tcx(name, tcx);
    });
}

pub fn override_queries(
    _session: &Session,
    providers: &mut Providers,
    _providers_extern: &mut Providers,
) {
    providers.unsafety_check_result = unsafety_check_result;
}

fn unsafety_check_result(tcx: TyCtxt<'_>, def_id: DefId) -> rustc::mir::UnsafetyCheckResult {
    let mut providers = Providers::default();
    rustc_mir::provide(&mut providers);
    let original_unsafety_check_result = providers.unsafety_check_result;
    check_unsafety::unsafety_check_result(tcx, def_id); // TODO: Extract information whether an unsafe function does not use unsafe.
    original_unsafety_check_result(tcx, def_id)
}
