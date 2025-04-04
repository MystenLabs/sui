// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { group, indent, line } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'abort_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printAbortExpression;
    }

    return null;
}

/**
 * Print `abort_expression` node.
 */
function printAbortExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const expression = path.node.nonFormattingChildren[0];
    const printed = path.call(print, 'nonFormattingChildren', 0);

    if (!expression) return 'abort';

    return group([
        'abort',
        expression?.isList || expression?.isControlFlow
            ? [' ', printed]
            : [indent(line), indent(printed)],
    ]);
}
