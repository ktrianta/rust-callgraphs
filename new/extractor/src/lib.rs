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

use lazy_static::lazy_static;
use rustc::hir::def_id::DefId;
use rustc::hir::intravisit::walk_crate;
use rustc::session::Session;
use rustc::ty::query::Providers;
use rustc::ty::TyCtxt;
use rustc_interface::interface::Compiler;
use rustc_interface::Queries;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;

/// The struct to share the state among queries.
#[derive(Default)]
struct SharedState {
    /// Does the given function use unsafe operations directly in its body.
    /// (This can be true only for functions marked with `unsafe`.)
    function_unsafe_use: HashMap<DefId, bool>,
}

lazy_static! {
    static ref SHARED_STATE: Mutex<SharedState> = Mutex::new(SharedState::default());
}

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

    let mut filler = hir_visitor.filler();

    {
        let state = SHARED_STATE.lock().unwrap();
        for (def_id, uses_unsafe) in state.function_unsafe_use.iter() {
            let def_path = filler.resolve_def_id(def_id.clone());
            filler
                .tables
                .register_function_unsafe_use(def_path, *uses_unsafe);
        }
    }

    let tables = filler.tables;
    let mut path: PathBuf = std::env::var("RUST_CORPUS_DATA_PATH").unwrap().into();
    path.push(file_name);

    tables.save_json(path.clone());
    tables.save_bincode(path);
}

pub fn analyse<'tcx>(_compiler: &Compiler, queries: &'tcx Queries<'tcx>) {
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
    let result = check_unsafety::unsafety_check_result(tcx, def_id);
    {
        let mut state = SHARED_STATE.lock().unwrap();
        state.function_unsafe_use.insert(def_id, result);
    }

    original_unsafety_check_result(tcx, def_id)
}
