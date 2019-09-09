// Licensed under the MIT license <LICENSE or
// http://opensource.org/licenses/MIT>. This file may not be copied,
// modified, or distributed except according to those terms.

use corpus_common::types;
use rustc::hir;
use rustc::mir;

pub trait ConvertInto<T> {
    fn convert_into(&self) -> T;
}

impl ConvertInto<types::Visibility> for hir::Visibility {
    fn convert_into(&self) -> types::Visibility {
        match self.node {
            hir::VisibilityKind::Public => types::Visibility::Public,
            hir::VisibilityKind::Crate(_) => types::Visibility::Crate,
            hir::VisibilityKind::Restricted { .. } => types::Visibility::Restricted,
            hir::VisibilityKind::Inherited => types::Visibility::Private,
        }
    }
}

impl ConvertInto<types::Visibility> for Option<&hir::Visibility> {
    fn convert_into(&self) -> types::Visibility {
        match self {
            Some(visibility) => visibility.convert_into(),
            None => types::Visibility::Unknown,
        }
    }
}

impl ConvertInto<types::Unsafety> for hir::Unsafety {
    fn convert_into(&self) -> types::Unsafety {
        match self {
            hir::Unsafety::Unsafe => types::Unsafety::Unsafe,
            hir::Unsafety::Normal => types::Unsafety::Normal,
        }
    }
}

impl ConvertInto<types::Mutability> for hir::Mutability {
    fn convert_into(&self) -> types::Mutability {
        match self {
            hir::MutMutable => types::Mutability::Mutable,
            hir::MutImmutable => types::Mutability::Immutable,
        }
    }
}

impl ConvertInto<types::ScopeSafety> for Option<mir::Safety> {
    fn convert_into(&self) -> types::ScopeSafety {
        match self {
            Some(mir::Safety::Safe) => types::ScopeSafety::Safe,
            Some(mir::Safety::BuiltinUnsafe) => types::ScopeSafety::BuiltinUnsafe,
            Some(mir::Safety::FnUnsafe) => types::ScopeSafety::FnUnsafe,
            Some(mir::Safety::ExplicitUnsafe(_)) => types::ScopeSafety::ExplicitUnsafe,
            None => types::ScopeSafety::Unknown,
        }
    }
}
