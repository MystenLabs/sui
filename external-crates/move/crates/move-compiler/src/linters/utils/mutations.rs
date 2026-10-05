// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diagnostics::filter::FilterScope,
    naming::ast::TypeInner,
    typing::{
        ast::{self as T},
        visitor::TypingVisitorContext,
    },
};
use move_ir_types::location::Loc;

#[derive(Clone, Copy, Debug)]
pub(crate) enum MutationSite {
    Assign(Loc),
    Mutate(Loc),
    CallWithMutableReference(Loc),
}

impl MutationSite {
    pub(crate) fn loc(self) -> Loc {
        match self {
            Self::Assign(loc) | Self::Mutate(loc) | Self::CallWithMutableReference(loc) => loc,
        }
    }
}

pub(crate) fn first_mutation(exp: &T::Exp) -> Option<MutationSite> {
    let mut detector = MutationDetector { site: None };
    detector.visit_exp(exp);
    detector.site
}

pub(crate) fn call_takes_mutable_reference(call: &T::ModuleCall) -> bool {
    call.parameter_types
        .iter()
        .any(|ty| matches!(ty.value.inner(), TypeInner::Ref(true, _)))
}

struct MutationDetector {
    site: Option<MutationSite>,
}

impl TypingVisitorContext for MutationDetector {
    fn push_warning_filter_scope(&mut self, _filters: FilterScope) {}

    fn pop_warning_filter_scope(&mut self) {}

    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        use T::UnannotatedExp_ as E;

        if self.site.is_some() {
            return true;
        }
        let site = match &exp.exp.value {
            E::Assign(_, _, _) => MutationSite::Assign(exp.exp.loc),
            E::Mutate(_, _) => MutationSite::Mutate(exp.exp.loc),
            E::ModuleCall(call) if call_takes_mutable_reference(call) => {
                MutationSite::CallWithMutableReference(exp.exp.loc)
            }
            _ => return false,
        };
        self.site = Some(site);
        true
    }
}
