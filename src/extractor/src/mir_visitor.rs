// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use crate::converters::ConvertInto;
use crate::table_filler::TableFiller;
use corpus_database::types;
use rustc::hir;
use rustc::mir;
use rustc::ty::{self, Instance, TyCtxt};
use std::collections::HashMap;

pub(crate) struct MirVisitor<'a, 'b, 'tcx> {
    tcx: TyCtxt<'tcx>,
    body_path: types::DefPath,
    body_id: hir::def_id::DefId,
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
        let (root_scope,) = filler.tables.register_mir_cfgs(item, body_path);
        Self {
            tcx,
            body_path,
            body_id,
            body,
            root_scope,
            filler,
            scopes: HashMap::new(),
        }
    }
    /// Visit MIR and extract all information about it.
    pub fn visit(&mut self) {
        self.visit_scopes();
        let mut basic_blocks = HashMap::new();
        for (basic_block_index, basic_block_data) in self.body.basic_blocks().iter_enumerated() {
            let basic_block_kind = if basic_block_index == mir::START_BLOCK {
                assert!(!basic_block_data.is_cleanup);
                types::BasicBlockKind::Entry
            } else {
                if basic_block_data.is_cleanup {
                    types::BasicBlockKind::CleanUp
                } else {
                    types::BasicBlockKind::Regular
                }
            };
            let (basic_block,) = self
                .filler
                .tables
                .register_basic_blocks(self.body_path, basic_block_kind);
            basic_blocks.insert(basic_block_index, basic_block);
            for (statement_index, statement) in basic_block_data.statements.iter().enumerate() {
                let scope = self.scopes[&statement.source_info.scope];
                let (stmt, stmt_kind) = self.visit_statement(&statement);
                self.filler.tables.register_statements(
                    stmt,
                    basic_block,
                    statement_index.into(),
                    stmt_kind,
                    scope,
                );
            }
        }
        for (basic_block_index, basic_block_data) in self.body.basic_blocks().iter_enumerated() {
            let terminator = basic_block_data.terminator();
            let basic_block = basic_blocks[&basic_block_index];
            let kind = self.visit_terminator(basic_block, &terminator, &basic_blocks);
            let scope = self.scopes[&terminator.source_info.scope];
            self.filler
                .tables
                .register_terminators(basic_block, kind, scope);
        }
    }
    /// Extract information about scopes.
    fn visit_scopes(&mut self) {
        for (scope, scope_data) in self.body.source_scopes.iter_enumerated() {
            let parent_scope = if let Some(ref parent) = scope_data.parent_scope {
                self.scopes[parent]
            } else {
                self.root_scope
            };
            let span = self.filler.register_span(scope_data.span);
            let mir_scope_safety = self.get_scope_safety(scope);
            let (scope_id,) = self.filler.tables.register_subscopes(
                parent_scope,
                mir_scope_safety.convert_into(),
                span,
            );
            self.scopes.insert(scope, scope_id);
        }
    }
    fn get_scope_safety(&self, scope: mir::SourceScope) -> Option<mir::Safety> {
        match self.body.source_scopes[scope].local_data {
            mir::ClearCrossCrate::Set(ref data) => Some(data.safety),
            mir::ClearCrossCrate::Clear => None,
        }
    }
    fn visit_statement(&mut self, statement: &mir::Statement<'tcx>) -> (types::Statement, String) {
        let (stmt, kind) = match &statement.kind {
            mir::StatementKind::Assign(box (place, rvalue)) => {
                let target_type = place.ty(self.body, self.tcx);
                let interned_target_type = self.filler.register_type(target_type.ty);
                let (stmt, kind) = match rvalue {
                    mir::Rvalue::Use(operand) => {
                        let interned_operand = self.visit_operand(operand);
                        let (stmt,) = self
                            .filler
                            .tables
                            .register_statements_assign_use(interned_target_type, interned_operand);
                        (stmt, "Assign/Use")
                    }
                    mir::Rvalue::Repeat(operand, len) => {
                        let interned_operand = self.visit_operand(operand);
                        let (stmt,) = self.filler.tables.register_statements_assign_repeat(
                            interned_target_type,
                            interned_operand,
                            *len,
                        );
                        (stmt, "Assign/Repeat")
                    }
                    mir::Rvalue::Ref(_region, kind, place) => {
                        let place_ty = self.filler.register_type(place.ty(self.body, self.tcx).ty);
                        let (stmt,) = self.filler.tables.register_statements_assign_ref(
                            interned_target_type,
                            place_ty,
                            kind.convert_into(),
                        );
                        (stmt, "Assign/Ref")
                    }
                    mir::Rvalue::AddressOf(mutability, place) => {
                        let place_ty = self.filler.register_type(place.ty(self.body, self.tcx).ty);
                        let (stmt,) = self.filler.tables.register_statements_assign_address(
                            interned_target_type,
                            place_ty,
                            mutability.convert_into(),
                        );
                        (stmt, "Assign/AddressOf")
                    }
                    mir::Rvalue::Len(place) => {
                        let place_ty = self.filler.register_type(place.ty(self.body, self.tcx).ty);
                        let (stmt,) = self
                            .filler
                            .tables
                            .register_statements_assign_len(interned_target_type, place_ty);
                        (stmt, "Assign/Len")
                    }
                    mir::Rvalue::Cast(kind, operand, typ) => {
                        let interned_operand = self.visit_operand(operand);
                        let interned_type = self.filler.register_type(typ);
                        let (stmt,) = self.filler.tables.register_statements_assign_cast(
                            interned_target_type,
                            kind.convert_into(),
                            interned_operand,
                            interned_type,
                        );
                        (stmt, "Assign/Cast")
                    }
                    mir::Rvalue::BinaryOp(op, first, second) => {
                        let first_interned_operand = self.visit_operand(first);
                        let second_interned_operand = self.visit_operand(second);
                        let (stmt,) = self.filler.tables.register_statements_assign_binary_op(
                            interned_target_type,
                            format!("{:?}", op),
                            first_interned_operand,
                            second_interned_operand,
                        );
                        (stmt, "Assign/BinaryOp")
                    }
                    mir::Rvalue::CheckedBinaryOp(op, first, second) => {
                        let first_interned_operand = self.visit_operand(first);
                        let second_interned_operand = self.visit_operand(second);
                        let (stmt,) = self
                            .filler
                            .tables
                            .register_statements_assign_checked_binary_op(
                                interned_target_type,
                                format!("{:?}", op),
                                first_interned_operand,
                                second_interned_operand,
                            );
                        (stmt, "Assign/CheckedBinaryOp")
                    }
                    mir::Rvalue::NullaryOp(op, typ) => {
                        let interned_type = self.filler.register_type(typ);
                        let (stmt,) = self.filler.tables.register_statements_assign_nullary_op(
                            interned_target_type,
                            format!("{:?}", op),
                            interned_type,
                        );
                        (stmt, "Assign/NullaryOp")
                    }
                    mir::Rvalue::UnaryOp(op, operand) => {
                        let interned_operand = self.visit_operand(operand);
                        let (stmt,) = self.filler.tables.register_statements_assign_unary_op(
                            interned_target_type,
                            format!("{:?}", op),
                            interned_operand,
                        );
                        (stmt, "Assign/UnaryOp")
                    }
                    mir::Rvalue::Discriminant(place) => {
                        let place_ty = self.filler.register_type(place.ty(self.body, self.tcx).ty);
                        let (stmt,) = self.filler.tables.register_statements_assign_discriminant(
                            interned_target_type,
                            place_ty,
                        );
                        (stmt, "Assign/Discriminant")
                    }
                    mir::Rvalue::Aggregate(aggregate, operands) => {
                        let (stmt,) = self.filler.tables.register_statements_assign_aggregate(
                            interned_target_type,
                            aggregate.convert_into(),
                        );
                        for (i, operand) in operands.iter().enumerate() {
                            let interned_operand = self.visit_operand(operand);
                            self.filler
                                .tables
                                .register_statements_assign_aggregate_operands(
                                    stmt,
                                    i.into(),
                                    interned_operand,
                                );
                        }
                        (stmt, "Assign/Aggregate")
                    }
                };
                (stmt, kind)
            }
            mir::StatementKind::InlineAsm(box mir::InlineAsm {
                outputs: box outputs,
                inputs: box inputs,
                ..
            }) => {
                let stmt = self.filler.tables.get_fresh_statement();
                for (_, operand) in inputs {
                    let interned_operand = self.visit_operand(operand);
                    self.filler
                        .tables
                        .register_statements_inline_asm_inputs(stmt, interned_operand);
                }
                for place in outputs {
                    let interned_type = self.filler.register_type(place.ty(self.body, self.tcx).ty);
                    self.filler
                        .tables
                        .register_statements_inline_asm_outputs(stmt, interned_type);
                }
                (stmt, "InlineAsm")
            }
            mir::StatementKind::FakeRead(..) => {
                (self.filler.tables.get_fresh_statement(), "FakeRead")
            }
            mir::StatementKind::SetDiscriminant { .. } => {
                (self.filler.tables.get_fresh_statement(), "SetDiscriminant")
            }
            mir::StatementKind::StorageLive(..) => {
                (self.filler.tables.get_fresh_statement(), "StorageLive")
            }
            mir::StatementKind::StorageDead(..) => {
                (self.filler.tables.get_fresh_statement(), "StorageDead")
            }
            mir::StatementKind::Retag(..) => (self.filler.tables.get_fresh_statement(), "Retag"),
            mir::StatementKind::AscribeUserType(..) => {
                (self.filler.tables.get_fresh_statement(), "AscribeUserType")
            }
            mir::StatementKind::Nop => (self.filler.tables.get_fresh_statement(), "Nop"),
        };
        (stmt, kind.to_string())
    }
    fn visit_operand(&mut self, operand: &mir::Operand<'tcx>) -> types::Operand {
        let typ = operand.ty(self.body, self.tcx);
        let interned_type = self.filler.register_type(typ);
        let kind = match operand {
            mir::Operand::Copy(_) => types::OperandKind::Copy,
            mir::Operand::Move(_) => types::OperandKind::Move,
            mir::Operand::Constant(_) => types::OperandKind::Constant,
        };
        let (operand,) = self.filler.tables.register_operands(kind, interned_type);

        operand
    }
    fn visit_terminator(
        &mut self,
        block: types::BasicBlock,
        terminator: &mir::Terminator<'tcx>,
        basic_blocks: &HashMap<mir::BasicBlock, types::BasicBlock>,
    ) -> String {
        let no_block = self.filler.tables.get_no_block();
        let get_maybe_block = |maybe_mir_block: &Option<_>| {
            if let Some(ref mir_block) = maybe_mir_block {
                basic_blocks[mir_block]
            } else {
                no_block
            }
        };
        let kind = match &terminator.kind {
            mir::TerminatorKind::Goto { target } => {
                self.filler
                    .tables
                    .register_terminators_goto(block, basic_blocks[target]);
                "Goto"
            }
            mir::TerminatorKind::SwitchInt {
                discr,
                switch_ty,
                values,
                targets,
            } => {
                let discriminant = self.visit_operand(&discr);
                let typ = self.filler.register_type(switch_ty);
                self.filler
                    .tables
                    .register_terminators_switch_int(block, discriminant, typ);
                for (value, target) in values.iter().zip(targets) {
                    self.filler.tables.register_terminators_switch_int_targets(
                        block,
                        *value,
                        basic_blocks[target],
                    );
                }
                "SwitchInt"
            }
            mir::TerminatorKind::Resume => "Resume",
            mir::TerminatorKind::Abort => "Abort",
            mir::TerminatorKind::Return => "Return",
            mir::TerminatorKind::Unreachable => "Unreachable",
            mir::TerminatorKind::Drop {
                location,
                target,
                unwind,
            } => {
                let location_type = self
                    .filler
                    .register_type(location.ty(self.body, self.tcx).ty);
                let unwind_block = get_maybe_block(unwind);
                self.filler.tables.register_terminators_drop(
                    block,
                    location_type,
                    basic_blocks[target],
                    unwind_block,
                );
                "Drop"
            }
            mir::TerminatorKind::DropAndReplace {
                location,
                value,
                target,
                unwind,
            } => {
                let location_type = self
                    .filler
                    .register_type(location.ty(self.body, self.tcx).ty);
                let unwind_block = get_maybe_block(unwind);
                let interned_operand = self.visit_operand(value);
                self.filler.tables.register_terminators_drop_and_replace(
                    block,
                    location_type,
                    interned_operand,
                    basic_blocks[target],
                    unwind_block,
                );
                "DropAndReplace"
            }
            mir::TerminatorKind::Call {
                func,
                args,
                destination,
                cleanup,
                from_hir_call: _,
            } => {
                let interned_func = self.visit_operand(func);
                let (return_ty, destination_block) =
                    if let Some((target_place, target_block)) = destination {
                        (
                            target_place.ty(self.body, self.tcx).ty,
                            basic_blocks[target_block],
                        )
                    } else {
                        (self.tcx.mk_unit(), no_block)
                    };
                let interned_return_ty = self.filler.register_type(return_ty);
                let func_ty = func.ty(self.body, self.tcx);
                let sig = func_ty.fn_sig(self.tcx);
                let unsafety = sig.unsafety().convert_into();
                let abi = sig.abi().name().to_string();
                let (function_call,) = self.filler.tables.register_terminators_call(
                    block,
                    interned_func,
                    unsafety,
                    abi,
                    interned_return_ty,
                    destination_block,
                    get_maybe_block(cleanup),
                );
                for (i, arg) in args.iter().enumerate() {
                    let interned_arg = self.visit_operand(arg);
                    self.filler.tables.register_terminators_call_arg(
                        function_call,
                        i.into(),
                        interned_arg,
                    );
                }
                match func {
                    mir::Operand::Constant(constant) => {
                        if let ty::TyKind::FnDef(id, substs) = constant.literal.ty.kind {
                            use rustc::ty::ParamEnv;
                            let mut is_generic = false;
                            let mut instances = Vec::new();
                            for typ in substs.types() {
                                match typ.kind {
                                    rustc::ty::TyKind::Param(_) => is_generic = true,
                                    _ => {}
                                }
                            }
                            if let Some(caller_substs_list) = self.filler.substs_map.get(&self.body_id) {
                                for caller_substs in caller_substs_list {
                                    let mut updated_substs = Vec::new();
                                    for subst in substs {
                                        let subst = self.tcx.subst_and_normalize_erasing_regions(caller_substs, ParamEnv::reveal_all(), subst);
                                        updated_substs.push(subst);
                                    }
                                    let updated_substs = self.tcx.mk_substs(updated_substs.iter());
                                    let instance_opt = Instance::resolve(self.tcx, ParamEnv::reveal_all(), id, updated_substs);
                                    if let Some(instance) = instance_opt {
                                        instances.push(instance);
                                    }
                                }
                            }
                            for instance in &instances {
                                if let Some(subs_list) = self.filler.substs_map.get_mut(&instance.def_id()) {
                                    subs_list.insert(instance.substs);
                                } else {
                                    use rustc_data_structures::fx::FxHashSet;
                                    let mut set: FxHashSet<rustc::ty::subst::SubstsRef> = FxHashSet::default();
                                    set.insert(instance.substs);
                                    self.filler.substs_map.insert(instance.def_id(), set);
                                }
                            }
                            let def_path = self.filler.resolve_def_id(id);
                            self.filler
                                .tables
                                .register_terminators_call_const_target(function_call, def_path);

                            if is_generic {
                                for instance in &instances {
                                    let instance_def_path = self.filler.resolve_def_id(instance.def_id());
                                    self.filler
                                        .tables
                                        .register_instantiations(function_call, instance_def_path);
                                }
                            }
                            if is_generic {
                                self.filler
                                    .tables
                                    .register_call_graph(function_call, self.body_path, def_path);
                                self.filler
                                    .tables
                                    .register_generic_calls(def_path);
                            } else if !instances.is_empty() {
                                let instance = instances[0];
                                let def_path = self.filler.resolve_def_id(instance.def_id());
                                self.filler
                                    .tables
                                    .register_call_graph(function_call, self.body_path, def_path);
                                if let ty::InstanceDef::Virtual(..) = instance.def {
                                    self.filler
                                        .tables
                                        .register_virtual_calls(def_path);
                                }
                            }
                        } else {
                            unreachable!("Unexpected called constant type: {:?}", constant);
                        }
                    }
                    mir::Operand::Copy(_) | mir::Operand::Move(_) => {}
                };
                "Call"
            }
            mir::TerminatorKind::Assert {
                cond,
                expected,
                msg: _,
                target,
                cleanup,
            } => {
                let interned_cond = self.visit_operand(cond);
                self.filler.tables.register_terminators_assert(
                    block,
                    interned_cond,
                    *expected,
                    basic_blocks[target],
                    get_maybe_block(cleanup),
                );
                "Assert"
            }
            mir::TerminatorKind::Yield {
                value,
                resume,
                drop,
            } => {
                let interned_value = self.visit_operand(value);
                self.filler.tables.register_terminators_yield(
                    block,
                    interned_value,
                    basic_blocks[resume],
                    get_maybe_block(drop),
                );
                "Yield"
            }
            mir::TerminatorKind::GeneratorDrop => "GeneratorDrop",
            mir::TerminatorKind::FalseEdges {
                real_target,
                imaginary_target,
            } => {
                self.filler.tables.register_terminators_false_edges(
                    block,
                    basic_blocks[real_target],
                    basic_blocks[imaginary_target],
                );
                "FalseEdges"
            }
            mir::TerminatorKind::FalseUnwind {
                real_target,
                unwind,
            } => {
                self.filler.tables.register_terminators_false_unwind(
                    block,
                    basic_blocks[real_target],
                    get_maybe_block(unwind),
                );
                "FalseUnwind"
            }
        };
        kind.to_string()
    }
}
