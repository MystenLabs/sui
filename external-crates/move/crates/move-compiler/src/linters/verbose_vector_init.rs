// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Detects `vector::empty()` and `vector::singleton()` calls that can be replaced with `vector[]`.

use crate::{
    diag,
    linters::StyleCodes,
    shared::Identifier,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    VerboseVectorInit,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        let UnannotatedExp_::ModuleCall(mcall) = &exp.exp.value else {
            return false;
        };

        if !mcall.module.value.named_address_is("std", "vector") {
            return false;
        }

        if mcall.name.value().as_str() == "empty" {
            let msg = "'vector::empty()' can be replaced with literal 'vector[]'";
            let diag = diag!(
                StyleCodes::VerboseVectorInit.diag_info(),
                (exp.exp.loc, msg)
            );
            self.add_diag(diag);
        } else if mcall.name.value().as_str() == "singleton" {
            let msg =
                "'vector::singleton(arg0)' can be replaced with vector literal 'vector[arg0]'";
            let mut diag = diag!(
                StyleCodes::VerboseVectorInit.diag_info(),
                (exp.exp.loc, msg)
            );
            diag.add_note("Instantiation through vector literal is more concise and efficient.");
            self.add_diag(diag);
        }

        false
    }
);
