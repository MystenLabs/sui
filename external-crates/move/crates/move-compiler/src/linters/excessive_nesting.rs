// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects nested if conditions that can be combined using logical AND
//! Aims to improve code readability by flattening nested if statements

use crate::{
    diag,
    linters::StyleCodes,
    typing::{
        ast::{self as H},
        visitor::simple_visitor,
    },
};

simple_visitor!(
    NestingExceed,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        if !matches!(&exp.exp.value, H::UnannotatedExp_::IfElse(..)) {
            return false;
        }
        let if_block = match &exp.exp.value {
            H::UnannotatedExp_::IfElse(_, block, _) => block,
            _ => unreachable!(),
        };

        let seq_items = match &if_block.exp.value {
            H::UnannotatedExp_::Block((_, items)) => items,
            _ => return false,
        };
        if seq_items.is_empty() {
            return false;
        }

        let sp!(_, first_item) = &seq_items[0];

        let inner_exp = match first_item {
            H::SequenceItem_::Seq(e) => e,
            _ => return false,
        };

        if matches!(&inner_exp.exp.value, H::UnannotatedExp_::IfElse(..)) {
            let msg = "Nested if statements can be combined";
            let help = "Consider combining conditions with && if possible".to_string();

            self.add_diag(diag!(
                StyleCodes::ExcessiveNesting.diag_info(),
                (exp.exp.loc, msg),
                (exp.exp.loc, help),
            ));
        }

        false
    }
);
