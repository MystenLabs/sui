// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::ast;
use sui_types::transaction::ProgrammableTransaction;

struct TypingContext {
    inputs: Vec<ast::InputType>,
    results: Vec<ast::ResultType>,
}

pub fn translate(pt: ProgrammableTransaction) -> ast::Transaction {
    let mut context = TypingContext {
        inputs: vec![],
        results: vec![],
    };

    let commands = pt
        .commands
        .into_iter()
        .map(|c| type_command(&mut context, c))
        .collect();
    (context, commands)
}
