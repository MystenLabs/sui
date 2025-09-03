// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
import { list } from '../../utilities';
const { group } = doc.builders;

/** The type of the node implemented in this file */
const NODE_TYPE = 'index_expression';

export default function (path: AstPath<Node>): treeFn | null {
    switch (path.node.type) {
        case NODE_TYPE:
            return printIndexExpression;
    }

    return null;
}

/**
 * Print `index_expression` node.
 */
export function printIndexExpression(
    path: AstPath<Node>,
    options: MoveOptions,
    print: printFn,
): Doc {
    return group(
        [
            path.call(print, 'nonFormattingChildren', 0), // lhs
            list({ path, options, open: '[', close: ']', print, skipChildren: 1 }),
        ],
        { shouldBreak: false },
    );
}
