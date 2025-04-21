// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

import { Node } from '../..';
import { MoveOptions, printFn, treeFn } from '../../printer';
import { AstPath, Doc, doc } from 'prettier';
const { indent, softline, group } = doc.builders;

/** The type of the node implemented in this file */
export const NODE_TYPE = 'while_expression';

export default function (path: AstPath<Node>): treeFn | null {
    if (path.node.type === NODE_TYPE) {
        return printWhileExpression;
    }

    return null;
}

/**
 * Print `while_expression` node.
 *
 * ```
 * // single line
 * while (bool_expr) expr
 *
 * // break condition
 * while (
 *	  very_long_expr &&
 *	  very_long_expr
 * ) {
 *   expr;
 * }
 * ```
 */
function printWhileExpression(path: AstPath<Node>, options: MoveOptions, print: printFn): Doc {
    const condition = path.node.nonFormattingChildren[0]!.isList
        ? [indent(softline), path.call(print, 'nonFormattingChildren', 0), softline]
        : [indent(softline), indent(path.call(print, 'nonFormattingChildren', 0)), softline];

    return [
        ['while (', group(condition), ') '],
        path.call(print, 'nonFormattingChildren', 1), // body
    ];
}
