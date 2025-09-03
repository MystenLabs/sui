// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { join, indent } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'return_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printReturnExpression;
    }

    return null;
}

/**
 * Print `return_expression` node.
 */
function printReturnExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const nodes = path.node.nonFormattingChildren;

    if (nodes.length === 0) {
        return 'return';
    }

    // either label or expression
    if (nodes.length === 1) {
        const expression = nodes[0]!;
        const printed = path.call(print, 'nonFormattingChildren', 0);
        return ['return ', expression.isBreakableExpression ? printed : indent(printed)];
    }

    // labeled expression
    if (nodes.length === 2) {
        const expression = nodes[1]!;
        const printedLabel = path.call(print, 'nonFormattingChildren', 0);
        const printedExpression = path.call(print, 'nonFormattingChildren', 1);
        return join(' ', [
            'return',
            printedLabel,
            expression.isBreakableExpression ? printedExpression : indent(printedExpression),
        ]);
    }

    throw new Error('Invalid return expression');
}
