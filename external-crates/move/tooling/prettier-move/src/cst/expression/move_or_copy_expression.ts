// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const {} = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'move_or_copy_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printMoveOrCopyExpression;
    }

    return null;
}

/**
 * Print `move_or_copy_expression` node.
 */
function printMoveOrCopyExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const ref = path.node.child(0)!.text == 'move' ? ['move', ' '] : ['copy', ' '];
    return [
        ...ref,
        path.call(print, 'nonFormattingChildren', 0), // expression
    ];
}
