// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::stackless::ast::Function;

mod inline_immediates;

pub fn optimize(function: &mut Function) {
    inline_immediates::optimize(function);
}
