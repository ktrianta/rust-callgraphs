// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use crate::converters::ConvertInto;
use crate::mir_visitor::MirVisitor;
use crate::table_filler::TableFiller;
use corpus_common::tables::Tables;
use corpus_common::types;
use rustc::hir::{
    self,
    intravisit::{self, Visitor},
    map::Map as HirMap,
    HirId,
};
use rustc::mir;
use rustc::ty::TyCtxt;
use std::mem;
use syntax::source_map::Span;

pub(crate) struct HirVisitor<'a, 'tcx> {
    tcx: TyCtxt<'tcx>,
    hir_map: &'a HirMap<'tcx>,
    filler: TableFiller<'a, 'tcx>,
    current_module: types::Module,
    current_item: Option<types::Item>,
}

impl<'a, 'tcx> HirVisitor<'a, 'tcx> {
    pub fn new(
        crate_name: String,
        crate_hash: u64,
        pkg_name: String,
        pkg_version: String,
        hir_map: &'a HirMap<'tcx>,
        tcx: TyCtxt<'tcx>,
    ) -> Self {
        let mut tables = Tables::default();
        let (_build, root_module) =
            tables.register_build(pkg_name, pkg_version, crate_name, crate_hash);
        let filler = TableFiller::new(tcx, hir_map, tables);
        Self {
            tcx,
            hir_map,
            filler,
            current_module: root_module,
            current_item: None,
        }
    }
    pub fn tables(self) -> Tables {
        self.filler.tables
    }
    fn visit_submodule(
        &mut self,
        name: &str,
        visibility: types::Visibility,
        module: &'tcx hir::Mod,
        id: HirId,
    ) {
        let parent_module = self.current_module;
        let new_module = self
            .filler
            .tables
            .register_submodule(parent_module, name, visibility);
        self.current_module = new_module;
        intravisit::walk_mod(self, module, id);
        self.current_module = parent_module;
    }
    fn visit_static(
        &mut self,
        name: &str,
        visibility: types::Visibility,
        mutability: types::Mutability,
        typ: &'tcx hir::Ty,
        id: HirId,
        body_id: hir::BodyId,
    ) {
        let item =
            self.filler
                .tables
                .register_static(self.current_module, name, visibility, mutability);
        let old_item = mem::replace(&mut self.current_item, Some(item));
        self.visit_id(id);
        self.visit_ty(typ);
        self.visit_nested_body(body_id);
        self.current_item = old_item;
    }
    /// Extract information from unoptmized MIR.
    fn visit_mir(&mut self, body_id: hir::def_id::DefId, body: &mir::BodyAndCache<'tcx>) {
        let error = format!("Mir outside of an item: {:?}", body.span);
        let item = self.current_item.expect(&error);
        let mut mir_visitor = MirVisitor::new(self.tcx, item, body_id, body, &mut self.filler);
        mir_visitor.visit_scopes();
        mir::visit::Visitor::visit_body(&mut mir_visitor, body.unwrap_read_only());
    }
}

impl<'a, 'tcx> Visitor<'tcx> for HirVisitor<'a, 'tcx> {
    fn visit_item(&mut self, item: &'tcx hir::Item) {
        let name: &str = &item.ident.name.as_str();
        let visibility: types::Visibility = item.vis.convert_into();
        match item.kind {
            hir::ItemKind::Mod(ref module) => {
                // This avoids visiting the root module.
                self.visit_submodule(name, visibility, module, item.hir_id);
            }
            hir::ItemKind::Static(ref typ, ref mutability, body_id) => {
                self.visit_static(
                    name,
                    visibility,
                    mutability.convert_into(),
                    typ,
                    item.hir_id,
                    body_id,
                );
            }
            hir::ItemKind::Const(ref typ, body_id) => {
                self.visit_static(
                    name,
                    visibility,
                    types::Mutability::Const,
                    typ,
                    item.hir_id,
                    body_id,
                );
            }
            hir::ItemKind::Impl(unsafety, ..) => {
                let item_id = self.filler.tables.register_impl(
                    self.current_module,
                    name,
                    visibility,
                    unsafety.convert_into(),
                );
                let old_item = mem::replace(&mut self.current_item, Some(item_id));
                intravisit::walk_item(self, item);
                self.current_item = old_item;
            }
            _ => {
                let item_id =
                    self.filler
                        .tables
                        .register_item(self.current_module, name, visibility);
                let old_item = mem::replace(&mut self.current_item, Some(item_id));
                intravisit::walk_item(self, item);
                self.current_item = old_item;
            }
        }
    }
    fn visit_fn(
        &mut self,
        fn_kind: intravisit::FnKind<'tcx>,
        fn_def: &'tcx hir::FnDecl,
        body_id: hir::BodyId,
        span: Span,
        id: HirId,
    ) {
        let def_id = self.filler.resolve_hir_id(id);
        let function = match fn_kind {
            intravisit::FnKind::Method(_name, method_sig, visibility, _attributes) => {
                self.filler.tables.register_function_declaration(
                    self.current_module,
                    def_id,
                    visibility.convert_into(),
                    method_sig.header.unsafety.convert_into(),
                    method_sig.header.abi.name(),
                )
            }
            intravisit::FnKind::ItemFn(_name, _generics, header, visibility, _block) => {
                self.filler.tables.register_function_declaration(
                    self.current_module,
                    def_id,
                    visibility.convert_into(),
                    header.unsafety.convert_into(),
                    header.abi.name(),
                )
            }
            intravisit::FnKind::Closure(_) => self.filler.tables.register_function_declaration(
                self.current_module,
                def_id,
                types::Visibility::Unknown,
                types::Unsafety::Unknown,
                "Closure",
            ),
        };
        let old_item = mem::replace(&mut self.current_item, Some(function));
        intravisit::walk_fn(self, fn_kind, fn_def, body_id, span, id);
        self.current_item = old_item;
    }
    fn visit_foreign_item(&mut self, item: &'tcx hir::ForeignItem) {
        let def_id = self.filler.resolve_hir_id(item.hir_id);
        let visibility = item.vis.convert_into();
        match item.kind {
            hir::ForeignItemKind::Fn(..) => {
                self.filler.tables.register_function_declaration(
                    self.current_module,
                    def_id,
                    visibility,
                    types::Unsafety::Unsafe,
                    "ForeignItem",
                );
            }
            hir::ForeignItemKind::Static(_, mutability) => {
                let name: &str = &item.ident.name.as_str();
                self.filler.tables.register_static(
                    self.current_module,
                    name,
                    visibility,
                    mutability.convert_into(),
                );
            }
            hir::ForeignItemKind::Type => {}
        }
        intravisit::walk_foreign_item(self, item);
    }
    fn visit_body(&mut self, body: &'tcx hir::Body) {
        intravisit::walk_body(self, body);
        let id = body.id();
        let owner = self.hir_map.body_owner_def_id(id);
        let mir_body = self.tcx.optimized_mir(owner);
        self.visit_mir(owner, mir_body);
    }
    fn nested_visit_map<'this>(&'this mut self) -> intravisit::NestedVisitorMap<'this, 'tcx> {
        intravisit::NestedVisitorMap::All(self.hir_map)
    }
}
