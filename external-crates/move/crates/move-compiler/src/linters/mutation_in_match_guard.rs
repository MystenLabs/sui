// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    diag,
    linters::{StyleCodes, utils::mutations},
    typing::{
        ast::{self as T},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    MutationInMatchGuard,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        let T::UnannotatedExp_::Match(_, arms) = &exp.exp.value else {
            return false;
        };
        for arm in &arms.value {
            let Some(guard) = &arm.value.guard else {
                continue;
            };
            let Some(mutation) = mutations::first_mutation(guard) else {
                continue;
            };
            let mut diag = diag!(
                StyleCodes::MutationInMatchGuard.diag_info(),
                (guard.exp.loc, "Match guard may mutate state"),
                (mutation.loc(), "This may mutate state"),
            );
            diag.add_note(
                "Match guards may be run multiple times during match decisions, and should not be \
                 used to mutate state.",
            );
            self.add_diag(diag);
        }
        false
    }
);
