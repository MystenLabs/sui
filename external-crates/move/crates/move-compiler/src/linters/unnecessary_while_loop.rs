// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Encourages replacing `while(true)` with `loop` for infinite loops in Move for clarity and conciseness.
//! Identifies `while(true)` patterns, suggesting a more idiomatic approach using `loop`.
//! Aims to enhance code readability and adherence to Rust idioms.
use crate::{
    diag,
    expansion::ast::Value_,
    linters::StyleCodes,
    typing::{
        ast::{self as T, UnannotatedExp_},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    WhileTrueToLoop,
    fn visit_exp_custom(&mut self, exp: &T::Exp) -> bool {
        let UnannotatedExp_::While(_, cond, _) = &exp.exp.value else {
            return false;
        };
        let UnannotatedExp_::Value(sp!(_, Value_::Bool(true))) = &cond.exp.value else {
            return false;
        };

        let msg = "'while (true)' can be always replaced with 'loop'";
        let mut diag = diag!(StyleCodes::WhileTrueToLoop.diag_info(), (exp.exp.loc, msg));
        diag.add_note(
            "A 'loop' is more useful in these cases. Unlike 'while', 'loop' can have a \
            'break' with a value, e.g. 'let x = loop { break 42 };'",
        );
        self.add_diag(diag);

        false
    }
);
