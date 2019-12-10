// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use crate::mirai_utils;
use corpus_common::tables::Tables;
use corpus_common::types;
use rustc::hir::{self, map::Map as HirMap, HirId};
use rustc::ty::TyCtxt;

/// A wrapper around `Tables` that keeps some local state.
pub(crate) struct TableFiller<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    hir_map: &'a HirMap<'tcx>,
    pub(crate) tables: Tables,
}

impl<'a, 'tcx> TableFiller<'a, 'tcx> {
    pub fn new(tcx: TyCtxt<'tcx>, hir_map: &'a HirMap<'tcx>, tables: Tables) -> Self {
        Self {
            tcx,
            hir_map,
            tables,
        }
    }
    pub fn resolve_hir_id(&mut self, id: HirId) -> types::DefPath {
        let def_id = self.hir_map.local_def_id(id);
        self.resolve_def_id(def_id)
    }
    pub fn resolve_def_id(&mut self, def_id: hir::def_id::DefId) -> types::DefPath {
        let crate_num = def_id.krate;
        let crate_name = &self.tcx.crate_name(crate_num).as_str();
        let crate_hash = self.tcx.crate_hash(crate_num).as_u64().into();
        let def_path_str = self.tcx.def_path_debug_str(def_id);
        let def_path_hash = self.tcx.def_path_hash(def_id).0.as_value().into();
        let summary_key_str = mirai_utils::summary_key_str(self.tcx, def_id);
        let summary_key_str_value = std::rc::Rc::try_unwrap(summary_key_str).unwrap();
        self.tables
            .register_def_path(crate_name, crate_hash, def_path_str, def_path_hash, summary_key_str_value)
    }
}
