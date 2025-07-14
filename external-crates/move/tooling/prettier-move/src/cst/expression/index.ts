// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

/**
 * Route through `_expression` nodes.
 *
 * @module src/cst/expression
 */

import { Node } from '../..';
import { treeFn } from '../../printer';
import { AstPath } from 'prettier';

// Folder imports:
import abort_expression from './abort_expression';
import annotation_expression from './annotation_expression';
import assign_expression from './assign_expression';
import binary_expression from './binary_expression';
import block_item from './block_item';
import block from './block';
import borrow_expression from './borrow_expression';
import break_expression from './break_expression';
import call_expression from './call_expression';
import cast_expression from './cast_expression';
import continue_expression from './continue_expression';
import dereference_expression from './dereference_expression';
import dot_expression from './dot_expression';
import expression_list from './expression_list';
import if_expression from './if_expression';
import identified_expression from './identified_expression';
import index_expression from './index_expression';
import lambda_expression from './lambda_expression';
import let_statement from './let_statement';
import loop_expression from './loop_expression';
import macro_call_expression from './macro_call_expression';
import match_expression from './match_expression';
import move_or_copy_expression from './move_or_copy_expression';
import name_expression from './name_expression';
import pack_expression from './pack_expression';
import return_expression from './return_expression';
import unary_expression from './unary_expression';
import unit_expression from './unit_expression';
import vector_expression from './vector_expression';
import while_expression from './while_expression';

export default function (path: AstPath<Node>): treeFn | null {
    // route to separated functions
    const result =
        abort_expression(path) ||
        annotation_expression(path) ||
        assign_expression(path) ||
        binary_expression(path) ||
        block_item(path) ||
        block(path) ||
        borrow_expression(path) ||
        break_expression(path) ||
        call_expression(path) ||
        cast_expression(path) ||
        continue_expression(path) ||
        dereference_expression(path) ||
        dot_expression(path) ||
        expression_list(path) ||
        if_expression(path) ||
        identified_expression(path) ||
        index_expression(path) ||
        lambda_expression(path) ||
        let_statement(path) ||
        loop_expression(path) ||
        macro_call_expression(path) ||
        match_expression(path) ||
        move_or_copy_expression(path) ||
        name_expression(path) ||
        pack_expression(path) ||
        return_expression(path) ||
        unary_expression(path) ||
        unit_expression(path) ||
        vector_expression(path) ||
        while_expression(path);

    if (result !== null) {
        return result;
    }

    return null;
}
