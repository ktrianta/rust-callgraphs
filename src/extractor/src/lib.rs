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
extern crate rustc_span;
extern crate syntax;

mod check_unsafety;
mod converters;
mod hir_visitor;
mod mir_visitor;
mod mirai_utils;
mod table_filler;

use hir_visitor::HirVisitor;
use lazy_static::lazy_static;
use rustc::hir::def_id::DefId;
use rustc::hir::intravisit::walk_crate;
use rustc::mir::mono::MonoItem;
use rustc::session::Session;
use rustc::ty::query::Providers;
use rustc::ty::subst::SubstsRef;
use rustc::ty::TyCtxt;
use rustc_data_structures::fx::FxHashSet;
use rustc_interface::interface::Compiler;
use rustc_interface::Queries;
use rustc_mir::monomorphize::collector;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Mutex;

type SubstsSet<'tcx> = FxHashSet<SubstsRef<'tcx>>;
type SubstsMap<'tcx> = HashMap<DefId, SubstsSet<'tcx>>;

/// The struct to share the state among queries.
#[derive(Default)]
struct SharedState {
    /// Does the given function use unsafe operations directly in its body.
    /// (This can be true only for functions marked with `unsafe`.)
    function_unsafe_use: HashMap<DefId, bool>,
    /// What `cfg!` configuration is enabled for this crate?
    crate_cfg: Vec<(String, Option<String>)>,
}

lazy_static! {
    static ref SHARED_STATE: Mutex<SharedState> = Mutex::new(SharedState::default());
}

fn analyse_with_tcx(name: String, tcx: TyCtxt, session: &Session) {
    let hash = tcx.crate_hash(rustc::hir::def_id::LOCAL_CRATE);
    let file_name = format!("{}_{}", name, hash.to_string());
    let cargo_pkg_version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let cargo_pkg_name = std::env::var("CARGO_PKG_NAME").unwrap();
    let mut tables = corpus_database::tables::Tables::default();
    let build = tables.register_builds(
        cargo_pkg_name,
        cargo_pkg_version,
        name,
        hash.as_u64().into(),
        session.opts.edition.to_string(),
    );

    let mut cargo_toml_path: PathBuf = std::env::var("CARGO_MANIFEST_DIR").unwrap().into();
    cargo_toml_path.push("Cargo.toml");
    let mut file = File::open(cargo_toml_path).unwrap();
    let mut cargo_toml_content = String::new();
    file.read_to_string(&mut cargo_toml_content).unwrap();
    let cargo_toml = cargo_toml_content.parse::<toml::Value>().unwrap();
    if let toml::Value::Table(table) = cargo_toml {
        if let Some(toml::Value::Table(package_table)) = table.get("package") {
            if let Some(toml::Value::Array(authors)) = package_table.get("authors") {
                for author in authors {
                    if let toml::Value::String(name) = author {
                        tables.register_crate_authors(build, name.to_string());
                    } else {
                        unreachable!();
                    }
                }
            }
            if let Some(toml::Value::Array(keywords)) = package_table.get("keywords") {
                for keyword in keywords {
                    if let toml::Value::String(name) = keyword {
                        tables.register_crate_keywords(build, name.to_string());
                    } else {
                        unreachable!();
                    }
                }
            }
            if let Some(toml::Value::Array(categories)) = package_table.get("categories") {
                for category in categories {
                    if let toml::Value::String(name) = category {
                        tables.register_crate_categories(build, name.to_string());
                    } else {
                        unreachable!();
                    }
                }
            }
        }
    }

    for crate_type in &session.opts.crate_types {
        tables.register_build_crate_types(build, crate_type.to_string());
    }

    let hir_map = &tcx.hir();
    let krate = hir_map.krate();

    let mut substs_map: SubstsMap = HashMap::new();
    let (set, _) =
        collector::collect_crate_mono_items(tcx, collector::MonoItemCollectionMode::Lazy);
    for item in set.iter() {
        if let MonoItem::Fn(fn_instance) = item {
            if let Some(substs_list) = substs_map.get_mut(&fn_instance.def_id()) {
                substs_list.insert(fn_instance.substs);
            } else {
                let mut set: SubstsSet = FxHashSet::default();
                set.insert(fn_instance.substs);
                substs_map.insert(fn_instance.def_id(), set);
            }
        }
    }

    let mut hir_visitor = HirVisitor::new(tables, substs_map, build, session, hir_map, tcx);
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
        for (key, value) in &state.crate_cfg {
            filler.tables.register_crate_cfgs(
                build,
                key.clone(),
                value
                    .as_ref()
                    .map(String::as_str)
                    .unwrap_or("n/a")
                    .to_string(),
            );
        }
    }

    let tables = filler.tables;
    let mut path: PathBuf = std::env::var("CARGO_TARGET_DIR").unwrap().into();
    path.push("rust-corpus");
    std::fs::create_dir_all(&path).unwrap();
    path.push(file_name);

    if Some("true")
        == std::env::var("CORPUS_OUTPUT_JSON")
            .ok()
            .as_ref()
            .map(|s| s.as_ref())
    {
        tables.save_json(path.clone());
    }
    tables.save_bincode(path);
}

pub fn analyse<'tcx>(compiler: &Compiler, queries: &'tcx Queries<'tcx>) {
    let name = queries.crate_name().unwrap().peek().clone();
    let session = compiler.session();

    queries.global_ctxt().unwrap().peek_mut().enter(|tcx| {
        analyse_with_tcx(name, tcx, session);
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

/// Save `cfg!` configuration.
pub fn save_cfg_configuration(set: &FxHashSet<(String, Option<String>)>) {
    let mut state = SHARED_STATE.lock().unwrap();
    state.crate_cfg = set.iter().cloned().collect();
}
