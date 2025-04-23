// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const {} = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'loop_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printLoopExpression;
    }

    return null;
}

/**
 * Print `loop_expression` node.
 *
 * ```
 * loop `_expression`
 * ```
 */
function printLoopExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    return [`loop `, path.call(print, 'nonFormattingChildren', 0)];
}
