// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Detects excessive nesting of control flow structures (if/while/loop)
//! Aims to improve code readability by encouraging flatter code structure
use crate::{
    diag,
    linters::StyleCodes,
    typing::{
        ast::{self as H},
        visitor::simple_visitor,
    },
};
const MAX_NESTING_LEVEL: u8 = 3;

simple_visitor!(
    NestingExceed,
    fn visit_exp_custom(&mut self, exp: &H::Exp) -> bool {
        let nesting_level = calculate_nesting_level(&exp.exp.value, 0);

        if nesting_level > MAX_NESTING_LEVEL {
            let msg = format!(
                "Nesting level of {} exceeds maximum allowed ({})",
                nesting_level, MAX_NESTING_LEVEL
            );
            self.add_diag(diag!(
                StyleCodes::ExcessiveNesting.diag_info(),
                (exp.exp.loc, msg),
                (exp.exp.loc, "Consider refactoring to reduce nesting"),
            ));
        }

        false
    }
);

fn calculate_nesting_level(exp: &H::UnannotatedExp_, current_level: u8) -> u8 {
    match exp {
        H::UnannotatedExp_::Block(seq) => seq
            .1
            .iter()
            .map(|sp!(_, item)| {
                if let H::SequenceItem_::Seq(e) = item {
                    calculate_nesting_level(&e.exp.value, current_level)
                } else {
                    current_level
                }
            })
            .max()
            .unwrap_or(current_level),
        H::UnannotatedExp_::IfElse(_, if_block, else_block) => {
            let next_level = current_level + 1;
            let if_level = calculate_nesting_level(&if_block.exp.value, next_level);
            let else_level = else_block
                .as_ref()
                .map(|e| calculate_nesting_level(&e.exp.value, next_level))
                .unwrap_or(next_level);
            if_level.max(else_level)
        }
        H::UnannotatedExp_::While(_, _, block) => {
            calculate_nesting_level(&block.exp.value, current_level + 1)
        }
        H::UnannotatedExp_::Loop { body, .. } => {
            calculate_nesting_level(&body.exp.value, current_level + 1)
        }
        _ => current_level,
    }
}
