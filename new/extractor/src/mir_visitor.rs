// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use crate::converters::ConvertInto;
use crate::table_filler::TableFiller;
use corpus_common::types;
use rustc::hir;
use rustc::mir::{self, visit::Visitor};
use rustc::ty::{self, TyCtxt};
use std::collections::HashMap;

pub(crate) struct MirVisitor<'a, 'b, 'tcx> {
    tcx: TyCtxt<'tcx>,
    body: &'a mir::Body<'tcx>,
    filler: &'a mut TableFiller<'b, 'tcx>,
    root_scope: types::Scope,
    scopes: HashMap<mir::SourceScope, types::Scope>,
}

impl<'a, 'b, 'tcx> MirVisitor<'a, 'b, 'tcx> {
    pub fn new(
        tcx: TyCtxt<'tcx>,
        item: types::Item,
        body_id: hir::def_id::DefId,
        body: &'a mir::Body<'tcx>,
        filler: &'a mut TableFiller<'b, 'tcx>,
    ) -> Self {
        let body_path = filler.resolve_def_id(body_id);
        let root_scope = filler.tables.register_mir(item, body_path);
        Self {
            tcx,
            body,
            root_scope,
            filler,
            scopes: HashMap::new(),
        }
    }
    /// Extract information about scopes.
    pub fn visit_scopes(&mut self) {
        for (scope, scope_data) in self.body.source_scopes.iter_enumerated() {
            let parent_scope = if let Some(ref parent) = scope_data.parent_scope {
                self.scopes[parent]
            } else {
                self.root_scope
            };
            let mir_scope_safety = self.get_scope_safety(scope);
            let scope_id = self
                .filler
                .tables
                .register_subscope(parent_scope, mir_scope_safety.convert_into());
            self.scopes.insert(scope, scope_id);
            // if let Some(mir::Safety::ExplicitUnsafe(hir_id)) = mir_scope_safety {
            //     let unsafe_def_id = self.filler.resolve_hir_id(hir_id);
            //     self.filler
            //         .tables
            //         .register_unsafe_block_scope(unsafe_def_id, scope_id);
            // }
        }
    }
    fn get_scope_safety(&self, scope: mir::SourceScope) -> Option<mir::Safety> {
        match self.body.source_scope_local_data {
            mir::ClearCrossCrate::Set(ref data) => Some(data[scope].safety),
            mir::ClearCrossCrate::Clear => None,
        }
    }
}

impl<'a, 'b, 'tcx> Visitor<'tcx> for MirVisitor<'a, 'b, 'tcx> {
    fn visit_terminator(&mut self, terminator: &mir::Terminator<'tcx>, location: mir::Location) {
        match terminator.kind {
            mir::TerminatorKind::Goto { .. }
            | mir::TerminatorKind::SwitchInt { .. }
            | mir::TerminatorKind::Drop { .. }
            | mir::TerminatorKind::Yield { .. }
            | mir::TerminatorKind::Assert { .. }
            | mir::TerminatorKind::DropAndReplace { .. }
            | mir::TerminatorKind::GeneratorDrop
            | mir::TerminatorKind::Resume
            | mir::TerminatorKind::Abort
            | mir::TerminatorKind::Return
            | mir::TerminatorKind::Unreachable
            | mir::TerminatorKind::FalseEdges { .. }
            | mir::TerminatorKind::FalseUnwind { .. } => {}
            mir::TerminatorKind::Call { ref func, .. } => {
                let func_ty = func.ty(self.body, self.tcx);
                let sig = func_ty.fn_sig(self.tcx);

                let unsafety = sig.unsafety().convert_into();
                let abi = sig.abi().name();
                let scope = self.scopes[&terminator.source_info.scope];
                let call = self.filler.tables.register_call(scope, unsafety, abi);
                match func {
                    mir::Operand::Constant(constant) => {
                        if let ty::TyKind::FnDef(target_id, _) = constant.literal.ty.sty {
                            let id = self.filler.resolve_def_id(target_id);
                            self.filler.tables.register_const_call_target(call, id);
                        } else {
                            unreachable!("Unexpected called constant type: {:?}", constant);
                        }
                    }
                    mir::Operand::Copy(_) | mir::Operand::Move(_) => {}
                };
            }
        }
        self.super_terminator(terminator, location);
    }
}
