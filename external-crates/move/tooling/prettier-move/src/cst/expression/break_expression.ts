// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { join } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'break_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printBreakExpression;
    }

    return null;
}

/**
 * Print `break_expression` node.
 */
export function printBreakExpression(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    if (path.node.nonFormattingChildren.length > 0) {
        return join(' ', ['break', ...path.map(print, 'nonFormattingChildren')]);
    }

    return 'break';
}
