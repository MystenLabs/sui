// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects empty loop expressions, including `while(true) {}` and `loop {}` without exit mechanisms, highlighting potential infinite loops.
//! Aims to identify and warn against loops that may lead to hangs or excessive resource consumption due to lack of content.
//! Encourages adding meaningful logic within loops or ensuring proper exit conditions to improve code reliability and maintainability.
use super::StyleCodes;
use crate::{
    diag,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::{exp_satisfies, simple_visitor},
    },
};

simple_visitor!(
    LoopWithoutExit,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        // we do not care about `while` since there is another lint that handles reporting
        // that `while (true)` should be `loop`
        let UnannotatedExp_::Loop {
            name: _,
            has_break: false,
            body,
        } = &exp.exp.value
        else {
            return false;
        };
        // TODO maybe move this to Loop? Bit of an n^2 problem here in the worst case
        if has_return(body) {
            return false;
        }
        let diag = diag!(
            StyleCodes::LoopWithoutExit.diag_info(),
            (
                exp.exp.loc,
                "'loop' without 'break' or 'return'. \
                This code will until it errors, e.g. reaching an 'abort' or running out of gas"
            )
        );
        self.add_diag(diag);
        false
    }
);

fn has_return(e: &T::Exp) -> bool {
    exp_satisfies(e, |e| matches!(e.exp.value, UnannotatedExp_::Return(_)))
}
